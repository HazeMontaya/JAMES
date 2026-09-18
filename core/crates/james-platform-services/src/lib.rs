//! JAMES Platform Services — bridges platform ports into the Capability Broker.
//!
//! This crate adapts the platform abstraction into broker "capabilities"
//! (`platform.*`) so the enforced execution chain can pass through real
//! platform operations (filesystem, process, power, network, system info, data
//! directories). The Windows adapter (or any future adapter) is supplied as an
//! `Arc<dyn Platform>`.
//!
//! Capability IDs:
//! - `platform.descriptor`, `platform.features`
//! - `platform.system.info`, `platform.system.uptime`
//! - `platform.power.status`
//! - `platform.process.spawn|list|get|kill|wait`
//! - `platform.fs.read|read_text|write|write_text|append_text|exists|stat|list|remove|rename|is_dir`
//! - `platform.network.interfaces|online|reachable`
//! - `platform.data_dir.root|area|ensure`

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use james_capability_broker::CapabilityExecutor;
use james_capabilities::{CapabilityCategory, CapabilityDefinition, ExecutionTarget, RiskLevel};
use james_platform::{
    DataArea, DataDirectoryPort, FeatureDetectionPort, FileSystemPort, NetworkPort, Platform,
    PlatformError, PowerPort, ProcessPort, SpawnRequest, SystemInfoPort,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub mod bootstrap;

/// Dispatches capability requests to platform ports.
pub struct PlatformCapabilityExecutor {
    platform: Arc<dyn Platform>,
}

impl PlatformCapabilityExecutor {
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        Self { platform }
    }

    pub fn platform(&self) -> &Arc<dyn Platform> {
        &self.platform
    }
}

/// Registry of `platform.*` capability ids.
pub mod ids {
    pub const DESCRIPTOR: &str = "platform.descriptor";
    pub const FEATURES: &str = "platform.features";
    pub const SYSTEM_INFO: &str = "platform.system.info";
    pub const SYSTEM_UPTIME: &str = "platform.system.uptime";
    pub const POWER_STATUS: &str = "platform.power.status";
    pub const PROCESS_SPAWN: &str = "platform.process.spawn";
    pub const PROCESS_LIST: &str = "platform.process.list";
    pub const PROCESS_GET: &str = "platform.process.get";
    pub const PROCESS_KILL: &str = "platform.process.kill";
    pub const PROCESS_WAIT: &str = "platform.process.wait";
    pub const FS_READ: &str = "platform.fs.read";
    pub const FS_READ_TEXT: &str = "platform.fs.read_text";
    pub const FS_WRITE: &str = "platform.fs.write";
    pub const FS_WRITE_TEXT: &str = "platform.fs.write_text";
    pub const FS_APPEND_TEXT: &str = "platform.fs.append_text";
    pub const FS_EXISTS: &str = "platform.fs.exists";
    pub const FS_IS_DIR: &str = "platform.fs.is_dir";
    pub const FS_STAT: &str = "platform.fs.stat";
    pub const FS_LIST: &str = "platform.fs.list";
    pub const FS_REMOVE: &str = "platform.fs.remove";
    pub const FS_RENAME: &str = "platform.fs.rename";
    pub const NET_INTERFACES: &str = "platform.network.interfaces";
    pub const NET_ONLINE: &str = "platform.network.online";
    pub const NET_REACHABLE: &str = "platform.network.reachable";
    pub const DATA_ROOT: &str = "platform.data_dir.root";
    pub const DATA_AREA: &str = "platform.data_dir.area";
    pub const DATA_ENSURE: &str = "platform.data_dir.ensure";

    pub const ALL: &[&str] = &[
        DESCRIPTOR,
        FEATURES,
        SYSTEM_INFO,
        SYSTEM_UPTIME,
        POWER_STATUS,
        PROCESS_SPAWN,
        PROCESS_LIST,
        PROCESS_GET,
        PROCESS_KILL,
        PROCESS_WAIT,
        FS_READ,
        FS_READ_TEXT,
        FS_WRITE,
        FS_WRITE_TEXT,
        FS_APPEND_TEXT,
        FS_EXISTS,
        FS_IS_DIR,
        FS_STAT,
        FS_LIST,
        FS_REMOVE,
        FS_RENAME,
        NET_INTERFACES,
        NET_ONLINE,
        NET_REACHABLE,
        DATA_ROOT,
        DATA_AREA,
        DATA_ENSURE,
    ];
}

/// Register all `platform.*` capabilities in a registry.
pub async fn register_capabilities(
    registry: &james_capabilities::CapabilityRegistry,
    provider: &str,
) {
    for id in ids::ALL {
        let definition = CapabilityDefinition {
            id: id.to_string(),
            name: id.to_string(),
            category: CapabilityCategory::System,
            version: "1.0.0".to_string(),
            provider: provider.to_string(),
            description: format!("Platform operation ({id})"),
            risk_level: RiskLevel::Medium,
            required_permissions: vec![id.to_string()],
            dependencies: vec![],
            input_schema: None,
            output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["platform".to_string()],
            deprecated: false,
            experimental: false,
        };
        registry
            .register(definition, provider)
            .await
            .expect("platform.register");
    }
}

