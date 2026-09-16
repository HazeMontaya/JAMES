use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, error};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use james_events::{
    Event, builtin_events,
    create_system_event, EventBus,
};
pub use james_events::CoreStatus;
use james_registry::Registry;
use james_capabilities::CapabilityRegistry;
use james_services::ServiceRegistry;
use james_tasks::TaskManager;
use james_scheduler::Scheduler;
use james_health::HealthMonitor;

mod config;
pub use config::CoreConfig;
mod logging;
pub use logging::{init_tracing, LogFields};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreState {
    pub instance_id: Uuid,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub status: CoreStatus,
}

pub struct JamesCore {
    config: CoreConfig,
    state: Arc<RwLock<CoreState>>,
    health_status: Arc<RwLock<CoreStatus>>,
    event_bus: Arc<EventBus>,
    registry: Arc<Registry>,
    capability_registry: Arc<CapabilityRegistry>,
    service_registry: Arc<ServiceRegistry>,
    task_manager: Arc<TaskManager>,
    scheduler: Arc<Scheduler>,
    health_monitor: Arc<HealthMonitor>,
    background_tasks: Vec<JoinHandle<()>>,
}

impl JamesCore {
    pub async fn new(config: CoreConfig) -> Result<Self> {
        let instance_id = Uuid::now_v7();
        let state = Arc::new(RwLock::new(CoreState {
            instance_id,
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: Utc::now(),
            status: CoreStatus::Starting,
        }));

        let health_status = Arc::new(RwLock::new(CoreStatus::Starting));

        let event_bus = Arc::new(EventBus::new(config.event_bus_buffer_size));
        let registry = Arc::new(Registry::new());
        let capability_registry = Arc::new(CapabilityRegistry::new());
        let service_registry = Arc::new(ServiceRegistry::new());
        let task_manager = Arc::new(TaskManager::new(Some(event_bus.clone())));
        let scheduler = Arc::new(Scheduler::new(task_manager.clone()));
        let health_monitor = Arc::new(HealthMonitor::new(
            health_status.clone(),
            Some(event_bus.clone()),
            Some(registry.clone()),
            Some(service_registry.clone()),
            Some(task_manager.clone()),
            Some(scheduler.clone()),
        ));

        Ok(Self {
            config,
            state,
            health_status,
            event_bus,
            registry,
            capability_registry,
            service_registry,
            task_manager,
            scheduler,
            health_monitor,
            background_tasks: Vec::new(),
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting JAMES Core v{}", env!("CARGO_PKG_VERSION"));
        
        {
            let mut state = self.state.write().await;
            state.status = CoreStatus::Running;
        }
        
        *self.health_status.write().await = CoreStatus::Running;

        self.event_bus.start().await?;
        self.registry.start().await?;
        self.capability_registry.start().await?;
        self.service_registry.start().await?;
        self.task_manager.start().await?;
        self.scheduler.start().await?;
        self.health_monitor.start().await?;

        self.emit_event(create_system_event(builtin_events::SYSTEM_STARTED, "james-core"))
            .await?;

        self.spawn_background_tasks().await?;

        info!("JAMES Core started successfully");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping JAMES Core...");
        
        {
            let mut state = self.state.write().await;
            state.status = CoreStatus::Stopping;
        }
        
        *self.health_status.write().await = CoreStatus::Stopping;

        self.emit_event(create_system_event(builtin_events::SYSTEM_STOPPING, "james-core"))
            .await?;

        for task in self.background_tasks.drain(..) {
            task.abort();
        }

        self.scheduler.stop().await?;
        self.task_manager.stop().await?;
        self.service_registry.stop().await?;
        self.capability_registry.stop().await?;
        self.registry.stop().await?;

        {
            let mut state = self.state.write().await;
            state.status = CoreStatus::Stopped;
        }

        *self.health_status.write().await = CoreStatus::Stopped;

        // SYSTEM_STOPPED must be emitted BEFORE the bus stops: since F1-01
        // publish-on-stopped-bus returns Err instead of silently dropping.
        self.emit_event(create_system_event(builtin_events::SYSTEM_STOPPED, "james-core"))
            .await?;

        self.event_bus.stop().await?;
        self.health_monitor.stop().await?;

        info!("JAMES Core stopped");
        Ok(())
    }

    async fn spawn_background_tasks(&mut self) -> Result<()> {
        let _event_bus = self.event_bus.clone();
        let health_monitor = self.health_monitor.clone();
        let interval = Duration::from_secs(self.config.health_check_interval_secs);

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;
                if let Err(e) = health_monitor.check_all().await {
                    error!("Health check failed: {}", e);
                }
            }
        });
        self.background_tasks.push(handle);

        let _event_bus2 = self.event_bus.clone();
        let scheduler = self.scheduler.clone();
        let interval2 = Duration::from_secs(self.config.scheduler_tick_interval_secs);

        let handle2 = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval2);
            loop {
                interval_timer.tick().await;
                if let Err(e) = scheduler.tick().await {
                    error!("Scheduler tick failed: {}", e);
                }
            }
        });
        self.background_tasks.push(handle2);

        Ok(())
    }

    pub async fn emit_event(&self, event: Event) -> Result<()> {
        self.event_bus.publish(event).await
    }

    pub fn event_bus(&self) -> Arc<EventBus> {
        self.event_bus.clone()
    }

    pub fn registry(&self) -> Arc<Registry> {
        self.registry.clone()
    }

    pub fn capability_registry(&self) -> Arc<CapabilityRegistry> {
        self.capability_registry.clone()
    }

    pub fn service_registry(&self) -> Arc<ServiceRegistry> {
        self.service_registry.clone()
    }

    pub fn task_manager(&self) -> Arc<TaskManager> {
        self.task_manager.clone()
    }

    pub fn scheduler(&self) -> Arc<Scheduler> {
        self.scheduler.clone()
    }

    pub fn health_monitor(&self) -> Arc<HealthMonitor> {
        self.health_monitor.clone()
    }

    pub async fn state(&self) -> CoreState {
        self.state.read().await.clone()
    }

    pub async fn status(&self) -> CoreStatus {
        self.state.read().await.status.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_core_startup_shutdown() {
        let config = CoreConfig::default();
        let mut core = JamesCore::new(config).await.unwrap();
        
        core.start().await.unwrap();
        assert_eq!(core.status().await, CoreStatus::Running);
        
        core.stop().await.unwrap();
        assert_eq!(core.status().await, CoreStatus::Stopped);
    }

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::with_default_buffer();
        bus.start().await.unwrap();

        let mut rx = bus.subscribe("test.event");
        
        let event = Event::new("test.event", "test-source")
            .with_payload(serde_json::json!({"key": "value"}));
        
        bus.publish(event).await.unwrap();
        
        let received = rx.recv().await.unwrap();
        assert_eq!(received.event.event_type, "test.event");
        assert_eq!(received.event.payload["key"], "value");
    }

    #[tokio::test]
    async fn test_event_bus_wildcard_subscribe() {
        let bus = EventBus::new(100);
        bus.start().await.unwrap();

        let mut rx = bus.subscribe_all();
        
        let event = Event::new("any.event", "test-source");
        bus.publish(event).await.unwrap();
        
        let received = rx.recv().await.unwrap();
        assert_eq!(received.event.event_type, "any.event");
    }
}