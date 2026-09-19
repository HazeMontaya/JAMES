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

    /// Reconcile the Rust registry with the Python bridge's current tool set.
    ///
    /// Python is authoritative only for capabilities it owns (provider == "python").
    /// Core/module-owned capabilities are never overwritten or removed.
    pub async fn sync_all(&self, python_caps: &[PythonCapabilityInfo]) -> anyhow::Result<()> {
        info!("Reconciling {} Python capabilities with Rust registry", python_caps.len());

        let incoming: std::collections::HashSet<String> =
            python_caps.iter().map(|cap| cap.id.clone()).collect();

        for cap_info in python_caps {
            if let Err(e) = self.sync_one(cap_info).await {
                warn!("Failed to sync capability {}: {}", cap_info.id, e);
            }
        }

        let stale: Vec<String> = self
            .registry
            .list_by_provider("python")
            .into_iter()
            .filter(|registered| !incoming.contains(&registered.definition.id))
            .map(|registered| registered.definition.id)
            .collect();

        for capability_id in stale {
            match self.registry.unregister(&capability_id).await {
                Ok(true) => info!("Removed stale Python capability: {}", capability_id),
                Ok(false) => debug!("Python capability already absent: {}", capability_id),
                Err(e) => warn!("Failed to remove stale Python capability {}: {}", capability_id, e),
            }
        }

        Ok(())
    }

    /// Sync one capability without allowing Python to overwrite another owner.
    pub async fn sync_one(&self, cap_info: &PythonCapabilityInfo) -> anyhow::Result<()> {
        let definition = self.python_to_rust_definition(cap_info);

        match self.registry.get(&cap_info.id) {
            None => {
                self.registry.register(definition, "python-bridge").await?;
                info!("Registered Python capability: {}", cap_info.id);
            }
            Some(existing) if existing.definition.provider == "python" => {
                if existing.definition != definition {
                    self.registry.unregister(&cap_info.id).await?;
                    self.registry.register(definition, "python-bridge").await?;
                    info!("Updated Python capability contract: {}", cap_info.id);
                } else {
                    debug!("Python capability {} unchanged", cap_info.id);
                }
            }
            Some(existing) => {
                warn!(
                    "Python capability {} conflicts with owner {}; keeping existing definition",
                    cap_info.id, existing.definition.provider
                );
            }
        }

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