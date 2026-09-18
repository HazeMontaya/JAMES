use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use dashmap::DashMap;
use cron::Schedule;

use james_events::EventBus;
use james_tasks::{TaskManager, Task, TaskStatus, TaskPriority, RetryPolicy};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScheduleType {
    Immediate,
    Delayed { delay_secs: u64 },
    Interval { interval_secs: u64 },
    Cron { expression: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub name: String,
    pub task_template: TaskTemplate,
    pub schedule: ScheduleType,
    pub next_run: Option<DateTime<Utc>>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTemplate {
    pub task_type: String,
    pub name: String,
    pub priority: TaskPriority,
    pub source: String,
    pub timeout_secs: u64,
    pub retry_policy: RetryPolicy,
    pub payload: serde_json::Value,
}

pub struct Scheduler {
    scheduled_tasks: DashMap<Uuid, ScheduledTask>,
    task_manager: Arc<TaskManager>,
    running: Arc<RwLock<bool>>,
    event_bus: Option<Arc<EventBus>>,
}

impl Scheduler {
    pub fn new(task_manager: Arc<TaskManager>) -> Self {
        Self {
            scheduled_tasks: DashMap::new(),
            task_manager,
            running: Arc::new(RwLock::new(false)),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        self.update_next_runs().await;
        info!("Scheduler started");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("Scheduler stopped");
        Ok(())
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn schedule(&self, mut task: ScheduledTask) -> Result<Uuid> {
        if task.id == Uuid::nil() {
            task.id = Uuid::now_v7();
        }
        
        if task.created_at == DateTime::UNIX_EPOCH {
            task.created_at = Utc::now();
        }
        task.updated_at = Utc::now();
        
        // For interval tasks, run immediately on first tick, then at interval
        task.next_run = match &task.schedule {
            ScheduleType::Interval { .. } => Some(Utc::now()),
            _ => self.calculate_next_run(&task.schedule, None),
        };
        
        self.scheduled_tasks.insert(task.id, task.clone());
        Ok(task.id)
    }

    pub fn unschedule(&self, id: Uuid) -> Result<bool> {
        Ok(self.scheduled_tasks.remove(&id).is_some())
    }

    pub fn get(&self, id: Uuid) -> Option<ScheduledTask> {
        self.scheduled_tasks.get(&id).map(|t| t.clone())
    }

    pub fn list_all(&self) -> Vec<ScheduledTask> {
        self.scheduled_tasks.iter().map(|t| t.clone()).collect()
    }

    pub fn list_enabled(&self) -> Vec<ScheduledTask> {
        self.scheduled_tasks
            .iter()
            .filter(|t| t.enabled)
            .map(|t| t.clone())
            .collect()
    }

    pub fn enable(&self, id: Uuid) -> Result<bool> {
        if let Some(mut task) = self.scheduled_tasks.get_mut(&id) {
            task.enabled = true;
            task.updated_at = Utc::now();
            task.next_run = self.calculate_next_run(&task.schedule, None);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn disable(&self, id: Uuid) -> Result<bool> {
        if let Some(mut task) = self.scheduled_tasks.get_mut(&id) {
            task.enabled = false;
            task.updated_at = Utc::now();
            task.next_run = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn tick(&self) -> Result<()> {
        if !*self.running.read().await {
            return Ok(());
        }

        let now = Utc::now();
        let mut due_tasks = Vec::new();

        for entry in self.scheduled_tasks.iter() {
            let task = entry.value();
            if !task.enabled {
                continue;
            }

            if let Some(next_run) = task.next_run {
                if next_run <= now {
                    due_tasks.push(task.clone());
                }
            }
        }

        for task in due_tasks {
            self.execute_task(&task).await?;
        }

        Ok(())
    }

    async fn execute_task(&self, scheduled: &ScheduledTask) -> Result<()> {
        let mut task = scheduled.task_template.to_task();
        task.name = format!("{} (scheduled)", scheduled.name);
        
        let _task_id = self.task_manager.create_task(task).await?;
        
        if let Some(mut scheduled) = self.scheduled_tasks.get_mut(&scheduled.id) {
            scheduled.last_run = Some(Utc::now());
            scheduled.run_count += 1;
            scheduled.next_run = self.calculate_next_run(&scheduled.schedule, Some(Utc::now()));
            scheduled.updated_at = Utc::now();
        }

        info!("Executed scheduled task: {} (id: {})", scheduled.name, scheduled.id);
        Ok(())
    }

    fn calculate_next_run(&self, schedule: &ScheduleType, from: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
        let base = from.unwrap_or_else(Utc::now);
        
        match schedule {
            ScheduleType::Immediate => Some(base),
            ScheduleType::Delayed { delay_secs } => Some(base + Duration::from_secs(*delay_secs as u64)),
            ScheduleType::Interval { interval_secs } => Some(base + Duration::from_secs(*interval_secs as u64)),
            ScheduleType::Cron { expression } => {
                if let Ok(schedule) = expression.parse::<Schedule>() {
                    schedule.upcoming(Utc).next()
                } else {
                    None
                }
            }
        }
    }

    async fn update_next_runs(&self) {
        for mut task in self.scheduled_tasks.iter_mut() {
            if task.enabled && task.next_run.is_none() {
                task.next_run = self.calculate_next_run(&task.schedule, None);
            }
        }
    }
}

impl TaskTemplate {
    pub fn to_task(&self) -> Task {
        Task {
            id: Uuid::nil(),
            task_type: self.task_type.clone(),
            name: self.name.clone(),
            priority: self.priority.clone(),
            status: TaskStatus::Queued,
            created_at: DateTime::UNIX_EPOCH,
            started_at: None,
            completed_at: None,
            source: self.source.clone(),
            dependencies: vec![],
            timeout_secs: self.timeout_secs,
            retry_policy: self.retry_policy.clone(),
            current_retry: 0,
            payload: self.payload.clone(),
            result: None,
            error: None,
            assigned_worker: None,
            progress: None,
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(Arc::new(TaskManager::new(None)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_immediate() {
        let task_manager = Arc::new(TaskManager::new(None));
        task_manager.start().await.unwrap();
        
        let scheduler = Scheduler::new(task_manager.clone());
        scheduler.start().await.unwrap();

        let task = ScheduledTask {
            id: Uuid::nil(),
            name: "immediate-test".to_string(),
            task_template: TaskTemplate {
                task_type: "test".to_string(),
                name: "test".to_string(),
                priority: TaskPriority::Normal,
                source: "scheduler".to_string(),
                timeout_secs: 60,
                retry_policy: RetryPolicy::default(),
                payload: serde_json::json!({}),
            },
            schedule: ScheduleType::Immediate,
            next_run: None,
            last_run: None,
            run_count: 0,
            enabled: true,
            created_at: DateTime::UNIX_EPOCH,
            updated_at: DateTime::UNIX_EPOCH,
        };

        let id = scheduler.schedule(task).unwrap();
        scheduler.tick().await.unwrap();
        
        let queued = task_manager.list_by_status(TaskStatus::Queued);
        assert_eq!(queued.len(), 1);
        
        let scheduled = scheduler.get(id).unwrap();
        assert!(scheduled.last_run.is_some());
        assert_eq!(scheduled.run_count, 1);
    }

    #[tokio::test]
    async fn test_scheduler_interval() {
        let task_manager = Arc::new(TaskManager::new(None));
        task_manager.start().await.unwrap();
        
        let scheduler = Scheduler::new(task_manager.clone());
        scheduler.start().await.unwrap();

        let task = ScheduledTask {
            id: Uuid::nil(),
            name: "interval-test".to_string(),
            task_template: TaskTemplate {
                task_type: "test".to_string(),
                name: "test".to_string(),
                priority: TaskPriority::Normal,
                source: "scheduler".to_string(),
                timeout_secs: 60,
                retry_policy: RetryPolicy::default(),
                payload: serde_json::json!({}),
            },
            schedule: ScheduleType::Interval { interval_secs: 1 },
            next_run: None,
            last_run: None,
            run_count: 0,
            enabled: true,
            created_at: DateTime::UNIX_EPOCH,
            updated_at: DateTime::UNIX_EPOCH,
        };

        let id = scheduler.schedule(task).unwrap();
        
        scheduler.tick().await.unwrap();
        let queued1 = task_manager.list_by_status(TaskStatus::Queued);
        assert_eq!(queued1.len(), 1);
        
        let scheduled1 = scheduler.get(id).unwrap();
        assert_eq!(scheduled1.run_count, 1);
        assert!(scheduled1.next_run.is_some());
    }

    #[tokio::test]
    async fn test_scheduler_cron() {
        let task_manager = Arc::new(TaskManager::new(None));
        task_manager.start().await.unwrap();
        
        let scheduler = Scheduler::new(task_manager.clone());
        scheduler.start().await.unwrap();

        let task = ScheduledTask {
            id: Uuid::nil(),
            name: "cron-test".to_string(),
            task_template: TaskTemplate {
                task_type: "test".to_string(),
                name: "test".to_string(),
                priority: TaskPriority::Normal,
                source: "scheduler".to_string(),
                timeout_secs: 60,
                retry_policy: RetryPolicy::default(),
                payload: serde_json::json!({}),
            },
            schedule: ScheduleType::Cron { expression: "* * * * * *".to_string() }, // Every second
            next_run: None,
            last_run: None,
            run_count: 0,
            enabled: true,
            created_at: DateTime::UNIX_EPOCH,
            updated_at: DateTime::UNIX_EPOCH,
        };

        let id = scheduler.schedule(task).unwrap();
        assert!(scheduler.get(id).unwrap().next_run.is_some());
    }

    #[tokio::test]
    async fn test_scheduler_enable_disable() {
        let task_manager = Arc::new(TaskManager::new(None));
        task_manager.start().await.unwrap();
        
        let scheduler = Scheduler::new(task_manager.clone());
        scheduler.start().await.unwrap();

        let task = ScheduledTask {
            id: Uuid::nil(),
            name: "enable-test".to_string(),
            task_template: TaskTemplate {
                task_type: "test".to_string(),
                name: "test".to_string(),
                priority: TaskPriority::Normal,
                source: "scheduler".to_string(),
                timeout_secs: 60,
                retry_policy: RetryPolicy::default(),
                payload: serde_json::json!({}),
            },
            schedule: ScheduleType::Interval { interval_secs: 60 },
            next_run: None,
            last_run: None,
            run_count: 0,
            enabled: true,
            created_at: DateTime::UNIX_EPOCH,
            updated_at: DateTime::UNIX_EPOCH,
        };

        let id = scheduler.schedule(task).unwrap();
        scheduler.disable(id).unwrap();
        
        let disabled = scheduler.get(id).unwrap();
        assert!(!disabled.enabled);
        assert!(disabled.next_run.is_none());

        scheduler.enable(id).unwrap();
        let enabled = scheduler.get(id).unwrap();
        assert!(enabled.enabled);
        assert!(enabled.next_run.is_some());
    }
}