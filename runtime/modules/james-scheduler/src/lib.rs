//! James-Scheduler - Scheduled task execution for JAMES
//!
//! Provides cron-like scheduling with JSON file persistence.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use anyhow::Result;
use cron::Schedule;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use james_tasks::{Task, TasksModule};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub name: String,
    pub description: String,
    pub cron_expression: String,
    pub task_template: Task,
    pub enabled: bool,
    pub next_run: Option<chrono::DateTime<chrono::Utc>>,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    pub run_count: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub database_path: String,
    pub tick_interval_secs: u64,
    pub max_concurrent_jobs: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            database_path: ".james/scheduler.json".to_string(),
            tick_interval_secs: 60,
            max_concurrent_jobs: 5,
        }
    }
}

pub struct SchedulerModule {
    config: SchedulerConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    jobs: Arc<RwLock<Vec<ScheduledJob>>>,
    tasks_module: Arc<TasksModule>,
    scheduler_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl SchedulerModule {
    pub fn new(
        config: SchedulerConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        tasks_module: Arc<TasksModule>,
    ) -> Self {
        Self {
            config,
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            jobs: Arc::new(RwLock::new(Vec::new())),
            tasks_module,
            scheduler_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        self.load().await?;

        let scheduler = self.clone();
        let handle = tokio::spawn(async move {
            scheduler.run_scheduler_loop().await;
        });
        *self.scheduler_handle.write().await = Some(handle);

        info!("James-Scheduler started");
        self.event_bus.publish(Event::new("module.scheduler.started", "james-scheduler")
            .with_payload(serde_json::json!({"database": self.config.database_path}))).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        if let Some(handle) = self.scheduler_handle.write().await.take() {
            handle.abort();
        }
        self.save().await?;
        info!("James-Scheduler stopped");
        self.event_bus.publish(Event::new("module.scheduler.stopped", "james-scheduler")).await?;
        Ok(())
    }

    async fn load(&self) -> Result<()> {
        let path = Path::new(&self.config.database_path);
        if path.exists() {
            let raw = tokio::fs::read_to_string(path).await?;
            if !raw.trim().is_empty() {
                *self.jobs.write().await = serde_json::from_str(&raw)?;
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
        let jobs = self.jobs.read().await.clone();
        tokio::fs::write(path, serde_json::to_string_pretty(&jobs)?).await?;
        Ok(())
    }

    pub async fn add_job(&self, mut job: ScheduledJob) -> Result<String> {
        if job.id.is_empty() {
            job.id = Uuid::now_v7().to_string();
        }
        job.created_at = chrono::Utc::now();
        job.updated_at = chrono::Utc::now();

        if let Ok(schedule) = Schedule::from_str(&job.cron_expression) {
            job.next_run = schedule.upcoming(chrono::Utc).next();
        }

        {
            let mut jobs = self.jobs.write().await;
            jobs.push(job.clone());
        }
        self.save().await?;

        self.event_bus.publish(Event::new("scheduler.job.added", "james-scheduler")
            .with_payload(serde_json::json!({"job_id": job.id, "name": job.name}))).await?;

        Ok(job.id)
    }

    pub async fn get_job(&self, id: &str) -> Result<Option<ScheduledJob>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.iter().find(|j| j.id == id).cloned())
    }

    pub async fn set_job_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        {
            let mut jobs = self.jobs.write().await;
            if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
                job.enabled = enabled;
                job.updated_at = chrono::Utc::now();
                if enabled {
                    if let Ok(schedule) = Schedule::from_str(&job.cron_expression) {
                        job.next_run = schedule.upcoming(chrono::Utc).next();
                    }
                } else {
                    job.next_run = None;
                }
            }
        }
        self.save().await?;
        Ok(())
    }

