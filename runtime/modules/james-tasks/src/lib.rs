//! James-Tasks - Task queue and execution for JAMES
//!
//! Provides task management capabilities with JSON file persistence.

use std::path::Path;
use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use james_memory::MemoryModule;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Created,
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capability: String,
    pub payload: serde_json::Value,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub dependencies: Vec<String>,
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub retries: u32,
    pub max_retries: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub assigned_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksConfig {
    pub database_path: String,
    pub max_concurrent: usize,
    pub default_timeout_secs: u64,
    pub retry_delay_secs: u64,
}

impl Default for TasksConfig {
    fn default() -> Self {
        Self {
            database_path: ".james/tasks.json".to_string(),
            max_concurrent: 10,
            default_timeout_secs: 300,
            retry_delay_secs: 60,
        }
    }
}

pub struct TasksModule {
    config: TasksConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    tasks: Arc<RwLock<Vec<Task>>>,
    _memory: Arc<MemoryModule>,
}

impl TasksModule {
    pub fn new(
        config: TasksConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        memory_module: Arc<MemoryModule>,
    ) -> Self {
        Self {
            config,
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            tasks: Arc::new(RwLock::new(Vec::new())),
            _memory: memory_module,
        }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        self.load().await?;
        info!("James-Tasks started");
        self.event_bus.publish(Event::new("module.tasks.started", "james-tasks")
            .with_payload(serde_json::json!({"database": self.config.database_path}))).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.save().await?;
        *self.running.write().await = false;
        info!("James-Tasks stopped");
        self.event_bus.publish(Event::new("module.tasks.stopped", "james-tasks")).await?;
        Ok(())
    }

    async fn load(&self) -> Result<()> {
        let path = Path::new(&self.config.database_path);
        if path.exists() {
            let raw = tokio::fs::read_to_string(path).await?;
            if !raw.trim().is_empty() {
                *self.tasks.write().await = serde_json::from_str(&raw)?;
            }
        }
        Ok(())
    }

    async fn save(&self) -> Result<()> {
        let path = Path::new(&self.config.database_path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let tasks = self.tasks.read().await.clone();
        tokio::fs::write(path, serde_json::to_string_pretty(&tasks)?).await?;
        Ok(())
    }

    pub async fn create_task(&self, mut task: Task) -> Result<String> {
        if task.id.is_empty() {
            task.id = Uuid::now_v7().to_string();
        }
        task.created_at = chrono::Utc::now();
        task.updated_at = chrono::Utc::now();

        {
            let mut tasks = self.tasks.write().await;
            tasks.push(task.clone());
        }
        self.save().await?;

        self.event_bus.publish(Event::new("task.created", "james-tasks")
            .with_payload(serde_json::json!({"task_id": task.id, "name": task.name}))).await?;

        Ok(task.id)
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.iter().find(|t| t.id == id).cloned())
    }

    pub async fn update_task_status(&self, id: &str, status: TaskStatus, result: Option<serde_json::Value>, error: Option<String>) -> Result<()> {
        let now = chrono::Utc::now();
        {
            let mut tasks = self.tasks.write().await;
            if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
                task.status = status;
                task.result = result;
                task.error = error;
                task.updated_at = now;
                if status == TaskStatus::Running && task.started_at.is_none() {
                    task.started_at = Some(now);
                }
                if status == TaskStatus::Completed || status == TaskStatus::Failed {
                    task.completed_at = Some(now);
                }
            }
        }
        self.save().await?;
        Ok(())
    }

    pub async fn list_tasks(&self, status: Option<TaskStatus>, limit: usize) -> Result<Vec<Task>> {
        let tasks = self.tasks.read().await;
        let mut result: Vec<Task> = tasks.iter()
            .filter(|t| status.map(|s| t.status == s).unwrap_or(true))
            .cloned()
            .collect();
        result.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.created_at.cmp(&b.created_at)));
        result.truncate(limit);
        Ok(result)
    }

    pub async fn queue_task(&self, id: &str) -> Result<()> {
        self.update_task_status(id, TaskStatus::Queued, None, None).await?;
        self.event_bus.publish(Event::new("task.queued", "james-tasks")
            .with_payload(serde_json::json!({"task_id": id}))).await?;
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.tasks".to_string(),
        name: "James-Tasks".to_string(),
        version: "0.1.0".to_string(),
        description: "Task queue and execution for JAMES".to_string(),
        module_type: ModuleType::Service,
        entry_point: "james_tasks".to_string(),
        capabilities: vec![
            "task.create".to_string(),
            "task.queue".to_string(),
            "task.execute".to_string(),
            "task.cancel".to_string(),
            "task.status".to_string(),
        ],
        dependencies: vec![
            james_module_host::ModuleDependency {
                name: "james.memory".to_string(),
                version: "0.1.0".to_string(),
                optional: false,
                reason: Some("Required for persistence".to_string()),
            },
        ],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "database_path": {"type": "string", "default": ".james/tasks.json"},
                "max_concurrent": {"type": "integer", "default": 10},
                "default_timeout_secs": {"type": "integer", "default": 300},
                "retry_delay_secs": {"type": "integer", "default": 60}
            }
        })),
        default_config: Some(serde_json::json!({
            "database_path": ".james/tasks.json",
            "max_concurrent": 10,
            "default_timeout_secs": 300,
            "retry_delay_secs": 60
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["tasks".to_string(), "queue".to_string(), "execution".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id, name, desc) in [
        ("task.create", "Task Create", "Create a new task"),
        ("task.queue", "Task Queue", "Queue a task for execution"),
        ("task.execute", "Task Execute", "Execute a task"),
        ("task.cancel", "Task Cancel", "Cancel a running task"),
        ("task.status", "Task Status", "Get task status"),
    ] {
        registry.register(CapabilityDefinition {
            id: id.to_string(), name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("tasks".to_string()),
            version: "1.0.0".to_string(), provider: "james.tasks".to_string(),
            description: desc.to_string(), risk_level: RiskLevel::Low,
            required_permissions: vec![], dependencies: vec![],
            input_schema: None, output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["tasks".to_string()], deprecated: false, experimental: false,
        }, "james.tasks".to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_tasks_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.tasks");
        assert_eq!(m.capabilities.len(), 5);
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}