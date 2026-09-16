use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use dashmap::DashMap;

use james_events::{EventEnvelope, builtin_events, create_system_event, EventBus};
use james_registry::{Registry, RegistryEntry, RegistryEntryType, RegistryStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServiceStatus {
    Running,
    Starting,
    Stopping,
    Stopped,
    Failed,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    pub id: String,
    pub name: String,
    pub version: String,
    pub provider: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<String>,
    pub config_schema: Option<serde_json::Value>,
    pub default_config: Option<serde_json::Value>,
    pub health_check_endpoint: Option<String>,
    pub restart_policy: RestartPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RestartPolicy {
    Never,
    OnFailure,
    Always,
    UnlessStopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub definition: ServiceDefinition,
    pub status: ServiceStatus,
    pub config: serde_json::Value,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub restart_count: u32,
    pub last_health_check: Option<DateTime<Utc>>,
    pub health_status: Option<ServiceHealth>,
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServiceHealth {
    Healthy,
    Unhealthy,
    Degraded,
    Unknown,
}

pub struct ServiceRegistry {
    services: DashMap<String, ServiceInstance>,
    definitions: DashMap<String, ServiceDefinition>,
    registry: Option<Arc<Registry>>,
    event_bus: Option<Arc<EventBus>>,
    running: Arc<RwLock<bool>>,
}



impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: DashMap::new(),
            definitions: DashMap::new(),
            registry: None,
            event_bus: None,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_registry(mut self, registry: Arc<Registry>) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Service Registry started");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        
        for entry in self.services.iter() {
            let id = entry.key().clone();
            let _ = self.stop_service(&id).await;
        }

        info!("Service Registry stopped");
        Ok(())
    }

    pub fn set_registry(&mut self, registry: Arc<Registry>) {
        self.registry = Some(registry);
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub fn register_definition(&self, definition: ServiceDefinition) -> Result<()> {
        if self.definitions.contains_key(&definition.id) {
            anyhow::bail!("Service definition '{}' already registered", definition.id);
        }
        
        self.definitions.insert(definition.id.clone(), definition);
        Ok(())
    }

    pub fn unregister_definition(&self, id: &str) -> Result<bool> {
        if self.services.contains_key(id) {
            anyhow::bail!("Cannot unregister definition '{}': service instance exists", id);
        }
        Ok(self.definitions.remove(id).is_some())
    }

    pub fn get_definition(&self, id: &str) -> Option<ServiceDefinition> {
        self.definitions.get(id).map(|d| d.clone())
    }

    pub fn list_definitions(&self) -> Vec<ServiceDefinition> {
        self.definitions.iter().map(|d| d.clone()).collect()
    }

    pub async fn start_service(&self, id: &str, config: Option<serde_json::Value>) -> Result<ServiceInstance> {
        let definition = self.definitions.get(id)
            .ok_or_else(|| anyhow::anyhow!("Service definition '{}' not found", id))?
            .clone();

        if self.services.contains_key(id) {
            anyhow::bail!("Service '{}' already running", id);
        }

        for dep in &definition.dependencies {
            if !self.services.contains_key(dep) {
                anyhow::bail!("Dependency '{}' not running", dep);
            }
        }

        let mut instance = ServiceInstance {
            definition: definition.clone(),
            status: ServiceStatus::Starting,
            config: config.unwrap_or_else(|| definition.default_config.clone().unwrap_or(serde_json::Value::Null)),
            started_at: None,
            stopped_at: None,
            restart_count: 0,
            last_health_check: None,
            health_status: Some(ServiceHealth::Unknown),
            process_id: None,
        };

        self.services.insert(id.to_string(), instance.clone());
        self.emit_service_event(builtin_events::SERVICE_STARTED, id, &instance).await;

        instance.status = ServiceStatus::Running;
        instance.started_at = Some(Utc::now());
        instance.health_status = Some(ServiceHealth::Healthy);
        
        self.services.insert(id.to_string(), instance.clone());
        self.emit_service_event(builtin_events::SERVICE_HEALTH_CHANGED, id, &instance).await;

        if let Some(reg) = &self.registry {
            let entry = RegistryEntry {
                id: Uuid::nil(),
                entry_type: RegistryEntryType::Service,
                name: id.to_string(),
                version: definition.version.clone(),
                provider: definition.provider.clone(),
                status: RegistryStatus::Active,
                capabilities: definition.capabilities.clone(),
                dependencies: definition.dependencies.iter().filter_map(|d| Uuid::try_parse(d).ok()).collect(),
                metadata: serde_json::to_value(&definition)?,
                registered_at: DateTime::UNIX_EPOCH,
                last_seen: DateTime::UNIX_EPOCH,
                heartbeat_interval_secs: Some(30),
            };
            let _ = reg.register(entry).await;
        }

        info!("Service '{}' started", id);
        Ok(instance)
    }

    pub async fn stop_service(&self, id: &str) -> Result<bool> {
        if let Some((_, mut instance)) = self.services.remove(id) {
            instance.status = ServiceStatus::Stopping;
            self.emit_service_event(builtin_events::SERVICE_STOPPED, id, &instance).await;

            instance.status = ServiceStatus::Stopped;
            instance.stopped_at = Some(Utc::now());
            instance.health_status = Some(ServiceHealth::Unknown);

            self.emit_service_event(builtin_events::SERVICE_STOPPED, id, &instance).await;

            if let Some(reg) = &self.registry {
                // Services register under their string id as entry name
                // (names are globally unique), so resolve the real entry id
                // instead of unregistering a nil UUID.
                if let Some(entry) = reg.get_by_name(id) {
                    let _ = reg.unregister(entry.id).await;
                }
            }

            info!("Service '{}' stopped", id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn restart_service(&self, id: &str) -> Result<ServiceInstance> {
        let config = self.services.get(id).map(|s| s.config.clone());
        let _ = self.stop_service(id).await;
        self.start_service(id, config).await
    }

    pub fn get_service(&self, id: &str) -> Option<ServiceInstance> {
        self.services.get(id).map(|s| s.clone())
    }

    pub fn list_services(&self) -> Vec<ServiceInstance> {
        self.services.iter().map(|s| s.clone()).collect()
    }

    pub fn list_running(&self) -> Vec<ServiceInstance> {
        self.services
            .iter()
            .filter(|s| s.status == ServiceStatus::Running)
            .map(|s| s.clone())
            .collect()
    }

    pub async fn update_health(&self, id: &str, health: ServiceHealth) -> Result<bool> {
        if let Some(mut instance) = self.services.get_mut(id) {
            instance.health_status = Some(health.clone());
            instance.last_health_check = Some(Utc::now());

            match health {
                ServiceHealth::Healthy => {
                    if instance.status == ServiceStatus::Degraded {
                        instance.status = ServiceStatus::Running;
                    }
                }
                ServiceHealth::Unhealthy => {
                    instance.status = ServiceStatus::Failed;
                }
                ServiceHealth::Degraded => {
                    instance.status = ServiceStatus::Degraded;
                }
                _ => {}
            }

            self.emit_service_event(builtin_events::SERVICE_HEALTH_CHANGED, id, &instance).await;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_status(&self, id: &str, status: ServiceStatus) -> Result<bool> {
        if let Some(mut instance) = self.services.get_mut(id) {
            instance.status = status;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get_process_id(&self, id: &str) -> Option<u32> {
        self.services.get(id).and_then(|s| s.process_id)
    }

    pub fn set_process_id(&self, id: &str, pid: u32) -> Result<bool> {
        if let Some(mut instance) = self.services.get_mut(id) {
            instance.process_id = Some(pid);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn emit_service_event(&self, event_type: &str, service_id: &str, instance: &ServiceInstance) {
        if let Some(bus) = &self.event_bus {
            let event = create_system_event(event_type, "service-registry")
                .with_payload(serde_json::json!({
                    "service_id": service_id,
                    "service_name": instance.definition.name,
                    "status": instance.status,
                    "health": instance.health_status,
                }));
            let _ = bus.publish_envelope(EventEnvelope::new(event)).await;
        }
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_registry_definition() {
        let registry = ServiceRegistry::new();
        registry.start().await.unwrap();

        let def = ServiceDefinition {
            id: "test-service".to_string(),
            name: "Test Service".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "A test service".to_string(),
            capabilities: vec!["test.cap".to_string()],
            dependencies: vec![],
            config_schema: None,
            default_config: Some(serde_json::json!({"setting": "value"})),
            health_check_endpoint: None,
            restart_policy: RestartPolicy::OnFailure,
        };

        registry.register_definition(def.clone()).unwrap();
        let retrieved = registry.get_definition("test-service").unwrap();
        assert_eq!(retrieved.name, "Test Service");
    }

    #[tokio::test]
    async fn test_service_start_stop() {
        let registry = ServiceRegistry::new();
        registry.start().await.unwrap();

        let def = ServiceDefinition {
            id: "test-service".to_string(),
            name: "Test Service".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "A test service".to_string(),
            capabilities: vec![],
            dependencies: vec![],
            config_schema: None,
            default_config: None,
            health_check_endpoint: None,
            restart_policy: RestartPolicy::Never,
        };

        registry.register_definition(def).unwrap();

        let instance = registry.start_service("test-service", None).await.unwrap();
        assert_eq!(instance.status, ServiceStatus::Running);
        assert!(instance.started_at.is_some());

        let stopped = registry.stop_service("test-service").await.unwrap();
        assert!(stopped);
        assert!(registry.get_service("test-service").is_none());
    }

    #[tokio::test]
    async fn test_service_health() {
        let registry = ServiceRegistry::new();
        registry.start().await.unwrap();

        let def = ServiceDefinition {
            id: "test-service".to_string(),
            name: "Test Service".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "A test service".to_string(),
            capabilities: vec![],
            dependencies: vec![],
            config_schema: None,
            default_config: None,
            health_check_endpoint: None,
            restart_policy: RestartPolicy::Never,
        };

        registry.register_definition(def).unwrap();
        registry.start_service("test-service", None).await.unwrap();

        registry.update_health("test-service", ServiceHealth::Degraded).await.unwrap();
        let instance = registry.get_service("test-service").unwrap();
        assert_eq!(instance.status, ServiceStatus::Degraded);
        assert_eq!(instance.health_status, Some(ServiceHealth::Degraded));

        registry.update_health("test-service", ServiceHealth::Unhealthy).await.unwrap();
        let instance = registry.get_service("test-service").unwrap();
        assert_eq!(instance.status, ServiceStatus::Failed);
    }

    #[tokio::test]
    async fn test_service_dependencies() {
        let registry = ServiceRegistry::new();
        registry.start().await.unwrap();

        let dep_def = ServiceDefinition {
            id: "dependency".to_string(),
            name: "Dependency".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "A dependency".to_string(),
            capabilities: vec![],
            dependencies: vec![],
            config_schema: None,
            default_config: None,
            health_check_endpoint: None,
            restart_policy: RestartPolicy::Never,
        };

        let svc_def = ServiceDefinition {
            id: "dependent".to_string(),
            name: "Dependent".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "A dependent service".to_string(),
            capabilities: vec![],
            dependencies: vec!["dependency".to_string()],
            config_schema: None,
            default_config: None,
            health_check_endpoint: None,
            restart_policy: RestartPolicy::Never,
        };

        registry.register_definition(dep_def).unwrap();
        registry.register_definition(svc_def).unwrap();

        let result = registry.start_service("dependent", None).await;
        assert!(result.is_err());

        registry.start_service("dependency", None).await.unwrap();
        let instance = registry.start_service("dependent", None).await.unwrap();
        assert_eq!(instance.status, ServiceStatus::Running);
    }

    #[tokio::test]
    async fn test_service_events_published() {
        let bus = Arc::new(EventBus::new(100));
        bus.start().await.unwrap();
        let registry = ServiceRegistry::new().with_event_bus(bus.clone());
        registry.start().await.unwrap();

        let mut rx = bus.subscribe(builtin_events::SERVICE_STARTED);

        let def = ServiceDefinition {
            id: "event-service".to_string(),
            name: "Event Service".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "Emits events".to_string(),
            capabilities: vec![],
            dependencies: vec![],
            config_schema: None,
            default_config: None,
            health_check_endpoint: None,
            restart_policy: RestartPolicy::Never,
        };

        registry.register_definition(def).unwrap();
        registry.start_service("event-service", None).await.unwrap();

        let envelope = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            rx.recv(),
        )
        .await
        .expect("SERVICE_STARTED event timeout")
        .expect("event channel closed");
        assert_eq!(envelope.event.event_type, builtin_events::SERVICE_STARTED);
        assert_eq!(envelope.event.payload["service_id"], "event-service");
    }
}