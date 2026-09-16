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
use james_capabilities::{CapabilityRegistry, CapabilityDefinition, CapabilityCategory, RiskLevel, ExecutionTarget};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RegistryEntryType {
    Service,
    Module,
    Plugin,
    Device,
    Capability,
    Workflow,
    Agent,
    Model,
}

impl std::str::FromStr for RegistryEntryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "service" => Ok(Self::Service),
            "module" => Ok(Self::Module),
            "plugin" => Ok(Self::Plugin),
            "device" => Ok(Self::Device),
            "capability" => Ok(Self::Capability),
            "workflow" => Ok(Self::Workflow),
            "agent" => Ok(Self::Agent),
            "model" => Ok(Self::Model),
            other => Err(format!("unknown registry entry type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    pub id: Uuid,
    pub entry_type: RegistryEntryType,
    pub name: String,
    pub version: String,
    pub provider: String,
    pub status: RegistryStatus,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<Uuid>,
    pub metadata: serde_json::Value,
    pub registered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub heartbeat_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RegistryStatus {
    Active,
    Inactive,
    Starting,
    Stopping,
    Failed,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryQuery {
    pub entry_type: Option<RegistryEntryType>,
    pub name: Option<String>,
    pub status: Option<RegistryStatus>,
    pub capability: Option<String>,
    pub provider: Option<String>,
}

pub struct Registry {
    entries: DashMap<Uuid, RegistryEntry>,
    by_name: DashMap<String, Uuid>,
    by_type: DashMap<RegistryEntryType, Vec<Uuid>>,
    event_bus: Option<Arc<EventBus>>,
    capability_registry: Option<Arc<CapabilityRegistry>>,
    running: Arc<RwLock<bool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            by_name: DashMap::new(),
            by_type: DashMap::new(),
            event_bus: None,
            capability_registry: None,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_capability_registry(mut self, capability_registry: Arc<CapabilityRegistry>) -> Self {
        self.capability_registry = Some(capability_registry);
        self
    }

    pub async fn start(&self) -> Result<()> {
        info!("Registry started");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Registry stopped");
        Ok(())
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub async fn register(&self, mut entry: RegistryEntry) -> Result<Uuid> {
        if entry.id == Uuid::nil() {
            entry.id = Uuid::now_v7();
        }
        
        let now = Utc::now();
        if entry.registered_at == DateTime::UNIX_EPOCH {
            entry.registered_at = now;
        }
        entry.last_seen = now;

        if entry.status == RegistryStatus::Unknown {
            entry.status = RegistryStatus::Active;
        }

        if self.by_name.contains_key(&entry.name) {
            anyhow::bail!("Entry with name '{}' already exists", entry.name);
        }

        self.by_name.insert(entry.name.clone(), entry.id);
        self.by_type.entry(entry.entry_type.clone()).or_default().push(entry.id);
        self.entries.insert(entry.id, entry.clone());

        // Auto-register capabilities for modules
        if entry.entry_type == RegistryEntryType::Module {
            if let Some(cap_reg) = &self.capability_registry {
                for cap_id in &entry.capabilities {
                    // Check if capability already exists
                    if cap_reg.get(cap_id).is_none() {
                        use james_capabilities::{CapabilityDefinition, CapabilityCategory, RiskLevel, ExecutionTarget};
                        let definition = CapabilityDefinition {
                            id: cap_id.clone(),
                            name: cap_id.clone(),
                            category: CapabilityCategory::Custom("module".to_string()),
                            version: "1.0.0".to_string(),
                            provider: entry.provider.clone(),
                            description: format!("Capability provided by module {}", entry.name),
                            risk_level: RiskLevel::Low,
                            required_permissions: vec![],
                            dependencies: vec![],
                            input_schema: None,
                            output_schema: None,
                            execution_target: ExecutionTarget::Local,
                            tags: vec![],
                            deprecated: false,
                            experimental: false,
                        };
                        let _ = cap_reg.register(definition, entry.provider.clone()).await;
                    }
                }
            }
        }

        if let Some(bus) = &self.event_bus {
            let event = create_system_event(builtin_events::REGISTRY_CHANGED, "registry")
                .with_payload(serde_json::json!({
                    "action": "registered",
                    "entry_id": entry.id,
                    "entry_type": entry.entry_type,
                    "name": entry.name,
                }));
            let _ = bus.publish_envelope(EventEnvelope::new(event)).await;
        }

        Ok(entry.id)
    }

    pub async fn unregister(&self, id: Uuid) -> Result<bool> {
        if let Some((_, entry)) = self.entries.remove(&id) {
            self.by_name.remove(&entry.name);
            
            if let Some(mut vec) = self.by_type.get_mut(&entry.entry_type) {
                vec.retain(|&x| x != id);
            }

if let Some(bus) = &self.event_bus {
            let event = create_system_event(builtin_events::REGISTRY_CHANGED, "registry")
                .with_payload(serde_json::json!({
                    "action": "unregistered",
                    "entry_id": id,
                    "entry_type": entry.entry_type,
                    "name": entry.name,
                }));
            let _ = bus.publish_envelope(EventEnvelope::new(event)).await;
        }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get(&self, id: Uuid) -> Option<RegistryEntry> {
        self.entries.get(&id).map(|e| e.clone())
    }

    pub fn get_by_name(&self, name: &str) -> Option<RegistryEntry> {
        self.by_name.get(name).as_ref().and_then(|id| self.entries.get(id).map(|e| e.clone()))
    }

    pub fn query(&self, query: RegistryQuery) -> Vec<RegistryEntry> {
        let mut candidates: Vec<RegistryEntry> = self.entries.iter().map(|e| e.clone()).collect();

        if let Some(entry_type) = query.entry_type {
            candidates.retain(|e| e.entry_type == entry_type);
        }

        if let Some(name) = query.name {
            candidates.retain(|e| e.name.contains(&name));
        }

        if let Some(status) = query.status {
            candidates.retain(|e| e.status == status);
        }

        if let Some(capability) = query.capability {
            candidates.retain(|e| e.capabilities.iter().any(|c| c.contains(&capability)));
        }

        if let Some(provider) = query.provider {
            candidates.retain(|e| e.provider.contains(&provider));
        }

        candidates
    }

    pub fn list_all(&self) -> Vec<RegistryEntry> {
        self.entries.iter().map(|e| e.clone()).collect()
    }

    pub fn list_by_type(&self, entry_type: RegistryEntryType) -> Vec<RegistryEntry> {
        self.by_type
            .get(&entry_type)
            .map(|ids| ids.iter().filter_map(|id| self.entries.get(id).map(|e| e.clone())).collect())
            .unwrap_or_default()
    }

    pub fn update_status(&self, id: Uuid, status: RegistryStatus) -> Result<bool> {
        if let Some(mut entry) = self.entries.get_mut(&id) {
            entry.status = status;
            entry.last_seen = Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_heartbeat(&self, id: Uuid) -> Result<bool> {
        if let Some(mut entry) = self.entries.get_mut(&id) {
            entry.last_seen = Utc::now();
            if entry.status == RegistryStatus::Inactive || entry.status == RegistryStatus::Unknown {
                entry.status = RegistryStatus::Active;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_capabilities(&self, id: Uuid, capabilities: Vec<String>) -> Result<bool> {
        if let Some(mut entry) = self.entries.get_mut(&id) {
            entry.capabilities = capabilities;
            entry.last_seen = Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn count_by_type(&self, entry_type: RegistryEntryType) -> usize {
        self.by_type.get(&entry_type).map(|v| v.len()).unwrap_or(0)
    }

    pub async fn cleanup_stale(&self, max_age: Duration) -> usize {
        let now = Utc::now();
        let mut removed = 0;

        for entry in self.entries.iter() {
            let age = now.signed_duration_since(entry.last_seen);
            if age.to_std().unwrap_or_default() > max_age {
                if entry.status != RegistryStatus::Active && entry.status != RegistryStatus::Starting {
                    let id = entry.id;
                    drop(entry);
                    self.unregister(id).await.ok();
                    removed += 1;
                }
            }
        }

        removed
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_register_unregister() {
        let registry = Registry::new();
        
        let entry = RegistryEntry {
            id: Uuid::nil(),
            entry_type: RegistryEntryType::Service,
            name: "test-service".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            status: RegistryStatus::Active,
            capabilities: vec!["test.capability".to_string()],
            dependencies: vec![],
            metadata: serde_json::Value::Null,
            registered_at: DateTime::UNIX_EPOCH,
            last_seen: DateTime::UNIX_EPOCH,
            heartbeat_interval_secs: Some(30),
        };

        let id = registry.register(entry).await.unwrap();
        assert_ne!(id, Uuid::nil());

        let retrieved = registry.get(id).unwrap();
        assert_eq!(retrieved.name, "test-service");
        assert_eq!(retrieved.entry_type, RegistryEntryType::Service);

        let found = registry.get_by_name("test-service").unwrap();
        assert_eq!(found.id, id);

        let unregistered = registry.unregister(id).await.unwrap();
        assert!(unregistered);

        assert!(registry.get(id).is_none());
        assert!(registry.get_by_name("test-service").is_none());
    }

    #[tokio::test]
    async fn test_registry_query() {
        let registry = Registry::new();
        
        let entry1 = RegistryEntry {
            id: Uuid::nil(),
            entry_type: RegistryEntryType::Service,
            name: "service-a".to_string(),
            version: "1.0.0".to_string(),
            provider: "provider-1".to_string(),
            status: RegistryStatus::Active,
            capabilities: vec!["cap.a".to_string()],
            dependencies: vec![],
            metadata: serde_json::Value::Null,
            registered_at: DateTime::UNIX_EPOCH,
            last_seen: DateTime::UNIX_EPOCH,
            heartbeat_interval_secs: Some(30),
        };

        let entry2 = RegistryEntry {
            id: Uuid::nil(),
            entry_type: RegistryEntryType::Module,
            name: "module-b".to_string(),
            version: "2.0.0".to_string(),
            provider: "provider-2".to_string(),
            status: RegistryStatus::Inactive,
            capabilities: vec!["cap.b".to_string()],
            dependencies: vec![],
            metadata: serde_json::Value::Null,
            registered_at: DateTime::UNIX_EPOCH,
            last_seen: DateTime::UNIX_EPOCH,
            heartbeat_interval_secs: Some(30),
        };

        registry.register(entry1).await.unwrap();
        registry.register(entry2).await.unwrap();

        let services = registry.query(RegistryQuery {
            entry_type: Some(RegistryEntryType::Service),
            name: None,
            status: None,
            capability: None,
            provider: None,
        });
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "service-a");

        let active = registry.query(RegistryQuery {
            entry_type: None,
            name: None,
            status: Some(RegistryStatus::Active),
            capability: None,
            provider: None,
        });
        assert_eq!(active.len(), 1);

        let by_cap = registry.query(RegistryQuery {
            entry_type: None,
            name: None,
            status: None,
            capability: Some("cap.a".to_string()),
            provider: None,
        });
        assert_eq!(by_cap.len(), 1);
    }

    #[tokio::test]
    async fn test_duplicate_name_fails() {
        let registry = Registry::new();
        
        let entry = RegistryEntry {
            id: Uuid::nil(),
            entry_type: RegistryEntryType::Service,
            name: "duplicate".to_string(),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            status: RegistryStatus::Active,
            capabilities: vec![],
            dependencies: vec![],
            metadata: serde_json::Value::Null,
            registered_at: DateTime::UNIX_EPOCH,
            last_seen: DateTime::UNIX_EPOCH,
            heartbeat_interval_secs: Some(30),
        };

        registry.register(entry.clone()).await.unwrap();
        let result = registry.register(entry).await;
        assert!(result.is_err());
    }
}