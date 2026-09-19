//! James-Scheduler module facade.
//!
//! Scheduling authority and task creation live in core james-scheduler and
//! james-tasks. This module only provides persistence/manifest/API adaptation.

use std::path::Path;
use std::sync::Arc;
use anyhow::Result;
use chrono::{DateTime, Utc};
use cron::Schedule;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use james_tasks::{Task, TaskPriority, RetryPolicy, TaskStatus};
use james_tasks_core::{TaskManager, Scheduler as CoreScheduler, ScheduledTask as CoreScheduledTask, ScheduleType, TaskTemplate};
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
    pub next_run: Option<DateTime<Utc>>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub database_path: String,
    pub tick_interval_secs: u64,
    pub max_concurrent_jobs: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { database_path: ".james/scheduler.json".into(), tick_interval_secs: 60, max_concurrent_jobs: 5 }
    }
}

pub struct SchedulerModule {
    config: SchedulerConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    jobs: Arc<RwLock<Vec<ScheduledJob>>>,
    scheduler: Arc<CoreScheduler>,
    scheduler_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl SchedulerModule {
    pub fn new(
        config: SchedulerConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        task_manager: Arc<TaskManager>,
    ) -> Self {
        let scheduler = Arc::new(CoreScheduler::new(task_manager).with_event_bus(event_bus.clone()));
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)), jobs: Arc::new(RwLock::new(Vec::new())), scheduler, scheduler_handle: Arc::new(RwLock::new(None)) }
    }

    pub fn scheduler(&self) -> Arc<CoreScheduler> { self.scheduler.clone() }

    pub async fn start(&self) -> Result<()> {
        self.load().await?;
        for job in self.jobs.read().await.clone() {
            if let Ok(core_job) = to_core(&job) {
                let _ = self.scheduler.schedule(core_job);
            }
        }
        self.scheduler.start().await?;
        *self.running.write().await = true;
        let this = self.clone();
        let handle = tokio::spawn(async move {
            let mut tick = interval(tokio::time::Duration::from_secs(this.config.tick_interval_secs.max(1)));
            while *this.running.read().await {
                tick.tick().await;
                if let Err(e) = this.scheduler.tick().await { warn!("Scheduler tick error: {}", e); }
                this.sync_from_core().await;
            }
        });
        *self.scheduler_handle.write().await = Some(handle);
        self.event_bus.publish(Event::new("module.scheduler.started", "james-scheduler")).await?;
        info!("James-Scheduler facade started");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        if let Some(handle) = self.scheduler_handle.write().await.take() { handle.abort(); }
        self.sync_from_core().await;
        self.scheduler.stop().await?;
        self.save().await?;
        self.event_bus.publish(Event::new("module.scheduler.stopped", "james-scheduler")).await?;
        Ok(())
    }

    pub async fn add_job(&self, mut job: ScheduledJob) -> Result<String> {
        if job.id.is_empty() { job.id = Uuid::now_v7().to_string(); }
        let now = Utc::now();
        job.created_at = now;
        job.updated_at = now;
        job.next_run = Schedule::from_str(&job.cron_expression).ok().and_then(|s| s.upcoming(Utc).next());
        let core_job = to_core(&job)?;
        self.scheduler.schedule(core_job)?;
        self.jobs.write().await.push(job.clone());
        self.save().await?;
        self.event_bus.publish(Event::new("scheduler.job.added", "james-scheduler")
            .with_payload(serde_json::json!({"job_id":job.id,"name":job.name}))).await?;
        Ok(job.id)
    }

    pub async fn get_job(&self, id: &str) -> Result<Option<ScheduledJob>> {
        self.sync_from_core().await;
        Ok(self.jobs.read().await.iter().find(|j| j.id == id).cloned())
    }

    pub async fn set_job_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        self.sync_from_core().await;
        let job = self.jobs.read().await.iter().find(|j| j.id == id).cloned().ok_or_else(|| anyhow::anyhow!("job not found"))?;
        let uid = Uuid::parse_str(&job.id)?;
        if enabled { self.scheduler.enable(uid)?; } else { self.scheduler.disable(uid)?; }
        if let Some(j) = self.jobs.write().await.iter_mut().find(|j| j.id == id) { j.enabled = enabled; j.updated_at = Utc::now(); }
        self.save().await?;
        Ok(())
    }

    pub async fn list_jobs(&self, enabled_only: bool) -> Result<Vec<ScheduledJob>> {
        self.sync_from_core().await;
        Ok(self.jobs.read().await.iter().filter(|j| !enabled_only || j.enabled).cloned().collect())
    }

    pub async fn remove_job(&self, id: &str) -> Result<()> {
        let uid = Uuid::parse_str(id)?;
        self.scheduler.unschedule(uid)?;
        self.jobs.write().await.retain(|j| j.id != id);
        self.save().await?;
        Ok(())
    }

    async fn load(&self) -> Result<()> {
        let path = Path::new(&self.config.database_path);
        if path.exists() {
            let raw = tokio::fs::read_to_string(path).await?;
            if !raw.trim().is_empty() { *self.jobs.write().await = serde_json::from_str(&raw)?; }
        }
        Ok(())
    }

    async fn save(&self) -> Result<()> {
        let path = Path::new(&self.config.database_path);
        if let Some(parent) = path.parent() { if !parent.as_os_str().is_empty() { tokio::fs::create_dir_all(parent).await?; } }
        let jobs = self.jobs.read().await.clone();
        tokio::fs::write(path, serde_json::to_string_pretty(&jobs)?).await?;
        Ok(())
    }

    async fn sync_from_core(&self) {
        let core_jobs = self.scheduler.list_all();
        let mut jobs = self.jobs.write().await;
        for core in core_jobs {
            if let Some(job) = jobs.iter_mut().find(|j| j.id == core.id.to_string()) {
                job.next_run = core.next_run;
                job.last_run = core.last_run;
                job.run_count = core.run_count;
                job.updated_at = core.updated_at;
            }
        }
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

impl Clone for SchedulerModule {
    fn clone(&self) -> Self {
        Self { config:self.config.clone(), event_bus:self.event_bus.clone(), capability_registry:self.capability_registry.clone(), running:self.running.clone(), jobs:self.jobs.clone(), scheduler:self.scheduler.clone(), scheduler_handle:self.scheduler_handle.clone() }
    }
}

fn to_core(job: &ScheduledJob) -> Result<CoreScheduledTask> {
    let id = Uuid::parse_str(&job.id)?;
    let dependencies = job.task_template.dependencies.iter().map(|x| Uuid::parse_str(x)).collect::<Result<Vec<_>,_>>()?;
    Ok(CoreScheduledTask {
        id,
        name: job.name.clone(),
        task_template: TaskTemplate {
            task_type: job.task_template.capability.clone(),
            name: job.task_template.name.clone(),
            priority: job.task_template.priority,
            source: "james.scheduler".into(),
            timeout_secs: 300,
            retry_policy: RetryPolicy::default(),
            payload: job.task_template.payload.clone(),
        },
        schedule: ScheduleType::Cron { expression: job.cron_expression.clone() },
        next_run: job.next_run,
        last_run: job.last_run,
        run_count: job.run_count,
        enabled: job.enabled,
        created_at: job.created_at,
        updated_at: job.updated_at,
    })
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id:"james.scheduler".into(), name:"James-Scheduler".into(), version:"0.1.0".into(),
        description:"Scheduler facade over the canonical core scheduler".into(),
        module_type:ModuleType::Service, entry_point:"james_scheduler".into(),
        capabilities:vec!["scheduler.add_job".into(),"scheduler.remove_job".into(),"scheduler.enable_job".into(),"scheduler.list_jobs".into()],
        dependencies:vec![james_module_host::ModuleDependency{name:"james.tasks".into(),version:"0.1.0".into(),optional:false,reason:Some("Task manager".into())}],
        permissions:vec![], configuration_schema:None, default_config:None, author:Some("JAMES Project".into()),
        homepage:None, repository:None, license:"MIT".into(), tags:vec!["scheduler".into(),"cron".into()], min_core_version:"0.1.0".into(),
        platforms:vec!["windows".into(),"linux".into(),"macos".into()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id,name,desc) in [("scheduler.add_job","Scheduler Add Job","Add a scheduled job"),("scheduler.remove_job","Scheduler Remove Job","Remove a scheduled job"),("scheduler.enable_job","Scheduler Enable Job","Enable or disable a scheduled job"),("scheduler.list_jobs","Scheduler List Jobs","List scheduled jobs")] {
        registry.register(CapabilityDefinition{id:id.into(),name:name.into(),category:james_capabilities::CapabilityCategory::Custom("scheduler".into()),version:"1.0.0".into(),provider:"james.scheduler".into(),description:desc.into(),risk_level:RiskLevel::Low,required_permissions:vec![],dependencies:vec!["task.create".into(),"task.queue".into()],input_schema:None,output_schema:None,execution_target:ExecutionTarget::Local,tags:vec!["scheduler".into()],deprecated:false,experimental:false},"james.scheduler".into()).await?;
    }
    Ok(())
}
