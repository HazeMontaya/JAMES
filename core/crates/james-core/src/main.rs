use james_core::{JamesCore, CoreConfig};
use james_diagnostics::Diagnostics;
use james_events::EventBus;
use james_registry::Registry;
use james_capabilities::CapabilityRegistry;
use james_services::ServiceRegistry;
use james_tasks::TaskManager;
use james_scheduler::Scheduler;
use james_health::HealthMonitor;
use tracing_subscriber::{fmt, EnvFilter};
use anyhow::Result;
use std::sync::Arc;
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    info!("Starting JAMES Core v{}", env!("CARGO_PKG_VERSION"));

    let config = CoreConfig::load().unwrap_or_else(|e| {
        error!("Failed to load config: {}, using defaults", e);
        CoreConfig::default()
    });

    if let Err(e) = config.save() {
        error!("Failed to save config: {}", e);
    }

    let event_bus = Arc::new(EventBus::new(config.event_bus_buffer_size));
    let registry = Arc::new(Registry::new());
    let capability_registry = Arc::new(CapabilityRegistry::new());
    let service_registry = Arc::new(ServiceRegistry::new());
    let task_manager = Arc::new(TaskManager::new(Some(event_bus.clone())));
    let scheduler = Arc::new(Scheduler::new(task_manager.clone()));
    let health_monitor = Arc::new(HealthMonitor::new(
        Arc::new(tokio::sync::RwLock::new(james_core::CoreState {
            instance_id: uuid::Uuid::nil(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: chrono::Utc::now(),
            status: "Starting".to_string(),
        })),
        Some(event_bus.clone()),
        Some(registry.clone()),
        Some(service_registry.clone()),
        Some(task_manager.clone()),
        Some(scheduler.clone()),
    ));
    let diagnostics = Arc::new(Diagnostics::new(
        Arc::new(tokio::sync::RwLock::new(james_core::CoreState {
            instance_id: uuid::Uuid::nil(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: chrono::Utc::now(),
            status: "Starting".to_string(),
        })),
        Some(event_bus.clone()),
        Some(registry.clone()),
        Some(capability_registry.clone()),
        Some(service_registry.clone()),
        Some(task_manager.clone()),
        Some(scheduler.clone()),
        Some(health_monitor.clone()),
    ));

    let mut core = JamesCore::new(config).await?;

    core.event_bus = event_bus.clone();
    core.registry = registry.clone();
    core.capability_registry = capability_registry.clone();
    core.service_registry = service_registry.clone();
    core.task_manager = task_manager.clone();
    core.scheduler = scheduler.clone();
    core.health_monitor = health_monitor.clone();
    core.diagnostics = diagnostics.clone();

    registry.set_event_bus(Arc::new(james_registry::RegistryEventBus::new().inner));
    capability_registry.set_event_bus(Arc::new(james_capabilities::CapabilityEventBus::new().inner));
    service_registry.set_event_bus(Arc::new(james_services::ServiceEventBus::new().inner));
    task_manager.set_event_bus(event_bus.clone());
    scheduler.set_event_bus(event_bus.clone());
    health_monitor.set_event_bus(Arc::new(james_health::HealthEventBus::new().inner));
    diagnostics.set_event_bus(Arc::new(james_diagnostics::EventBus::new()));

    core.start().await?;

    info!("JAMES Core running. Press Ctrl+C to stop.");

    signal::ctrl_c().await?;

    info!("Shutdown signal received");
    core.stop().await?;

    info!("JAMES Core stopped");
    Ok(())
}

fn init_tracing() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,james_core=debug,james_events=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    Ok(())
}