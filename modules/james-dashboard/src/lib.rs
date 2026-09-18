//! James-Dashboard - Administrative workspace for JAMES

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use james_ai::AiModule;
use james_chat::ChatModule;
use james_memory::{MemoryEntry, MemoryModule, MemoryQuery};
use james_scheduler::SchedulerModule;
use james_tasks::{Task, TasksModule};
use james_voice::VoiceModule;
use james_webresearch::WebResearchModule;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub title: String,
    pub port: u16,
    pub host: String,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self { title: "JAMES Dashboard".to_string(), port: 3001, host: "127.0.0.1".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub running: bool,
    pub modules_loaded: usize,
    pub capabilities_available: usize,
    pub events_published: u64,
    pub uptime_secs: u64,
}

pub struct DashboardModule {
    config: DashboardConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    memory: Arc<MemoryModule>,
    tasks: Arc<TasksModule>,
    start_time: std::time::Instant,
    event_count: Arc<RwLock<u64>>,
}

impl DashboardModule {
    pub fn new(config: DashboardConfig, event_bus: Arc<EventBus>, capability_registry: Arc<CapabilityRegistry>,
             _chat: Arc<ChatModule>, _ai: Arc<AiModule>, memory: Arc<MemoryModule>,
             tasks: Arc<TasksModule>, _web_research: Arc<WebResearchModule>,
             _voice: Arc<VoiceModule>, _scheduler: Arc<SchedulerModule>) -> Self {
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)),
             memory, tasks,
               start_time: std::time::Instant::now(), event_count: Arc::new(RwLock::new(0)) }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-Dashboard started on {}:{}", self.config.host, self.config.port);
        self.event_bus.publish(Event::new("module.dashboard.started", "james-dashboard")
            .with_payload(serde_json::json!({"host": self.config.host, "port": self.config.port}))).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-Dashboard stopped");
        self.event_bus.publish(Event::new("module.dashboard.stopped", "james-dashboard")).await?;
        Ok(())
    }

    pub async fn get_status(&self) -> SystemStatus {
        SystemStatus {
            running: *self.running.read().await,
            modules_loaded: 0, // Will be populated by orchestrator
            capabilities_available: self.capability_registry.count(),
            events_published: *self.event_count.read().await,
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }

    pub async fn get_tasks(&self) -> Result<Vec<Task>> {
        self.tasks.list_tasks(None, 100).await
    }

    pub async fn get_memory(&self) -> Result<Vec<MemoryEntry>> {
        self.memory.query(MemoryQuery {
            memory_types: None,
            query_text: None,
            tags: None,
            session_id: None,
            agent_id: None,
            limit: 100,
            min_importance: None,
            since: None,
        }).await
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.dashboard".to_string(), name: "James-Dashboard".to_string(), version: "0.1.0".to_string(),
        description: "Administrative workspace for JAMES".to_string(), module_type: ModuleType::Interface,
        entry_point: "james_dashboard".to_string(),
        capabilities: vec!["dashboard.status".to_string(), "dashboard.tasks".to_string(), "dashboard.memory".to_string()],
        dependencies: vec![
            james_module_host::ModuleDependency { name: "james.chat".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Chat".to_string()) },
            james_module_host::ModuleDependency { name: "james.ai".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("AI".to_string()) },
            james_module_host::ModuleDependency { name: "james.memory".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Memory".to_string()) },
            james_module_host::ModuleDependency { name: "james.tasks".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Tasks".to_string()) },
            james_module_host::ModuleDependency { name: "james.voice".to_string(), version: "0.1.0".to_string(), optional: true, reason: Some("Voice".to_string()) },
            james_module_host::ModuleDependency { name: "james.scheduler".to_string(), version: "0.1.0".to_string(), optional: true, reason: Some("Scheduler".to_string()) },
        ],
        permissions: vec![],
        configuration_schema: None, default_config: None,
        author: Some("JAMES Project".to_string()), homepage: None, repository: None,
        license: "MIT".to_string(), tags: vec!["dashboard".to_string(), "admin".to_string(), "ui".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id, name, desc) in [
        ("dashboard.status", "Dashboard Status", "System status"),
        ("dashboard.tasks", "Dashboard Tasks", "Task management"),
        ("dashboard.memory", "Dashboard Memory", "Memory view"),
    ] {
        registry.register(CapabilityDefinition {
            id: id.to_string(), name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("dashboard".to_string()),
            version: "1.0.0".to_string(), provider: "james.dashboard".to_string(),
            description: desc.to_string(), risk_level: RiskLevel::Low,
            required_permissions: vec![], dependencies: vec![],
            input_schema: None, output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["dashboard".to_string()], deprecated: false, experimental: false,
        }, "james.dashboard".to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_dashboard_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.dashboard");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}