// ---- Input shapes ----------------------------------------------------------

#[derive(Deserialize)]
struct FsPathInput {
    path: String,
}

#[derive(Deserialize)]
struct FsWriteInput {
    path: String,
    contents: String,
}

#[derive(Deserialize)]
struct FsAppendInput {
    path: String,
    contents: String,
}

#[derive(Deserialize)]
struct FsRenameInput {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct FsRemoveInput {
    path: String,
}

fn perr(id: &str, e: PlatformError) -> anyhow::Error {
    anyhow!("{id} failed: {e}")
}

#[async_trait]
impl CapabilityExecutor for PlatformCapabilityExecutor {
    async fn execute(&self, capability_id: &str, input: Value) -> Result<Value> {
        let p = self.platform.as_ref();
        match capability_id {
            ids::DESCRIPTOR => Ok(json!(Platform::descriptor(p))),
            ids::FEATURES => {
                let features = FeatureDetectionPort::features(p)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!(features))
            }
            ids::SYSTEM_INFO => {
                let info = SystemInfoPort::system_info(p)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!(info))
            }
            ids::SYSTEM_UPTIME => {
                let uptime = SystemInfoPort::uptime_secs(p)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "uptime_secs": uptime }))
            }
            ids::POWER_STATUS => {
                let status =
                    PowerPort::status(p).await.map_err(|e| perr(capability_id, e))?;
                Ok(json!(status))
            }

            ids::PROCESS_SPAWN => {
                #[derive(Deserialize)]
                struct SpawnInput {
                    program: String,
                    #[serde(default)]
                    args: Vec<String>,
                    working_dir: Option<String>,
                    #[serde(default)]
                    env: Vec<(String, String)>,
                    capture_output: Option<bool>,
                    timeout_ms: Option<u64>,
                }
                let req: SpawnInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let result = ProcessPort::spawn(
                    p,
                    SpawnRequest {
                        program: req.program,
                        args: req.args,
                        working_dir: req.working_dir,
                        env: req.env,
                        capture_output: req.capture_output.unwrap_or(false),
                        timeout_ms: req.timeout_ms,
                    },
                )
                .await
                .map_err(|e| perr(capability_id, e))?;
                Ok(json!(result))
            }
            ids::PROCESS_LIST => {
                let list = ProcessPort::list(p).await.map_err(|e| perr(capability_id, e))?;
                Ok(json!(list))
            }
            ids::PROCESS_GET => {
                #[derive(Deserialize)]
                struct PidInput {
                    pid: u32,
                }
                let req: PidInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let info = ProcessPort::get(p, req.pid)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!(info))
            }
            ids::PROCESS_KILL => {
                #[derive(Deserialize)]
                struct PidInput {
                    pid: u32,
                }
                let req: PidInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                ProcessPort::kill(p, req.pid)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "killed": true, "pid": req.pid }))
            }
            ids::PROCESS_WAIT => {
                #[derive(Deserialize)]
                struct WaitInput {
                    pid: u32,
                    #[serde(default = "default_wait_ms")]
                    timeout_ms: u64,
                }
                fn default_wait_ms() -> u64 {
                    30_000
                }
                let req: WaitInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let code = ProcessPort::wait(p, req.pid, req.timeout_ms)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "pid": req.pid, "exit_code": code, "timed_out": code.is_none() }))
            }

            ids::FS_READ => {
                let req: FsPathInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let bytes = FileSystemPort::read(p, &req.path)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({
                    "path": req.path,
                    "size_bytes": bytes.len(),
                    "data_base64": base64(&bytes),
                }))
            }
            ids::FS_READ_TEXT => {
                let req: FsPathInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let text = FileSystemPort::read_text(p, &req.path)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "path": req.path, "contents": text }))
            }
            ids::FS_WRITE | ids::FS_WRITE_TEXT => {
                let req: FsWriteInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                FileSystemPort::write_text(p, &req.path, &req.contents)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "path": req.path, "wrote_text": true }))
            }
            ids::FS_APPEND_TEXT => {
                let req: FsAppendInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                FileSystemPort::append_text(p, &req.path, &req.contents)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "path": req.path, "appended": true }))
            }
            ids::FS_EXISTS => {
                let req: FsPathInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let exists = FileSystemPort::exists(p, &req.path).await;
                Ok(json!({ "path": req.path, "exists": exists }))
            }
            ids::FS_IS_DIR => {
                let req: FsPathInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let is_dir = FileSystemPort::is_dir(p, &req.path).await;
                Ok(json!({ "path": req.path, "is_dir": is_dir }))
            }
            ids::FS_STAT => {
                let req: FsPathInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let entry = FileSystemPort::stat(p, &req.path)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!(entry))
            }
            ids::FS_LIST => {
                let req: FsPathInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let listing = FileSystemPort::list(p, &req.path)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!(listing))
            }
            ids::FS_REMOVE => {
                let req: FsRemoveInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                FileSystemPort::remove(p, &req.path)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "path": req.path, "removed": true }))
            }
            ids::FS_RENAME => {
                let req: FsRenameInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                FileSystemPort::rename(p, &req.from, &req.to)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "from": req.from, "to": req.to }))
            }

            ids::NET_INTERFACES => {
                let ifaces = NetworkPort::interfaces(p)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!(ifaces))
            }
            ids::NET_ONLINE => {
                let online = NetworkPort::is_online(p)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "online": online }))
            }
            ids::NET_REACHABLE => {
                #[derive(Deserialize)]
                struct ReachInput {
                    host: String,
                    timeout_ms: Option<u64>,
                }
                let req: ReachInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let reachable = NetworkPort::reachable(p, &req.host, req.timeout_ms.unwrap_or(3_000))
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "host": req.host, "reachable": reachable }))
            }

            ids::DATA_ROOT => {
                let root = DataDirectoryPort::root(p)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "root": root.to_string_lossy().to_string() }))
            }
            ids::DATA_AREA => {
                #[derive(Deserialize)]
                struct AreaInput {
                    area: DataArea,
                }
                let req: AreaInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let dir = DataDirectoryPort::area(p, req.area)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!(dir))
            }
            ids::DATA_ENSURE => {
                #[derive(Deserialize)]
                struct AreaInput {
                    area: DataArea,
                }
                let req: AreaInput =
                    serde_json::from_value(input).map_err(|e| anyhow!("invalid input: {e}"))?;
                let path = DataDirectoryPort::ensure(p, req.area)
                    .await
                    .map_err(|e| perr(capability_id, e))?;
                Ok(json!({ "area": req.area.as_str(), "path": path.to_string_lossy().to_string() }))
            }

            other => Err(anyhow!("unknown platform capability: {other}")),
        }
    }
}

