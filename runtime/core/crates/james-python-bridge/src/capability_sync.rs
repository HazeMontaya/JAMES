//! Capability synchronization between Python skills and Rust registry

use james_capabilities::{CapabilityDefinition, CapabilityCategory, ExecutionTarget, RiskLevel, CapabilityRegistry};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::nats_bridge::PythonCapabilityInfo;

/// Sync Python capabilities to Rust registry
#[derive(Clone)]
pub struct CapabilitySync {
    registry: Arc<CapabilityRegistry>,
}

impl CapabilitySync {
    pub fn new(registry: Arc<CapabilityRegistry>) -> Self {
        Self { registry }
    }

    /// Sync all Python capabilities to Rust registry
    pub async fn sync_all(&self, python_caps: &[PythonCapabilityInfo]) -> anyhow::Result<()> {
        info!("Syncing {} Python capabilities to Rust registry", python_caps.len());

        for cap_info in python_caps {
            if let Err(e) = self.sync_one(cap_info).await {
                warn!("Failed to sync capability {}: {}", cap_info.id, e);
            }
        }

        Ok(())
    }

    /// Sync a single Python capability
    pub async fn sync_one(&self, cap_info: &PythonCapabilityInfo) -> anyhow::Result<()> {
        // Check if already registered
        if self.registry.get(&cap_info.id).is_some() {
            debug!("Capability {} already registered, skipping", cap_info.id);
            return Ok(());
        }

        let definition = self.python_to_rust_definition(cap_info);
        self.registry.register(definition, "python-bridge").await?;

        info!("Registered Python capability in Rust: {}", cap_info.id);
        Ok(())
    }

    /// Convert Python capability info to Rust CapabilityDefinition
    fn python_to_rust_definition(&self, cap_info: &PythonCapabilityInfo) -> CapabilityDefinition {
        let category = match cap_info.category.to_lowercase().as_str() {
            "system" => CapabilityCategory::System,
            "file" => CapabilityCategory::File,
            "process" => CapabilityCategory::Process,
            "network" => CapabilityCategory::Network,
            "browser" => CapabilityCategory::Browser,
            "voice" => CapabilityCategory::Voice,
            "ai" => CapabilityCategory::Ai,
            "device" => CapabilityCategory::Device,
            "smarthome" | "smart_home" | "smart-home" => CapabilityCategory::SmartHome,
            "automation" => CapabilityCategory::Automation,
            "database" => CapabilityCategory::Database,
            "security" => CapabilityCategory::Security,
            "business" => CapabilityCategory::Custom("business".to_string()),
            "research" => CapabilityCategory::Custom("research".to_string()),
            _ => CapabilityCategory::Custom(cap_info.category.clone()),
        };

        let risk_level = match cap_info.risk_level.to_ascii_lowercase().as_str() {
            "critical" => RiskLevel::Critical,
            "high" => RiskLevel::High,
            "medium" => RiskLevel::Medium,
            _ => RiskLevel::Low,
        };

        CapabilityDefinition {
            id: cap_info.id.clone(),
            name: cap_info.name.clone(),
            category,
            version: cap_info.version.clone(),
            provider: "python".to_string(),
            description: cap_info.description.clone(),
            risk_level,
            required_permissions: cap_info.required_permissions.clone(),
            dependencies: vec![],
            input_schema: cap_info.input_schema.clone(),
            output_schema: cap_info.output_schema.clone(),
            execution_target: ExecutionTarget::Local,
            tags: vec!["python".to_string(), "bridge".to_string()],
            deprecated: false,
            experimental: true, // Python bridge is experimental
        }
    }
}

/// Python capability executor that forwards to Python via NATS
pub struct PythonCapabilityExecutor {
    nats_bridge: crate::nats_bridge::NatsBridge,
}

impl PythonCapabilityExecutor {
    pub fn new(nats_bridge: crate::nats_bridge::NatsBridge) -> Self {
        Self { nats_bridge }
    }
}

#[async_trait::async_trait]
impl james_capability_broker::CapabilityExecutor for PythonCapabilityExecutor {
    async fn execute(
        &self,
        capability_id: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        debug!("Executing Python capability via bridge: {}", capability_id);

        // Use a default caller for bridge executions
        let response = self.nats_bridge
            .execute_capability(capability_id, "python-bridge", input)
            .await?;

        if !response.success {
            return Err(anyhow::anyhow!(
                "Python capability execution failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        response.output.ok_or_else(|| anyhow::anyhow!("No output from Python capability"))
    }
}

/// Background task to periodically sync capabilities
pub async fn start_capability_sync_task(
    sync: Arc<CapabilitySync>,
    nats_bridge: Arc<crate::nats_bridge::NatsBridge>,
    interval_secs: u64,
) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

    loop {
        interval.tick().await;
        let caps = nats_bridge.list_python_capabilities().await;
        if let Err(e) = sync.sync_all(&caps).await {
            warn!("Capability sync failed: {}", e);
        }
    }
}