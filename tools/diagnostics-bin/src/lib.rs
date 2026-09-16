use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use clap::{Parser, Subcommand};
use tabled::{Table, Tabled};

use james_core::{CoreState, CoreStatus};
use james_events::{Event, EventEnvelope, builtin_events, EventBus};
use james_registry::{Registry, RegistryEntry, RegistryEntryType, RegistryStatus};
use james_capabilities::{CapabilityRegistry, RegisteredCapability, CapabilityDefinition};
use james_services::{ServiceRegistry, ServiceInstance, ServiceStatus};
use james_tasks::{TaskManager, Task, TaskStatus, TaskPriority};
use james_scheduler::{Scheduler, ScheduledTask, ScheduleType};
use james_health::{HealthMonitor, SystemHealth, ComponentHealth, HealthStatus};

pub struct Diagnostics {
    state: Arc<RwLock<CoreState>>,
    event_bus: Option<Arc<EventBus>>,
    registry: Option<Arc<Registry>>,
    capability_registry: Option<Arc<CapabilityRegistry>>,
    service_registry: Option<Arc<ServiceRegistry>>,
    task_manager: Option<Arc<TaskManager>>,
    scheduler: Option<Arc<Scheduler>>,
    health_monitor: Option<Arc<HealthMonitor>>,
}

impl Diagnostics {
    pub fn new(
        state: Arc<RwLock<CoreState>>,
        event_bus: Option<Arc<EventBus>>,
        registry: Option<Arc<Registry>>,
        capability_registry: Option<Arc<CapabilityRegistry>>,
        service_registry: Option<Arc<ServiceRegistry>>,
        task_manager: Option<Arc<TaskManager>>,
        scheduler: Option<Arc<Scheduler>>,
        health_monitor: Option<Arc<HealthMonitor>>,
    ) -> Self {
        Self {
            state,
            event_bus,
            registry,
            capability_registry,
            service_registry,
            task_manager,
            scheduler,
            health_monitor,
        }
    }

