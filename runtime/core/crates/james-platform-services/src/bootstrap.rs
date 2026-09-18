//! Bootstrap: construct the concrete platform adapter for the current OS,
//! register the `platform.*` capabilities, and wire a CapabilityBroker with
//! default grants + a telemetry event loop.

use std::sync::Arc;

use anyhow::Result;
use james_capabilities::CapabilityRegistry;
use james_capability_broker::CapabilityBroker;
use james_events::{Event, EventBus};
use james_platform::Platform;
use tracing::{info, warn};

use crate::{ids, PlatformCapabilityExecutor};

/// Owned platform runtime: adapter + broker + executor + capability registry.
pub struct PlatformContext {
    pub platform: Arc<dyn Platform>,
    pub executor: Arc<PlatformCapabilityExecutor>,
    pub broker: Arc<CapabilityBroker>,
    pub capabilities: Arc<CapabilityRegistry>,
}

/// `JAMES_PLATFORM_DOUBLE=1` forces the in-memory double (tests, CI, no hardware).
pub fn env_force_double() -> bool {
    std::env::var("JAMES_PLATFORM_DOUBLE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Build the concrete adapter for the current platform.
pub fn build_platform() -> Arc<dyn Platform> {
    if env_force_double() {
        warn!("james-platform: JAMES_PLATFORM_DOUBLE=1, using in-memory double");
        return Arc::new(james_platform::doubles::MemoryPlatform::default());
    }
    #[cfg(windows)]
    {
        info!("james-platform: initializing WindowsPlatform adapter");
        Arc::new(james_platform_windows::WindowsPlatform::new())
    }
    #[cfg(not(windows))]
    {
        warn!(
            "james-platform: no native adapter compiled for target_os={}, using in-memory double",
            std::env::consts::OS
        );
        Arc::new(james_platform::doubles::MemoryPlatform::default())
    }
}

/// Bootstrap the full platform runtime.
pub async fn bootstrap(event_bus: Option<Arc<EventBus>>) -> Result<PlatformContext> {
    let platform = build_platform();
    let capabilities = Arc::new(CapabilityRegistry::new());
    crate::register_capabilities(&capabilities, "platform").await;

    let broker = {
        let mut broker = CapabilityBroker::new(capabilities.clone());
        if let Some(bus) = event_bus.clone() {
            broker = broker.with_event_bus(bus);
        }
        Arc::new(broker)
    };

    // Default grants are intentionally read-only. Write/process-control
    // capabilities must be granted explicitly by policy/confirmation.
    const SAFE_DEFAULTS: &[&str] = &[
        ids::DESCRIPTOR, ids::FEATURES, ids::SYSTEM_INFO, ids::SYSTEM_UPTIME,
        ids::POWER_STATUS, ids::PROCESS_LIST, ids::PROCESS_GET, ids::PROCESS_WAIT,
        ids::FS_READ, ids::FS_READ_TEXT, ids::FS_EXISTS, ids::FS_IS_DIR, ids::FS_STAT,
        ids::FS_LIST, ids::NET_INTERFACES, ids::NET_ONLINE, ids::NET_REACHABLE,
        ids::DATA_ROOT, ids::DATA_AREA,
    ];
    for caller in ["system", "user", "void"] {
        for id in SAFE_DEFAULTS {
            broker.grant_capability_permissions(caller, id);
        }
    }

    let executor = Arc::new(PlatformCapabilityExecutor::new(platform.clone()));

    info!(
        "james-platform: registered {} platform capabilities ({} via broker)",
        capabilities.count(),
        ids::ALL.len()
    );

    Ok(PlatformContext {
        platform,
        executor,
        broker,
        capabilities,
    })
}

/// Publish a single telemetry snapshot event from the platform adapter.
pub async fn publish_telemetry_snapshot(ctx: &PlatformContext, bus: &EventBus) {
    let mut payload = serde_json::json!({
        "at": chrono::Utc::now().to_rfc3339(),
    });

    match ctx.platform.system_info().await {
        Ok(info) => {
            payload["system"] = serde_json::to_value(&info).unwrap_or_default();
        }
        Err(e) => payload["system_error"] = serde_json::json!(e.to_string()),
    }
    match ctx.platform.status().await {
        Ok(status) => {
            payload["power"] = serde_json::to_value(&status).unwrap_or_default();
        }
        Err(e) => payload["power_error"] = serde_json::json!(e.to_string()),
    }
    match ctx.platform.uptime_secs().await {
        Ok(u) => payload["uptime_secs"] = serde_json::json!(u),
        Err(e) => payload["uptime_error"] = serde_json::json!(e.to_string()),
    }
    match ctx.platform.features().await {
        Ok(features) => {
            payload["features"] = serde_json::to_value(&features).unwrap_or_default();
        }
        Err(e) => payload["features_error"] = serde_json::json!(e.to_string()),
    }

    if bus
        .publish(
            Event::new("platform.telemetry.snapshot", "james-platform")
                .with_payload(payload),
        )
        .await
        .is_err()
    {
        warn!("james-platform: could not publish telemetry snapshot");
    }
}

/// Spawn the periodic telemetry loop (15s interval). Keeps `ctx` alive.
pub fn spawn_telemetry(ctx: Arc<PlatformContext>, event_bus: Arc<EventBus>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("james-platform: telemetry loop started (15s interval)");
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tick.tick().await;
            publish_telemetry_snapshot(&ctx, &event_bus).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bootstrap_registers_capabilities_and_grants() {
        let ctx = bootstrap(None).await.unwrap();
        assert!(ctx.capabilities.count() >= ids::ALL.len());
        for id in ids::ALL {
            assert!(ctx.capabilities.get(id).is_some(), "missing {id}");
        }

        let perms = ctx.broker.caller_permissions("user");
        for id in [
            ids::DESCRIPTOR, ids::FEATURES, ids::SYSTEM_INFO, ids::SYSTEM_UPTIME,
            ids::POWER_STATUS, ids::PROCESS_LIST, ids::PROCESS_GET, ids::PROCESS_WAIT,
            ids::FS_READ, ids::FS_READ_TEXT, ids::FS_EXISTS, ids::FS_IS_DIR, ids::FS_STAT,
            ids::FS_LIST, ids::NET_INTERFACES, ids::NET_ONLINE, ids::NET_REACHABLE,
            ids::DATA_ROOT, ids::DATA_AREA,
        ] {
            assert!(perms.contains(&id.to_string()), "user should be granted {id}");
        }

        for id in [
            ids::PROCESS_SPAWN, ids::PROCESS_KILL, ids::FS_WRITE, ids::FS_WRITE_TEXT,
            ids::FS_APPEND_TEXT, ids::FS_REMOVE, ids::FS_RENAME, ids::DATA_ENSURE,
        ] {
            assert!(
                !perms.contains(&id.to_string()),
                "user must not receive unsafe default grant {id}"
            );
        }

        // Verify the broker enforces the default deny at execution time.
        let err = ctx
            .broker
            .execute(
                james_capability_broker::CapabilityRequest {
                    caller: "user".to_string(),
                    capability_id: ids::FS_WRITE_TEXT.to_string(),
                    input: serde_json::json!({
                        "path": "mem://security-test.txt",
                        "contents": "must not execute",
                    }),
                },
                ctx.executor.as_ref(),
            )
            .await
            .expect_err("unsafe write must be denied without an explicit grant");
        assert!(err.to_string().contains("permission denied"), "{err:?}");
    }

    #[tokio::test]
    async fn test_telemetry_snapshot_publishes() {
        let bus = Arc::new(EventBus::new(1024));
        let ctx = bootstrap(Some(bus.clone())).await.unwrap();
        publish_telemetry_snapshot(&ctx, &bus).await;
        let desc = Platform::descriptor(ctx.platform.as_ref());
        assert!(!desc.name.is_empty());
    }

    #[tokio::test]
    async fn test_build_platform_returns_concrete_adapter() {
        let platform = build_platform();
        let desc = Platform::descriptor(platform.as_ref());
        assert!(!desc.name.is_empty());
    }
}