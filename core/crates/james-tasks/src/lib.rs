use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{info, debug, warn, error};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use dashmap::DashMap;
use parking_lot::Mutex;

use james_events::{Event, EventEnvelope, builtin_events, create_system_event, EventBus};
use james_capabilities::CapabilityRegistry;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TaskPriority {
    Low = 0,
    Normal = 50,
    High = 100,
    Critical = 200,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_secs: u64,
    pub max_delay_secs: u64,
    pub exponential_backoff: bool,
    pub retry_on: Vec<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_secs: 1,
            max_delay_secs: 60,
            exponential_backoff: true,
            retry_on: vec!["timeout".to_string(), "error".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub task_type: String,
    pub name: String,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub source: String,
    pub dependencies: Vec<Uuid>,
    pub timeout_secs: u64,
    pub retry_policy: RetryPolicy,
    pub current_retry: u32,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub assigned_worker: Option<String>,
    pub progress: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: Uuid,
    pub status: TaskStatus,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub retries: u32,
}

pub struct TaskManager {
    tasks: DashMap<Uuid, Task>,
    by_status: DashMap<TaskStatus, Vec<Uuid>>,
    by_source: DashMap<String, Vec<Uuid>>,
    event_bus: Option<Arc<EventBus>>,
    capability_registry: Option<Arc<CapabilityRegistry>>,
    running: Arc<RwLock<bool>>,
    worker_id: String,
    max_concurrent: usize,
    active_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl TaskManager {
    pub fn new(event_bus: Option<Arc<EventBus>>) -> Self {
        Self {
            tasks: DashMap::new(),
            by_status: DashMap::new(),
            by_source: DashMap::new(),
            event_bus,
            capability_registry: None,
            running: Arc::new(RwLock::new(false)),
            worker_id: Uuid::now_v7().to_string(),
            max_concurrent: 100,
            active_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn with_capability_registry(mut self, registry: Arc<CapabilityRegistry>) -> Self {
        self.capability_registry = Some(registry);
        self
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Task Manager started (worker: {})", self.worker_id);
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        
        for entry in self.tasks.iter() {
            let mut task = entry.value().clone();
            if task.status == TaskStatus::Queued || task.status == TaskStatus::Running {
                task.status = TaskStatus::Cancelled;
                self.tasks.insert(task.id, task);
            }
        }

        info!("Task Manager stopped");
        Ok(())
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn set_capability_registry(&mut self, registry: Arc<CapabilityRegistry>) {
        self.capability_registry = Some(registry);
    }

    pub fn create_task(&self, mut task: Task) -> Result<Uuid> {
        if task.id == Uuid::nil() {
            task.id = Uuid::now_v7();
        }
        
        if task.created_at == DateTime::UNIX_EPOCH {
            task.created_at = Utc::now();
        }
        
        task.status = TaskStatus::Queued;
        
        self.by_status.entry(TaskStatus::Queued).or_default().push(task.id);
        self.by_source.entry(task.source.clone()).or_default().push(task.id);
        self.tasks.insert(task.id, task.clone());

        self.emit_task_event(builtin_events::TASK_CREATED, &task);

        Ok(task.id)
    }

    pub fn get_task(&self, id: Uuid) -> Option<Task> {
        self.tasks.get(&id).map(|t| t.clone())
    }

    pub fn update_status(&self, id: Uuid, status: TaskStatus) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            let old_status = task.status.clone();
            task.status = status.clone();

            if status == TaskStatus::Running && task.started_at.is_none() {
                task.started_at = Some(Utc::now());
                self.active_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            if matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Timeout) {
                task.completed_at = Some(Utc::now());
                self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }

            self.by_status.entry(old_status).or_default().retain(|&x| x != id);
            self.by_status.entry(status.clone()).or_default().push(id);

            self.emit_task_event(
                match status {
                    TaskStatus::Running => builtin_events::TASK_STARTED,
                    TaskStatus::Completed => builtin_events::TASK_COMPLETED,
                    TaskStatus::Failed => builtin_events::TASK_FAILED,
                    TaskStatus::Cancelled => builtin_events::TASK_CANCELLED,
                    _ => builtin_events::TASK_CREATED,
                },
                &task,
            );

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn set_result(&self, id: Uuid, result: serde_json::Value) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            task.result = Some(result);
            task.status = TaskStatus::Completed;
            task.completed_at = Some(Utc::now());
            self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            
            self.by_status.entry(TaskStatus::Running).or_default().retain(|&x| x != id);
            self.by_status.entry(TaskStatus::Completed).or_default().push(id);

            self.emit_task_event(builtin_events::TASK_COMPLETED, &task);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn set_error(&self, id: Uuid, error: String) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            task.error = Some(error);
            task.status = TaskStatus::Failed;
            task.completed_at = Some(Utc::now());
            self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            
            self.by_status.entry(TaskStatus::Running).or_default().retain(|&x| x != id);
            self.by_status.entry(TaskStatus::Failed).or_default().push(id);

            self.emit_task_event(builtin_events::TASK_FAILED, &task);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_progress(&self, id: Uuid, progress: f32) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            task.progress = Some(progress.clamp(0.0, 1.0));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn assign_worker(&self, id: Uuid, worker: String) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            task.assigned_worker = Some(worker);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn list_tasks(&self) -> Vec<Task> {
        self.tasks.iter().map(|t| t.clone()).collect()
    }

    pub fn list_by_status(&self, status: TaskStatus) -> Vec<Task> {
        self.by_status
            .get(&status)
            .map(|ids| ids.iter().filter_map(|id| self.tasks.get(id).map(|t| t.clone())).collect())
            .unwrap_or_default()
    }

    pub fn list_by_source(&self, source: &str) -> Vec<Task> {
        self.by_source
            .get(source)
            .map(|ids| ids.iter().filter_map(|id| self.tasks.get(id).map(|t| t.clone())).collect())
            .unwrap_or_default()
    }

    pub fn count(&self) -> usize {
        self.tasks.len()
    }

    pub fn count_by_status(&self, status: TaskStatus) -> usize {
        self.by_status.get(&status).map(|v| v.len()).unwrap_or(0)
    }

    pub fn active_count(&self) -> usize {
        self.active_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn can_accept_task(&self) -> bool {
        self.active_count() < self.max_concurrent
    }

    pub async fn cancel_task(&self, id: Uuid) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            if task.status == TaskStatus::Queued || task.status == TaskStatus::Running {
                task.status = TaskStatus::Cancelled;
                task.completed_at = Some(Utc::now());
                self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                
                self.by_status.entry(task.status.clone()).or_default().retain(|&x| x != id);
                self.by_status.entry(TaskStatus::Cancelled).or_default().push(id);

                self.emit_task_event(builtin_events::TASK_CANCELLED, &task);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    pub async fn retry_task(&self, id: Uuid) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            if task.status == TaskStatus::Failed && task.current_retry < task.retry_policy.max_retries {
                task.current_retry += 1;
                task.status = TaskStatus::Queued;
                task.error = None;
                task.result = None;
                task.started_at = None;
                task.completed_at = None;
                
                self.by_status.entry(TaskStatus::Failed).or_default().retain(|&x| x != id);
                self.by_status.entry(TaskStatus::Queued).or_default().push(id);

                let delay = self.calculate_retry_delay(&task);
                tokio::time::sleep(Duration::from_secs(delay)).await;

self.emit_task_event(builtin_events::TASK_CREATED, &task);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    fn calculate_retry_delay(&self, task: &Task) -> u64 {
        let policy = &task.retry_policy;
        let delay = if policy.exponential_backoff {
            policy.base_delay_secs * 2u64.pow(task.current_retry)
        } else {
            policy.base_delay_secs
        };
        delay.min(policy.max_delay_secs)
    }

    async fn emit_task_event(&self, event_type: &str, task: &Task) {
        if let Some(bus) = &self.event_bus {
            let event = create_system_event(event_type, "task-manager")
                .with_payload(serde_json::json!({
                    "task_id": task.id,
                    "task_type": task.task_type,
                    "name": task.name,
                    "status": task.status,
                    "source": task.source,
                }));
            let _ = bus.publish_envelope(EventEnvelope::new(event)).await;
        }
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new(None)
    }
}

pub struct TaskEventBus {
    inner: Arc<EventBus>,
}

impl TaskEventBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(EventBus::with_default_buffer()),
        }
    }

    pub fn subscribe(&self, event_type: &str) -> tokio::sync::mpsc::UnboundedReceiver<EventEnvelope> {
        self.inner.subscribe(event_type)
    }

    pub fn subscribe_all(&self) -> tokio::sync::mpsc::UnboundedReceiver<EventEnvelope> {
        self.inner.subscribe_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_manager_create() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let task = Task {
            id: Uuid::nil(),
            task_type: "test".to_string(),
            name: "Test Task".to_string(),
            priority: TaskPriority::Normal,
            status: TaskStatus::Queued,
            created_at: DateTime::UNIX_EPOCH,
            started_at: None,
            completed_at: None,
            source: "test".to_string(),
            dependencies: vec![],
            timeout_secs: 60,
            retry_policy: RetryPolicy::default(),
            current_retry: 0,
            payload: serde_json::json!({"input": "test"}),
            result: None,
            error: None,
            assigned_worker: None,
            progress: None,
        };

        let id = manager.create_task(task).unwrap();
        assert_ne!(id, Uuid::nil());

        let retrieved = manager.get_task(id).unwrap();
        assert_eq!(retrieved.task_type, "test");
        assert_eq!(retrieved.status, TaskStatus::Queued);
    }

    #[tokio::test]
    async fn test_task_lifecycle() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let task = Task {
            id: Uuid::nil(),
            task_type: "test".to_string(),
            name: "Test Task".to_string(),
            priority: TaskPriority::Normal,
            status: TaskStatus::Queued,
            created_at: DateTime::UNIX_EPOCH,
            started_at: None,
            completed_at: None,
            source: "test".to_string(),
            dependencies: vec![],
            timeout_secs: 60,
            retry_policy: RetryPolicy::default(),
            current_retry: 0,
            payload: serde_json::json!({}),
            result: None,
            error: None,
            assigned_worker: None,
            progress: None,
        };

        let id = manager.create_task(task).unwrap();
        
        manager.update_status(id, TaskStatus::Running).unwrap();
        let running = manager.get_task(id).unwrap();
        assert_eq!(running.status, TaskStatus::Running);
        assert!(running.started_at.is_some());

        manager.set_result(id, serde_json::json!({"output": "success"})).unwrap();
        let completed = manager.get_task(id).unwrap();
        assert_eq!(completed.status, TaskStatus::Completed);
        assert!(completed.completed_at.is_some());
        assert_eq!(completed.result.unwrap()["output"], "success");
    }

    #[tokio::test]
    async fn test_task_error() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let task = Task {
            id: Uuid::nil(),
            task_type: "test".to_string(),
            name: "Test Task".to_string(),
            priority: TaskPriority::Normal,
            status: TaskStatus::Queued,
            created_at: DateTime::UNIX_EPOCH,
            started_at: None,
            completed_at: None,
            source: "test".to_string(),
            dependencies: vec![],
            timeout_secs: 60,
            retry_policy: RetryPolicy::default(),
            current_retry: 0,
            payload: serde_json::json!({}),
            result: None,
            error: None,
            assigned_worker: None,
            progress: None,
        };

        let id = manager.create_task(task).unwrap();
        manager.update_status(id, TaskStatus::Running).unwrap();
        
        manager.set_error(id, "Something went wrong".to_string()).unwrap();
        let failed = manager.get_task(id).unwrap();
        assert_eq!(failed.status, TaskStatus::Failed);
        assert_eq!(failed.error.unwrap(), "Something went wrong");
    }

    #[tokio::test]
    async fn test_task_cancel() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let task = Task {
            id: Uuid::nil(),
            task_type: "test".to_string(),
            name: "Test Task".to_string(),
            priority: TaskPriority::Normal,
            status: TaskStatus::Queued,
            created_at: DateTime::UNIX_EPOCH,
            started_at: None,
            completed_at: None,
            source: "test".to_string(),
            dependencies: vec![],
            timeout_secs: 60,
            retry_policy: RetryPolicy::default(),
            current_retry: 0,
            payload: serde_json::json!({}),
            result: None,
            error: None,
            assigned_worker: None,
            progress: None,
        };

        let id = manager.create_task(task).unwrap();
        manager.cancel_task(id).await.unwrap();
        
        let cancelled = manager.get_task(id).unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_task_priority_ordering() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let priorities = vec![
            (TaskPriority::Low, "low"),
            (TaskPriority::Normal, "normal"),
            (TaskPriority::High, "high"),
            (TaskPriority::Critical, "critical"),
        ];

        for (priority, name) in priorities {
            let task = Task {
                id: Uuid::nil(),
                task_type: "test".to_string(),
                name: format!("{} task", name),
                priority,
                status: TaskStatus::Queued,
                created_at: DateTime::UNIX_EPOCH,
                started_at: None,
                completed_at: None,
                source: "test".to_string(),
                dependencies: vec![],
                timeout_secs: 60,
                retry_policy: RetryPolicy::default(),
                current_retry: 0,
                payload: serde_json::json!({}),
                result: None,
                error: None,
                assigned_worker: None,
                progress: None,
            };
            manager.create_task(task).unwrap();
        }

        let queued = manager.list_by_status(TaskStatus::Queued);
        assert_eq!(queued.len(), 4);
    }
}