    pub async fn start(&self) -> Result<()> {
        info!("Diagnostics started");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Diagnostics stopped");
        Ok(())
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub async fn status(&self) -> Result<StatusReport> {
        let state = self.state.read().await;
        
        let registry_count = self.registry.as_ref().map(|r| r.count()).unwrap_or(0);
        let capability_count = self.capability_registry.as_ref().map(|r| r.count()).unwrap_or(0);
        let service_count = self.service_registry.as_ref().map(|r| r.list_services().len()).unwrap_or(0);
        let running_services = self.service_registry.as_ref().map(|r| r.list_running().len()).unwrap_or(0);
        let task_count = self.task_manager.as_ref().map(|m| m.count()).unwrap_or(0);
        let queued_tasks = self.task_manager.as_ref().map(|m| m.count_by_status(TaskStatus::Queued)).unwrap_or(0);
        let running_tasks = self.task_manager.as_ref().map(|m| m.count_by_status(TaskStatus::Running)).unwrap_or(0);
        let scheduled_count = self.scheduler.as_ref().map(|s| s.list_enabled().len()).unwrap_or(0);
        
        let health = match &self.health_monitor {
            Some(h) => h.check_all().await?,
            None => SystemHealth {
                overall: HealthStatus::Unknown,
                components: HashMap::new(),
                checked_at: Utc::now(),
                uptime_secs: 0,
            },
        };

        Ok(StatusReport {
            instance_id: state.instance_id,
            version: state.version.clone(),
            started_at: state.started_at,
            status: format!("{:?}", state.status),
            uptime_secs: health.uptime_secs,
            registry_entries: registry_count,
            capabilities: capability_count,
            services_total: service_count,
            services_running: running_services,
            tasks_total: task_count,
            tasks_queued: queued_tasks,
            tasks_running: running_tasks,
            scheduled_tasks: scheduled_count,
            health_overall: health.overall,
            health_checked_at: health.checked_at,
        })
    }

    pub async fn registry_list(&self, entry_type: Option<RegistryEntryType>) -> Result<Vec<RegistryEntry>> {
        let registry = self.registry.as_ref().ok_or_else(|| anyhow::anyhow!("Registry not available"))?;
        
        if let Some(t) = entry_type {
            Ok(registry.list_by_type(t))
        } else {
            Ok(registry.list_all())
        }
    }

    pub async fn capabilities_list(&self, category: Option<String>) -> Result<Vec<RegisteredCapability>> {
        let registry = self.capability_registry.as_ref().ok_or_else(|| anyhow::anyhow!("Capability Registry not available"))?;
        
        if let Some(cat) = category {
            if let Ok(category) = cat.parse::<james_capabilities::CapabilityCategory>() {
                Ok(registry.list_by_category(category))
            } else {
                Err(anyhow::anyhow!("Invalid category"))
            }
        } else {
            Ok(registry.list_all())
        }
    }

    pub async fn events_list(&self, _limit: usize) -> Result<Vec<Event>> {
        Err(anyhow::anyhow!(
            "event history is not yet available (UNIMPLEMENTED: no event store wired)"
        ))
    }

    pub async fn export(&self, format: ExportFormat) -> Result<String> {
        let status = self.status().await?;
        
        match format {
            ExportFormat::Json => Ok(serde_json::to_string_pretty(&status)?),
            ExportFormat::Yaml => Ok(serde_yaml::to_string(&status)?),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub instance_id: Uuid,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub status: String,
    pub uptime_secs: u64,
    pub registry_entries: usize,
    pub capabilities: usize,
    pub services_total: usize,
    pub services_running: usize,
    pub tasks_total: usize,
    pub tasks_queued: usize,
    pub tasks_running: usize,
    pub scheduled_tasks: usize,
    pub health_overall: HealthStatus,
    pub health_checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Yaml,
}

#[derive(Parser)]
#[command(name = "james-core", version, about = "JAMES Core Diagnostics CLI")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Status,
    Version,
    Registry { 
        #[arg(short, long)]
        type_: Option<String>,
    },
    Capabilities {
        #[arg(short, long)]
        category: Option<String>,
    },
    Events {
        #[arg(short, long, default_value = "50")]
        limit: usize,
    },
    Health,
    Export {
        #[arg(short, long, default_value = "json")]
        format: String,
    },
}

impl Diagnostics {
    pub async fn run_cli(&self) -> Result<()> {
        let cli = Cli::parse();
        
        match cli.command {
            Commands::Status => {
                let status = self.status().await?;
                Self::print_status(&status);
            }
            Commands::Version => {
                let status = self.status().await?;
                println!("JAMES Core v{}", status.version);
                println!("Instance: {}", status.instance_id);
                println!("Started: {}", status.started_at);
                println!("Status: {}", status.status);
            }
            Commands::Registry { type_ } => {
                let entry_type = type_.and_then(|t| t.parse::<RegistryEntryType>().ok());
                let entries = self.registry_list(entry_type).await?;
                Self::print_registry(&entries);
            }
            Commands::Capabilities { category } => {
                let caps = self.capabilities_list(category).await?;
                Self::print_capabilities(&caps);
            }
            Commands::Events { limit } => {
                let events = self.events_list(limit).await?;
                Self::print_events(&events);
            }
            Commands::Health => {
                if let Some(monitor) = &self.health_monitor {
                    let health = monitor.check_all().await?;
                    Self::print_health(&health);
                } else {
                    println!("Health monitor not available");
                }
            }
            Commands::Export { format } => {
                let fmt = match format.to_lowercase().as_str() {
                    "json" => ExportFormat::Json,
                    "yaml" => ExportFormat::Yaml,
                    _ => ExportFormat::Json,
                };
                let output = self.export(fmt).await?;
                println!("{}", output);
            }
        }
        
        Ok(())
    }

    fn print_status(status: &StatusReport) {
        println!("{}", "=".repeat(50));
        println!("JAMES Core Status");
        println!("{}", "=".repeat(50));
        println!("Instance ID: {}", status.instance_id);
        println!("Version:     {}", status.version);
        println!("Started:     {}", status.started_at.format("%Y-%m-%d %H:%M:%S UTC"));
        println!("Status:      {}", status.status);
        println!("Uptime:      {}s", status.uptime_secs);
        println!();
        println!("Registry:    {} entries", status.registry_entries);
        println!("Capabilities: {}", status.capabilities);
        println!("Services:    {} total, {} running", status.services_total, status.services_running);
        println!("Tasks:       {} total, {} queued, {} running", status.tasks_total, status.tasks_queued, status.tasks_running);
        println!("Scheduled:   {} tasks", status.scheduled_tasks);
        println!("Health:      {:?} (checked: {})", status.health_overall, status.health_checked_at.format("%H:%M:%S"));
        println!("{}", "=".repeat(50));
    }

    fn print_registry(entries: &[RegistryEntry]) {
        #[derive(Tabled)]
        struct RegistryRow {
            #[tabled(rename = "ID")]
            id: String,
            #[tabled(rename = "Type")]
            entry_type: String,
            #[tabled(rename = "Name")]
            name: String,
            #[tabled(rename = "Version")]
            version: String,
            #[tabled(rename = "Status")]
            status: String,
            #[tabled(rename = "Provider")]
            provider: String,
        }

        let rows: Vec<RegistryRow> = entries.iter().map(|e| RegistryRow {
            id: e.id.to_string(),
            entry_type: format!("{:?}", e.entry_type),
            name: e.name.clone(),
            version: e.version.clone(),
            status: format!("{:?}", e.status),
            provider: e.provider.clone(),
        }).collect();

        if rows.is_empty() {
            println!("No registry entries found");
        } else {
            println!("{}", Table::new(rows).to_string());
        }
    }

    fn print_capabilities(caps: &[RegisteredCapability]) {
        #[derive(Tabled)]
        struct CapabilityRow {
            #[tabled(rename = "ID")]
            id: String,
            #[tabled(rename = "Name")]
            name: String,
            #[tabled(rename = "Category")]
            category: String,
            #[tabled(rename = "Version")]
            version: String,
            #[tabled(rename = "Provider")]
            provider: String,
            #[tabled(rename = "Risk")]
            risk: String,
            #[tabled(rename = "Status")]
            status: String,
            #[tabled(rename = "Usage")]
            usage: u64,
        }

        let rows: Vec<CapabilityRow> = caps.iter().map(|c| CapabilityRow {
            id: c.definition.id.clone(),
            name: c.definition.name.clone(),
            category: format!("{:?}", c.definition.category),
            version: c.definition.version.clone(),
            provider: c.definition.provider.clone(),
            risk: format!("{:?}", c.definition.risk_level),
            status: format!("{:?}", c.status),
            usage: c.usage_count,
        }).collect();

        if rows.is_empty() {
            println!("No capabilities found");
        } else {
            println!("{}", Table::new(rows).to_string());
        }
    }

    fn print_events(events: &[Event]) {
        #[derive(Tabled)]
        struct EventRow {
            #[tabled(rename = "Time")]
            time: String,
            #[tabled(rename = "Type")]
            event_type: String,
            #[tabled(rename = "Source")]
            source: String,
            #[tabled(rename = "Severity")]
            severity: String,
        }

        let rows: Vec<EventRow> = events.iter().map(|e| EventRow {
            time: e.timestamp.format("%H:%M:%S%.3f").to_string(),
            event_type: e.event_type.clone(),
            source: e.source.clone(),
            severity: format!("{:?}", e.severity),
        }).collect();

        if rows.is_empty() {
            println!("No events found");
        } else {
            println!("{}", Table::new(rows).to_string());
        }
    }

    fn print_health(health: &SystemHealth) {
        println!("{}", "=".repeat(60));
        println!("JAMES Core Health Report");
        println!("{}", "=".repeat(60));
        println!("Overall: {:?}", health.overall);
        println!("Checked: {}", health.checked_at.format("%Y-%m-%d %H:%M:%S UTC"));
        println!("Uptime:  {}s", health.uptime_secs);
        println!();

        #[derive(Tabled)]
        struct HealthRow {
            #[tabled(rename = "Component")]
            name: String,
            #[tabled(rename = "Status")]
            status: String,
            #[tabled(rename = "Message")]
            message: String,
            #[tabled(rename = "Response (ms)")]
            response_ms: u64,
        }

        let rows: Vec<HealthRow> = health.components.values().map(|c| HealthRow {
            name: c.name.clone(),
            status: format!("{:?}", c.status),
            message: c.message.clone().unwrap_or_default(),
            response_ms: c.response_time_ms,
        }).collect();

        if rows.is_empty() {
            println!("No health checks configured");
        } else {
            println!("{}", Table::new(rows).to_string());
        }
        println!("{}", "=".repeat(60));
    }
}

use std::collections::HashMap;
use serde_yaml;

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new(
            Arc::new(RwLock::new(CoreState {
                instance_id: Uuid::nil(),
                version: "0.1.0".to_string(),
                started_at: Utc::now(),
                // CoreStatus has no Unknown variant (see F1-02 unification);
                // Stopped here means "no core attached".
                status: CoreStatus::Stopped,
            })),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diagnostics_status() {
        let state = Arc::new(RwLock::new(CoreState {
            instance_id: Uuid::now_v7(),
            version: "0.1.0".to_string(),
            started_at: Utc::now(),
            status: CoreStatus::Running,
        }));

        let diagnostics = Diagnostics::new(state, None, None, None, None, None, None, None);
        diagnostics.start().await.unwrap();

        let status = diagnostics.status().await.unwrap();
        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.status, "Running");
    }
}