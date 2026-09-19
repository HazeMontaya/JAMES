//! James-Tasks module facade.
//!
//! Canonical task state lives in the core james-tasks::TaskManager.
//! This module owns only the public module DTO/configuration boundary.

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use james_memory::MemoryModule;
use james_tasks_core::{RetryPolicy as CoreRetryPolicy, Task as CoreTask, TaskManager, TaskStatus as CoreTaskStatus};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

pub use james_tasks_core::{RetryPolicy, TaskPriority, TaskStatus};

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
    manager: Arc<TaskManager>,
    _memory: Arc<MemoryModule>,
}

impl TasksModule {
    pub fn new(
        config: TasksConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        memory_module: Arc<MemoryModule>,
    ) -> Self {
        let manager = Arc::new(
            TaskManager::new(Some(event_bus.clone()))
                .with_capability_registry(capability_registry.clone())
                .with_max_concurrent(config.max_concurrent),
        );
        Self {
            config,
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            manager,
            _memory: memory_module,
        }
    }

    pub fn from_manager(
        config: TasksConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        memory_module: Arc<MemoryModule>,
        manager: Arc<TaskManager>,
    ) -> Self {
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)), manager, _memory: memory_module }
    }

    pub fn manager(&self) -> Arc<TaskManager> { self.manager.clone() }

    pub async fn start(&self) -> Result<()> {
        self.manager.start().await?;
        self.manager.load(&self.config.database_path).await?;
        *self.running.write().await = true;
        info!("James-Tasks facade started");
        self.event_bus.publish(Event::new("module.tasks.started", "james-tasks")
            .with_payload(serde_json::json!({"database": self.config.database_path}))).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.manager.save(&self.config.database_path).await?;
        self.manager.stop().await?;
        *self.running.write().await = false;
        self.event_bus.publish(Event::new("module.tasks.stopped", "james-tasks")).await?;
        Ok(())
    }

    pub async fn create_task(&self, task: Task) -> Result<String> {
        let core = to_core(task)?;
        let id = self.manager.create_task(core).await?;
        Ok(id.to_string())
    }

    pub async fn get_task(&self, id: &str) -> Result<Option<Task>> {
        let id = Uuid::parse_str(id)?;
        Ok(self.manager.get_task(id).map(from_core))
    }

    pub async fn update_task_status(&self, id: &str, status: TaskStatus, result: Option<serde_json::Value>, error: Option<String>) -> Result<()> {
        let id = Uuid::parse_str(id)?;
        if let Some(result) = result {
            self.manager.set_result(id, result).await?;
        } else if let Some(error) = error {
            self.manager.set_error(id, error).await?;
        } else {
            self.manager.update_status(id, status).await?;
        }
        Ok(())
    }

    pub async fn list_tasks(&self, status: Option<TaskStatus>, limit: usize) -> Result<Vec<Task>> {
        let mut tasks = match status {
            Some(s) => self.manager.list_by_status(s),
            None => self.manager.list_tasks(),
        };
        tasks.sort_by(|a,b| b.priority.cmp(&a.priority).then_with(|| a.created_at.cmp(&b.created_at)));
        tasks.truncate(limit);
        Ok(tasks.into_iter().map(from_core).collect())
    }

    pub async fn queue_task(&self, id: &str) -> Result<()> {
        let id = Uuid::parse_str(id)?;
        self.manager.update_status(id, CoreTaskStatus::Queued).await?;
        Ok(())
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

fn to_core(t: Task) -> Result<CoreTask> {
    let id = if t.id.is_empty() { Uuid::nil() } else { Uuid::parse_str(&t.id)? };
    let dependencies = t.dependencies.into_iter().map(|x| Uuid::parse_str(&x)).collect::<Result<Vec<_>,_>>()?;
    let retry_policy = CoreRetryPolicy { max_retries: t.max_retries, base_delay_secs: 1, max_delay_secs: 60, exponential_backoff: true, retry_on: vec!["timeout".into(), "error".into()] };
    Ok(CoreTask {
        id,
        task_type: t.capability.clone(),
        name: t.name,
        priority: t.priority,
        status: t.status,
        created_at: t.created_at,
        started_at: t.started_at,
        completed_at: t.completed_at,
        source: "james.tasks".into(),
        dependencies,
        timeout_secs: 300,
        retry_policy,
        current_retry: t.retries,
        payload: t.payload,
        result: t.result,
        error: t.error,
        assigned_worker: t.assigned_agent,
        progress: None,
    })
}

fn from_core(t: CoreTask) -> Task {
    Task {
        id: t.id.to_string(),
        name: t.name,
        description: String::new(),
        capability: t.task_type,
        payload: t.payload,
        priority: t.priority,
        status: t.status,
        dependencies: t.dependencies.into_iter().map(|x| x.to_string()).collect(),
        scheduled_at: None,
        started_at: t.started_at,
        completed_at: t.completed_at,
        result: t.result,
        error: t.error,
        retries: t.current_retry,
        max_retries: t.retry_policy.max_retries,
        created_at: t.created_at,
        updated_at: t.completed_at.unwrap_or(t.created_at),
        assigned_agent: t.assigned_worker,
    }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.tasks".into(), name: "James-Tasks".into(), version: "0.1.0".into(),
        description: "Task queue facade over the canonical core task manager".into(),
        module_type: ModuleType::Service, entry_point: "james_tasks".into(),
        capabilities: vec!["task.create".into(),"task.queue".into(),"task.execute".into(),"task.cancel".into(),"task.status".into()],
        dependencies: vec![james_module_host::ModuleDependency { name: "james.memory".into(), version: "0.1.0".into(), optional: false, reason: Some("Task integration".into()) }],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({"type":"object","properties":{"database_path":{"type":"string","default":".james/tasks.json"},"max_concurrent":{"type":"integer","default":10}}})),
        default_config: Some(serde_json::json!({"database_path":".james/tasks.json","max_concurrent":10})),
        author: Some("JAMES Project".into()), homepage: None, repository: None, license: "MIT".into(),
        tags: vec!["tasks".into(),"queue".into()], min_core_version: "0.1.0".into(),
        platforms: vec!["windows".into(),"linux".into(),"macos".into()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id,name,desc) in [("task.create","Task Create","Create a new task"),("task.queue","Task Queue","Queue a task for execution"),("task.execute","Task Execute","Execute a task"),("task.cancel","Task Cancel","Cancel a running task"),("task.status","Task Status","Get task status")] {
        registry.register(CapabilityDefinition { id:id.into(), name:name.into(), category:james_capabilities::CapabilityCategory::Custom("tasks".into()), version:"1.0.0".into(), provider:"james.tasks".into(), description:desc.into(), risk_level:RiskLevel::Low, required_permissions:vec![], dependencies:vec![], input_schema:None, output_schema:None, execution_target:ExecutionTarget::Local, tags:vec!["tasks".into()], deprecated:false, experimental:false }, "james.tasks".into()).await?;
    }
    Ok(())
}