/// Minimal base64 encoder to fold binary payloads into JSON (no external dep).
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_capabilities::CapabilityRegistry;
    use james_platform::doubles::MemoryPlatform;

    async fn test_executor() -> (PlatformCapabilityExecutor, CapabilityRegistry) {
        let platform: Arc<dyn Platform> = Arc::new(MemoryPlatform::default());
        let executor = PlatformCapabilityExecutor::new(platform);
        let registry = CapabilityRegistry::new();
        register_capabilities(&registry, "platform-svc").await;
        (executor, registry)
    }

    #[tokio::test]
    async fn test_descriptor_capability() {
        let (ex, _) = test_executor().await;
        let out = ex.execute(ids::DESCRIPTOR, json!({})).await.unwrap();
        assert_eq!(out["name"], "memory-platform");
    }

    #[tokio::test]
    async fn test_features_capability() {
        let (ex, _) = test_executor().await;
        let out = ex.execute(ids::FEATURES, json!({})).await.unwrap();
        assert!(out.is_array());
        assert!(!out.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_system_info_capability() {
        let (ex, _) = test_executor().await;
        let out = ex.execute(ids::SYSTEM_INFO, json!({})).await.unwrap();
        assert!(!out["os_name"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_fs_text_capabilities() {
        let (ex, _) = test_executor().await;
        let path = "mem://notes/test.txt";

        let out = ex
            .execute(ids::FS_WRITE_TEXT, json!({ "path": path, "contents": "hello" }))
            .await
            .unwrap();
        assert_eq!(out["wrote_text"], true);

        let out = ex
            .execute(ids::FS_READ_TEXT, json!({ "path": path }))
            .await
            .unwrap();
        assert_eq!(out["contents"], "hello");

        let out = ex.execute(ids::FS_EXISTS, json!({ "path": path })).await.unwrap();
        assert_eq!(out["exists"], true);

        let out = ex.execute(ids::FS_LIST, json!({ "path": "mem://notes" })).await.unwrap();
        assert_eq!(out["entries"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_process_capabilities() {
        let (ex, _) = test_executor().await;
        let out = ex
            .execute(
                ids::PROCESS_LIST,
                json!({}),
            )
            .await
            .unwrap();
        assert!(out.is_array());
    }

    #[tokio::test]
    async fn test_power_and_data_capabilities() {
        let (ex, _) = test_executor().await;
        let out = ex.execute(ids::POWER_STATUS, json!({})).await.unwrap();
        assert!(out["on_ac"].is_boolean());

        let out = ex
            .execute(ids::DATA_ENSURE, json!({ "area": "audit" }))
            .await
            .unwrap();
        assert_eq!(out["area"], "audit");
    }

    #[tokio::test]
    async fn test_unknown_capability_errors() {
        let (ex, _) = test_executor().await;
        let err = ex.execute("platform.nope", json!({})).await.unwrap_err();
        assert!(err.to_string().contains("unknown platform capability"));
    }

    #[tokio::test]
    async fn test_registered_capabilities() {
        let (_, registry) = test_executor().await;
        for id in ids::ALL {
            assert!(
                registry.get(id).is_some(),
                "capability {id} should be registered"
            );
        }
    }

    #[test]
    fn test_base64_roundtrip() {
        assert_eq!(base64(b"hello world"), "aGVsbG8gd29ybGQ=");
        assert_eq!(base64(b"a"), "YQ==");
    }
}