    pub async fn list_jobs(&self, enabled_only: bool) -> Result<Vec<ScheduledJob>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.iter()
            .filter(|j| !enabled_only || j.enabled)
            .cloned()
            .collect())
    }

    pub async fn remove_job(&self, id: &str) -> Result<()> {
        {
            let mut jobs = self.jobs.write().await;
            jobs.retain(|j| j.id != id);
        }
        self.save().await?;
        Ok(())
    }

    async fn run_scheduler_loop(&self) {
        let mut tick = interval(tokio::time::Duration::from_secs(self.config.tick_interval_secs));
        while *self.running.read().await {
            tick.tick().await;
            if let Err(e) = self.check_and_run_jobs().await {
                warn!("Scheduler tick error: {}", e);
            }
        }
    }

    async fn check_and_run_jobs(&self) -> Result<()> {
        let now = chrono::Utc::now();
        let jobs = self.list_jobs(true).await?;

        for job in jobs {
            if let Some(next_run) = job.next_run {
                if next_run <= now {
                    info!("Running scheduled job: {} ({})", job.name, job.id);

                    let mut task = job.task_template.clone();
                    task.id = String::new();
                    task.created_at = now;
                    task.updated_at = now;
                    task.scheduled_at = Some(next_run);

                    if let Ok(task_id) = self.tasks_module.create_task(task).await {
                        if let Err(e) = self.tasks_module.queue_task(&task_id).await {
                            warn!("Failed to queue task {}: {}", task_id, e);
                        } else {
                            let new_next_run = Schedule::from_str(&job.cron_expression)
                                .ok()
                                .and_then(|s| s.upcoming(chrono::Utc).next());

                            {
                                let mut jobs = self.jobs.write().await;
                                if let Some(j) = jobs.iter_mut().find(|j| j.id == job.id) {
                                    j.last_run = Some(now);
                                    j.next_run = new_next_run;
                                    j.run_count += 1;
                                    j.updated_at = now;
                                }
                            }
                            self.save().await?;

                            self.event_bus.publish(Event::new("scheduler.job.executed", "james-scheduler")
                                .with_payload(serde_json::json!({"job_id": job.id, "task_id": task_id}))).await?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

impl Clone for SchedulerModule {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            event_bus: self.event_bus.clone(),
            capability_registry: self.capability_registry.clone(),
            running: self.running.clone(),
            jobs: self.jobs.clone(),
            tasks_module: self.tasks_module.clone(),
            scheduler_handle: self.scheduler_handle.clone(),
        }
    }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.scheduler".to_string(),
        name: "James-Scheduler".to_string(),
        version: "0.1.0".to_string(),
        description: "Scheduled task execution for JAMES".to_string(),
        module_type: ModuleType::Service,
        entry_point: "james_scheduler".to_string(),
        capabilities: vec![
            "scheduler.add_job".to_string(),
            "scheduler.remove_job".to_string(),
            "scheduler.enable_job".to_string(),
            "scheduler.list_jobs".to_string(),
        ],
        dependencies: vec![
            james_module_host::ModuleDependency {
                name: "james.tasks".to_string(),
                version: "0.1.0".to_string(),
                optional: false,
                reason: Some("Required for task execution".to_string()),
            },
        ],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "database_path": {"type": "string", "default": ".james/scheduler.json"},
                "tick_interval_secs": {"type": "integer", "default": 60},
                "max_concurrent_jobs": {"type": "integer", "default": 5}
            }
        })),
        default_config: Some(serde_json::json!({
            "database_path": ".james/scheduler.json",
            "tick_interval_secs": 60,
            "max_concurrent_jobs": 5
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["scheduler".to_string(), "cron".to_string(), "automation".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id, name, desc) in [
        ("scheduler.add_job", "Scheduler Add Job", "Add a scheduled job"),
        ("scheduler.remove_job", "Scheduler Remove Job", "Remove a scheduled job"),
        ("scheduler.enable_job", "Scheduler Enable Job", "Enable/disable a scheduled job"),
        ("scheduler.list_jobs", "Scheduler List Jobs", "List all scheduled jobs"),
    ] {
        registry.register(CapabilityDefinition {
            id: id.to_string(), name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("scheduler".to_string()),
            version: "1.0.0".to_string(), provider: "james.scheduler".to_string(),
            description: desc.to_string(), risk_level: RiskLevel::Low,
            required_permissions: vec![], dependencies: vec!["task.create".to_string(), "task.queue".to_string()],
            input_schema: None, output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["scheduler".to_string(), "cron".to_string()], deprecated: false, experimental: false,
        }, "james.scheduler".to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_scheduler_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.scheduler");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}