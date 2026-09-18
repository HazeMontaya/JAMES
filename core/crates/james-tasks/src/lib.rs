use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use dashmap::DashMap;

use james_events::{EventEnvelope, builtin_events, create_system_event, EventBus};
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

    pub async fn create_task(&self, mut task: Task) -> Result<Uuid> {
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

        self.emit_task_event(builtin_events::TASK_CREATED, &task).await;

        Ok(task.id)
    }

    pub fn get_task(&self, id: Uuid) -> Option<Task> {
        self.tasks.get(&id).map(|t| t.clone())
    }

    pub async fn update_status(&self, id: Uuid, status: TaskStatus) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            let old_status = task.status.clone();
            task.status = status.clone();

            if status == TaskStatus::Running && task.started_at.is_none() {
                task.started_at = Some(Utc::now());
                self.active_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            // Only decrement when actually leaving the Running state;
            // transitions from Queued (never counted) must not underflow.
            if old_status == TaskStatus::Running
                && matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Timeout)
            {
                task.completed_at = Some(Utc::now());
                self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            } else if matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled | TaskStatus::Timeout) {
                task.completed_at = Some(Utc::now());
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
            ).await;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn set_result(&self, id: Uuid, result: serde_json::Value) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            let was_running = task.status == TaskStatus::Running;
            task.result = Some(result);
            task.status = TaskStatus::Completed;
            task.completed_at = Some(Utc::now());
            if was_running {
                self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }
            
            self.by_status.entry(TaskStatus::Running).or_default().retain(|&x| x != id);
            self.by_status.entry(TaskStatus::Completed).or_default().push(id);

            self.emit_task_event(builtin_events::TASK_COMPLETED, &task).await;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn set_error(&self, id: Uuid, error: String) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            let was_running = task.status == TaskStatus::Running;
            task.error = Some(error);
            task.status = TaskStatus::Failed;
            task.completed_at = Some(Utc::now());
            if was_running {
                self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            }
            
            self.by_status.entry(TaskStatus::Running).or_default().retain(|&x| x != id);
            self.by_status.entry(TaskStatus::Failed).or_default().push(id);

            self.emit_task_event(builtin_events::TASK_FAILED, &task).await;
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

    // ---- Persistence & recovery ------------------------------------------

    /// Persist all tasks (including queued/running ones) to a JSON file.
    /// Atomic via tmp-file + rename.
    pub async fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        let tasks = self.list_tasks();
        let json = serde_json::to_string_pretty(&tasks)?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, json.as_bytes()).await?;
        tokio::fs::rename(&tmp, path).await?;

        info!("task manager saved {} tasks to {}", tasks.len(), path.display());
        Ok(())
    }

    /// Load tasks from a JSON file into the manager, rebuilding indexes.
    /// Running tasks are reset to `Queued` so they can be re-dispatched
    /// after a restart (crash recovery).
    pub async fn load(&self, path: impl AsRef<std::path::Path>) -> Result<usize> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(0);
        }
        let raw = tokio::fs::read_to_string(path).await?;
        let tasks: Vec<Task> = serde_json::from_str(&raw)?;

        let mut recovered = 0;
        for mut task in tasks {
            // Duplicate ids are ignored; a re-run would have re-created them.
            if self.tasks.contains_key(&task.id) {
                continue;
            }
            // Crash recovery: anything still in-flight becomes queuable again.
            if task.status == TaskStatus::Running {
                task.status = TaskStatus::Queued;
                task.started_at = None;
            }
            self.tasks.insert(task.id, task.clone());
            self.by_status.entry(task.status.clone()).or_default().push(task.id);
            self.by_source.entry(task.source.clone()).or_default().push(task.id);

            // Re-emit so subscribers observe the recovered task.
            self.emit_task_event(builtin_events::TASK_CREATED, &task).await;

            if task.status != TaskStatus::Running {
                recovered += 1;
            }
        }

        info!("task manager recovered {} tasks from {}", recovered, path.display());
        Ok(recovered)
    }

    /// Count of queued tasks that are ready to run (no unresolved deps).
    pub fn ready_tasks(&self) -> Vec<Task> {
        self.list_by_status(TaskStatus::Queued)
            .into_iter()
            .filter(|t| {
                t.dependencies
                    .iter()
                    .all(|dep| self.tasks.get(dep).map(|d| d.status == TaskStatus::Completed).unwrap_or(false))
            })
            .collect()
    }

    pub async fn cancel_task(&self, id: Uuid) -> Result<bool> {
        if let Some(mut task) = self.tasks.get_mut(&id) {
            if task.status == TaskStatus::Queued || task.status == TaskStatus::Running {
                let old_status = task.status.clone();
                task.status = TaskStatus::Cancelled;
                task.completed_at = Some(Utc::now());
                // Only decrement when cancelling a Running task; Queued
                // tasks were never counted (underflow guard).
                if old_status == TaskStatus::Running {
                    self.active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                }
                
                self.by_status.entry(old_status).or_default().retain(|&x| x != id);
                self.by_status.entry(TaskStatus::Cancelled).or_default().push(id);

                self.emit_task_event(builtin_events::TASK_CANCELLED, &task).await;
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    pub async fn retry_task(&self, id: Uuid) -> Result<bool> {
        // Mutate under a short-lived lock, then drop the guard BEFORE
        // sleeping: holding a DashMap shard across .await risks deadlock.
        let delay = {
            let mut task = match self.tasks.get_mut(&id) {
                Some(t) => t,
                None => return Ok(false),
            };
            if !(task.status == TaskStatus::Failed
                && task.current_retry < task.retry_policy.max_retries)
            {
                return Ok(false);
            }
            task.current_retry += 1;
            task.status = TaskStatus::Queued;
            task.error = None;
            task.result = None;
            task.started_at = None;
            task.completed_at = None;

            self.by_status.entry(TaskStatus::Failed).or_default().retain(|&x| x != id);
            self.by_status.entry(TaskStatus::Queued).or_default().push(id);

            self.calculate_retry_delay(&task)
        };

        if delay > 0 {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }

        if let Some(task) = self.tasks.get(&id) {
            self.emit_task_event(builtin_events::TASK_CREATED, &task).await;
        }
        Ok(true)
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

        let id = manager.create_task(task).await.unwrap();
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

        let id = manager.create_task(task).await.unwrap();
        
        manager.update_status(id, TaskStatus::Running).await.unwrap();
        let running = manager.get_task(id).unwrap();
        assert_eq!(running.status, TaskStatus::Running);
        assert!(running.started_at.is_some());

        manager.set_result(id, serde_json::json!({"output": "success"})).await.unwrap();
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

        let id = manager.create_task(task).await.unwrap();
        manager.update_status(id, TaskStatus::Running).await.unwrap();
        
        manager.set_error(id, "Something went wrong".to_string()).await.unwrap();
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

        let id = manager.create_task(task).await.unwrap();
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
            manager.create_task(task).await.unwrap();
        }

        let queued = manager.list_by_status(TaskStatus::Queued);
        assert_eq!(queued.len(), 4);
    }

    #[tokio::test]
    async fn test_task_events_published() {
        let bus = Arc::new(EventBus::new(100));
        bus.start().await.unwrap();
        let manager = TaskManager::new(Some(bus.clone()));
        manager.start().await.unwrap();

        let mut rx_created = bus.subscribe(builtin_events::TASK_CREATED);
        let mut rx_started = bus.subscribe(builtin_events::TASK_STARTED);

        let task = Task {
            id: Uuid::nil(),
            task_type: "test".to_string(),
            name: "Event Task".to_string(),
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

        let id = manager.create_task(task).await.unwrap();
        let created = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            rx_created.recv(),
        )
        .await
        .expect("TASK_CREATED event timeout")
        .expect("event channel closed");
        assert_eq!(created.event.payload["task_id"], id.to_string());

        manager.update_status(id, TaskStatus::Running).await.unwrap();
        let started = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            rx_started.recv(),
        )
        .await
        .expect("TASK_STARTED event timeout")
        .expect("event channel closed");
        assert_eq!(started.event.event_type, builtin_events::TASK_STARTED);
    }

    #[tokio::test]
    async fn test_cancel_from_queued_keeps_counter_and_index() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let task = Task {
            id: Uuid::nil(),
            task_type: "test".to_string(),
            name: "Cancel Task".to_string(),
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

        let id = manager.create_task(task).await.unwrap();
        assert!(manager.cancel_task(id).await.unwrap());
        // Never Running: counter must stay 0 (no underflow to usize::MAX),
        // and the stale Queued index entry must be gone.
        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.count_by_status(TaskStatus::Queued), 0);
        assert_eq!(manager.count_by_status(TaskStatus::Cancelled), 1);
    }

    // ---- Persistence & recovery tests ----

    fn sample_task(id: Uuid, status: TaskStatus) -> Task {
        Task {
            id,
            task_type: "test".to_string(),
            name: "Persisted Task".to_string(),
            priority: TaskPriority::Normal,
            status,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            source: "test".to_string(),
            dependencies: vec![],
            timeout_secs: 60,
            retry_policy: RetryPolicy::default(),
            current_retry: 0,
            payload: serde_json::json!({"input": "data"}),
            result: None,
            error: None,
            assigned_worker: None,
            progress: None,
        }
    }

    #[tokio::test]
    async fn test_save_and_load_roundtrip() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let id1 = manager.create_task(sample_task(Uuid::nil(), TaskStatus::Queued)).await.unwrap();
        let id2 = manager.create_task(sample_task(Uuid::nil(), TaskStatus::Queued)).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        manager.save(&path).await.unwrap();

        // Fresh manager loads both tasks back.
        let manager2 = TaskManager::new(None);
        manager2.start().await.unwrap();
        let loaded = manager2.load(&path).await.unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(manager2.count(), 2);
        assert!(manager2.get_task(id1).is_some());
        assert!(manager2.get_task(id2).is_some());
        assert_eq!(manager2.count_by_status(TaskStatus::Queued), 2);
    }

    #[tokio::test]
    async fn test_load_resets_running_to_queued() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let id = manager.create_task(sample_task(Uuid::nil(), TaskStatus::Queued)).await.unwrap();
        manager.update_status(id, TaskStatus::Running).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        manager.save(&path).await.unwrap();

        // On load, the running task must come back as Queued (recoverable).
        let manager2 = TaskManager::new(None);
        manager2.start().await.unwrap();
        let loaded = manager2.load(&path).await.unwrap();
        assert_eq!(loaded, 1);
        let task = manager2.get_task(id).unwrap();
        assert_eq!(task.status, TaskStatus::Queued);
        assert_eq!(manager2.active_count(), 0);
    }

    #[tokio::test]
    async fn test_ready_tasks_respects_dependencies() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();

        let id_a = manager.create_task(sample_task(Uuid::nil(), TaskStatus::Queued)).await.unwrap();
        let mut task_b = sample_task(Uuid::nil(), TaskStatus::Queued);
        task_b.dependencies = vec![id_a];
        let id_b = manager.create_task(task_b).await.unwrap();

        // Only A is ready (B waits on A completing).
        let ready = manager.ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, id_a);

        manager.update_status(id_a, TaskStatus::Running).await.unwrap();
        manager.set_result(id_a, serde_json::json!({})).await.unwrap();
        let ready2 = manager.ready_tasks();
        assert_eq!(ready2.len(), 1);
        assert_eq!(ready2[0].id, id_b);
    }

    #[tokio::test]
    async fn test_load_missing_file_returns_zero() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let n = manager.load(dir.path().join("nope.json")).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_load_skips_duplicates() {
        let manager = TaskManager::new(None);
        manager.start().await.unwrap();
        let id = manager.create_task(sample_task(Uuid::nil(), TaskStatus::Queued)).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        manager.save(&path).await.unwrap();

        // Loading into the same manager skips the existing id.
        let loaded = manager.load(&path).await.unwrap();
        assert_eq!(loaded, 0);
        assert_eq!(manager.count(), 1);
        // But the task is still there.
        assert!(manager.get_task(id).is_some());
    }
}