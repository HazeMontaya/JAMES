use james_app_api::{preview_bind_allowed, serve, AppState, DashboardSnapshot};
use james_core::{init_tracing, CoreConfig, JamesCore, LogFields};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::signal;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let log = LogFields::new("james-app");
    info!("{} Starting JAMES App v{}", log.prefix(), env!("CARGO_PKG_VERSION"));

    let mut config = CoreConfig::load().unwrap_or_else(|e| {
        error!("{} Failed to load config: {}, using defaults", log.prefix(), e);
        CoreConfig::default()
    });

    if let Err(errors) = config.validate() {
        error!(
            "{} Invalid configuration ({} problem(s)), using defaults: {}",
            log.prefix(),
            errors.len(),
            errors.join("; ")
        );
        config = CoreConfig::default();
    }

    if let Err(e) = config.save() {
        error!("{} Failed to save config: {}", log.prefix(), e);
    }

    // Single construction path: JamesCore::new wires all components.
    // (No manual field assignment, no duplicate initialization.)
    let mut core = JamesCore::new(config).await?;
    core.start().await?;

    // Capture shared handles before spawning dashboard refresh (JamesCore is
    // not Clone).
    let event_bus = core.event_bus();
    let capability_registry = core.capability_registry();
    let started_at = Instant::now();

    let initial_status = core.status().await;
    let state = AppState {
        bus: event_bus.clone(),
        status: Arc::new(RwLock::new(initial_status.clone())),
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at: chrono::Utc::now(),
        static_dir: "interfaces/void".to_string(),
        dashboard: Arc::new(RwLock::new(DashboardSnapshot {
            core_status: initial_status.as_str().to_string(),
            ..Default::default()
        })),
        preview_unauthenticated: true,
    };

    let bind = "127.0.0.1";
    if !preview_bind_allowed(bind) {
        anyhow::bail!("refusing to bind {bind}: no auth yet, loopback only (A7b closes this)");
    }
    warn!(
        "{} UNAUTHENTICATED local preview (no bearer auth yet — lands in A7b). Loopback only.",
        log.prefix()
    );

    let dashboard = state.dashboard.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let mut snap = dashboard.write().await;
            snap.capabilities = capability_registry.count();
            snap.events_dropped = event_bus.dropped_count();
            snap.uptime_secs = started_at.elapsed().as_secs();
        }
    });

    let void_listener = tokio::net::TcpListener::bind((bind, 38241)).await?;
    info!(
        "{} Void transport listening on http://{}:{} (preview, unauthenticated in A7b)",
        log.prefix(), bind, 38241
    );
    let void_handle = tokio::spawn(serve(void_listener, state.clone()));

    let dashboard_listener = tokio::net::TcpListener::bind((bind, 38242)).await?;
    info!(
        "{} Dashboard transport listening on http://{}:{}",
        log.prefix(), bind, 38242
    );
    let dashboard_handle = tokio::spawn(serve(dashboard_listener, state));

    info!("{} JAMES Core running. Press Ctrl+C to stop.", log.prefix());
    signal::ctrl_c().await?;

    info!("{} Shutdown signal received", log.prefix());
    void_handle.abort();
    dashboard_handle.abort();
    core.stop().await?;

    info!("{} JAMES Core stopped", log.prefix());
    Ok(())
}