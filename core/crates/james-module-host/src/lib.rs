//! JAMES Module Host - Module lifecycle management
//!
//! This crate provides the module hosting infrastructure for JAMES.
//! It handles module discovery, validation, loading, lifecycle management,
//! and capability registration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use chrono::{DateTime, Utc};
use james_capabilities::{CapabilityCategory, CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{builtin_events, create_system_event, EventBus};
use james_registry::{Registry, RegistryEntry, RegistryEntryType, RegistryStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

pub mod abi;
mod manifest;
mod lifecycle;
mod loader;
mod permissions;

pub use manifest::{ModuleManifest, ModuleManifestValidator, ModuleType, ModulePermission, ModuleDependency};
pub use lifecycle::{ModuleLifecycle, ModuleHost as LifecycleModuleHost, ModuleHandle};
pub use loader::{ModuleLoader, ModuleLoaderConfig};
pub use permissions::{ModulePermissionChecker, PermissionPolicy, PolicyDecision};

/// Module host errors
#[derive(Debug, Error)]
pub enum ModuleHostError {
    #[error("Module already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Module not found: {0}")]
    NotFound(String),
    
    #[error("Dependency not found: {0}")]
    DependencyNotFound(String),
    
    #[error("Dependency cycle detected: {0}")]
    DependencyCycle(String),
    
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
    
    #[error("Load failed: {0}")]
    LoadFailed(String),
    
    #[error("Module is in invalid state: {0}")]
    InvalidState(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Module metadata for tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleMeta {
    pub id: String,
    pub manifest: crate::manifest::ModuleManifest,
    pub state: ModuleState,
    pub loaded_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub stopped_at: Option<chrono::DateTime<chrono::Utc>>,
    pub restart_count: u32,
    pub last_error: Option<String>,
}

/// Module runtime state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, PartialOrd, Ord)]
pub enum ModuleState {
    Discovered,
    Validating,
    Installed,
    Registered,
    Enabled,
    Disabled,
    Starting,
    Running,
    Blocked,
    Updating,
    Stopping,
    Stopped,
    Failed,
    Unloaded,
}

/// Module host configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleHostConfig {
    pub module_dirs: Vec<std::path::PathBuf>,
    pub auto_discover: bool,
    pub auto_enable: bool,
    pub max_modules: usize,
    pub load_timeout: std::time::Duration,
    pub start_timeout: std::time::Duration,
    pub stop_timeout: std::time::Duration,
    pub health_check_interval: std::time::Duration,
}

impl Default for ModuleHostConfig {
    fn default() -> Self {
        Self {
            module_dirs: vec![std::path::PathBuf::from("modules")],
            auto_discover: true,
            auto_enable: false,
            max_modules: 100,
            load_timeout: std::time::Duration::from_secs(30),
            start_timeout: std::time::Duration::from_secs(60),
            stop_timeout: std::time::Duration::from_secs(30),
            health_check_interval: std::time::Duration::from_secs(30),
        }
    }
}

/// Main module host
pub struct ModuleHost {
    config: ModuleHostConfig,
    modules: Arc<tokio::sync::RwLock<std::collections::HashMap<String, ModuleMeta>>>,
    registry: Arc<james_registry::Registry>,
    capability_registry: Arc<james_capabilities::CapabilityRegistry>,
    event_bus: Arc<james_events::EventBus>,
    module_loader: crate::loader::ModuleLoader,
    permission_checker: crate::permissions::ModulePermissionChecker,
    broker: Arc<tokio::sync::RwLock<Option<Arc<james_capability_broker::CapabilityBroker>>>>,
    running: Arc<tokio::sync::RwLock<bool>>,
}

impl ModuleHost {
    /// Create a new module host
    pub fn new(
        config: ModuleHostConfig,
        registry: Arc<james_registry::Registry>,
        capability_registry: Arc<james_capabilities::CapabilityRegistry>,
        event_bus: Arc<james_events::EventBus>,
    ) -> Self {
let module_loader = crate::loader::ModuleLoader::new(crate::loader::ModuleLoaderConfig {
            module_dirs: config.module_dirs.clone(),
            load_timeout: config.load_timeout,
            allow_unsigned: false,
        });
        
        let permission_checker = crate::permissions::ModulePermissionChecker::default();
        
Self {
            config,
            modules: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            registry,
            capability_registry,
            event_bus,
            module_loader,
            permission_checker,
            broker: Arc::new(tokio::sync::RwLock::new(None)),
            running: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }

    /// Attach a capability broker so permission checks are enforced through
    /// the full §6/§7 chain.
    pub async fn set_broker(&self, broker: Arc<james_capability_broker::CapabilityBroker>) {
        {
            let mut guard = self.broker.write().await;
            *guard = Some(broker.clone());
        }
        self.permission_checker.set_broker(broker).await;
    }

    /// Access the attached capability broker, if any.
    pub async fn broker(&self) -> Option<Arc<james_capability_broker::CapabilityBroker>> {
        self.broker.read().await.clone()
    }
    
    /// Start the module host
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = true;
        
        if self.config.auto_discover {
            self.discover_modules().await?;
        }
        
        if self.config.auto_enable {
            self.enable_all_discovered().await?;
        }
        
        info!("Module host started with {} modules", self.modules.read().await.len());
        Ok(())
    }
    
    /// Stop the module host
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = false;
        
        // Stop all running modules
        let modules = self.modules.read().await;
        let module_ids: Vec<String> = modules.keys().cloned().collect();
        drop(modules);
        
        for id in module_ids {
            if let Err(e) = self.stop_module(&id).await {
                warn!("Failed to stop module {}: {}", id, e);
            }
        }
        
        Ok(())
    }
    
    /// Discover modules in configured directories
    pub async fn discover_modules(&self) -> Result<Vec<String>> {
        let modules = self.module_loader.discover().await?;
        let mut discovered = Vec::new();
        
        for manifest in modules {
            let id = manifest.id.clone();
            let meta = ModuleMeta {
                id: id.clone(),
                manifest,
                state: ModuleState::Discovered,
                loaded_at: Some(chrono::Utc::now()),
                started_at: None,
                stopped_at: None,
                restart_count: 0,
                last_error: None,
            };
            
            self.modules.write().await.insert(id.clone(), meta);
            discovered.push(id);
        }
        
        Ok(discovered)
    }
    
/// Validate a module manifest
    pub async fn validate_module(&self, manifest: &crate::manifest::ModuleManifest) -> Result<()> {
        crate::manifest::ModuleManifestValidator::validate(manifest)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
    
    /// Install a module from a manifest
    pub async fn install_module(&self, manifest: crate::manifest::ModuleManifest) -> Result<String> {
        self.validate_module(&manifest).await?;
        
        let id = manifest.id.clone();
        let mut modules = self.modules.write().await;
        
if modules.contains_key(&manifest.id) {
            return Err(crate::ModuleHostError::AlreadyExists(manifest.id).into());
        }
        
// Check dependencies
        for dep in &manifest.dependencies {
            if !self.modules.read().await.contains_key(&dep.name) {
                return Err(crate::ModuleHostError::DependencyNotFound(dep.name.clone()).into());
            }
        }
        
        // Check for dependency cycles
        self.check_dependency_cycles(&manifest).await?;
        
// Register capabilities
        self.register_capabilities(&manifest).await?;
        
        // Register in registry
        self.register_in_registry(&manifest).await?;
        
        let module_id = manifest.id.clone();
        let meta = ModuleMeta {
            id: module_id.clone(),
            manifest,
            state: ModuleState::Installed,
            loaded_at: Some(chrono::Utc::now()),
            started_at: None,
            stopped_at: None,
            restart_count: 0,
            last_error: None,
        };
        
        modules.insert(module_id, meta);
        
        Ok(id)
    }
    
/// Enable a module
    pub async fn enable_module(&self, id: &str) -> Result<()> {
        let manifest = {
            let mut modules = self.modules.write().await;
            let meta = modules.get_mut(id)
                .ok_or_else(|| crate::ModuleHostError::NotFound(id.to_string()))?;
            
            if matches!(meta.state, ModuleState::Enabled | ModuleState::Starting | ModuleState::Running) {
                return Ok(()); // Already enabled
            }
            
            meta.state = ModuleState::Enabled;
            meta.manifest.clone()
        };
        
        // Mirror grants into the capability broker (enforced chain §6/§7).
        if let Some(broker) = self.broker().await {
            for cap_id in &manifest.capabilities {
                broker.grant_capability_permissions(id, cap_id);
            }
        }
        
        Ok(())
    }
    
/// Disable a module
    pub async fn disable_module(&self, id: &str) -> Result<()> {
        let mut modules = self.modules.write().await;
        let mut meta = modules.get_mut(id)
            .ok_or_else(|| crate::ModuleHostError::NotFound(id.to_string()))?;
        
        if !matches!(meta.state, ModuleState::Enabled | ModuleState::Starting | ModuleState::Running) {
            return Ok(()); // Already disabled
        }
        
        meta.state = ModuleState::Stopped;
        Ok(())
    }
    
/// Start a module
    pub async fn start_module(&self, id: &str) -> Result<()> {
        let manifest = {
            let mut modules = self.modules.write().await;
            let meta = modules.get_mut(id)
                .ok_or_else(|| crate::ModuleHostError::NotFound(id.to_string()))?;
            
            if !matches!(meta.state, ModuleState::Enabled | ModuleState::Stopped) {
                return Err(crate::ModuleHostError::InvalidManifest(
                    format!("Cannot start module in state {:?}", meta.state)
                ).into());
            }
            
            meta.state = ModuleState::Starting;
            meta.manifest.clone()
        };
        
        // Load and start the module
        self.module_loader.load_and_start(&manifest).await?;
        
        let mut modules = self.modules.write().await;
        if let Some(meta) = modules.get_mut(id) {
            meta.state = ModuleState::Running;
            meta.started_at = Some(chrono::Utc::now());
        }
        
// Emit module started event
        let event = james_events::create_system_event(
            james_events::builtin_events::MODULE_LOADED,
            "module-host"
        ).with_payload(serde_json::json!({
            "module_id": id,
            "name": manifest.name,
        }));
        let _ = self.event_bus.publish_envelope(james_events::EventEnvelope::new(event)).await;
        
        Ok(())
    }
    
/// Stop a module
    pub async fn stop_module(&self, id: &str) -> Result<()> {
        let manifest = {
            let mut modules = self.modules.write().await;
            let meta = modules.get_mut(id)
                .ok_or_else(|| crate::ModuleHostError::NotFound(id.to_string()))?;
            
            if !matches!(meta.state, ModuleState::Running | ModuleState::Starting) {
                return Ok(()); // Already stopped
            }
            
            meta.state = ModuleState::Stopping;
            meta.manifest.clone()
        };
        
        // Stop the module
        self.module_loader.stop_by_manifest(&manifest).await?;
        
        let mut modules = self.modules.write().await;
        if let Some(meta) = modules.get_mut(id) {
            meta.state = ModuleState::Stopped;
            meta.stopped_at = Some(chrono::Utc::now());
        }
        
        // Emit module unloaded event
        let event = james_events::create_system_event(
            james_events::builtin_events::MODULE_UNLOADED,
            "module-host"
        ).with_payload(serde_json::json!({
            "module_id": id,
        }));
        let _ = self.event_bus.publish_envelope(james_events::EventEnvelope::new(event)).await;
        
        Ok(())
    }
    
    /// Restart a module
    pub async fn restart_module(&self, id: &str) -> Result<()> {
        self.stop_module(id).await?;
        self.start_module(id).await
    }
    
    /// Uninstall a module
    pub async fn uninstall_module(&self, id: &str) -> Result<()> {
        // Stop if running
        self.stop_module(id).await?;
        
        // Unregister capabilities
        let meta = {
            let modules = self.modules.read().await;
            modules.get(id).cloned()
        };
        
        if let Some(meta) = meta {
            for cap_id in &meta.manifest.capabilities {
                let _ = self.capability_registry.unregister(cap_id).await;
            }
        }
        
// Unregister from registry
        let entries = self.registry.list_all();
        for entry in entries {
            if entry.name == id {
                let _ = self.registry.unregister(entry.id).await;
                break;
            }
        }
        
        self.modules.write().await.remove(id);
        Ok(())
    }
    
    /// Get module metadata
    pub async fn get_module(&self, id: &str) -> Option<ModuleMeta> {
        self.modules.read().await.get(id).cloned()
    }
    
    /// List all modules
    pub async fn list_modules(&self) -> Vec<ModuleMeta> {
        self.modules.read().await.values().cloned().collect()
    }
    
    /// Get module state
    pub async fn get_module_state(&self, id: &str) -> Option<ModuleState> {
        self.modules.read().await.get(id).map(|m| m.state)
    }
    
    /// Enable all discovered modules
    async fn enable_all_discovered(&self) -> Result<()> {
        let modules = self.modules.read().await;
        let ids: Vec<String> = modules.iter()
            .filter(|(_, m)| matches!(m.state, ModuleState::Discovered | ModuleState::Installed))
            .map(|(id, _)| id.clone())
            .collect();
        
        drop(modules);
        
        for id in ids {
            self.enable_module(&id).await?;
        }
        Ok(())
    }
    
    /// Register capabilities from manifest
    async fn register_capabilities(&self, manifest: &crate::manifest::ModuleManifest) -> Result<()> {
        for cap_id in &manifest.capabilities {
            if self.capability_registry.get(cap_id).is_none() {
                let definition = james_capabilities::CapabilityDefinition {
                    id: cap_id.clone(),
                    name: cap_id.clone(),
                    category: james_capabilities::CapabilityCategory::Custom("module".to_string()),
                    version: "1.0.0".to_string(),
                    provider: manifest.id.clone(),
                    description: format!("Capability provided by module {}", manifest.name),
                    risk_level: james_capabilities::RiskLevel::Low,
                    required_permissions: vec![],
                    dependencies: vec![],
                    input_schema: None,
                    output_schema: None,
                    execution_target: james_capabilities::ExecutionTarget::Local,
                    tags: vec![],
                    deprecated: false,
                    experimental: false,
                };
                self.capability_registry.register(definition, manifest.id.clone()).await?;
            }
        }
        Ok(())
    }
    
    /// Register module in registry
    async fn register_in_registry(&self, manifest: &crate::manifest::ModuleManifest) -> Result<()> {
        use james_registry::{RegistryEntry, RegistryEntryType, RegistryStatus};
        use chrono::Utc;
        use uuid::Uuid;
        
let entry = RegistryEntry {
            id: Uuid::now_v7(),
            entry_type: RegistryEntryType::Module,
            name: manifest.id.clone(),
            version: manifest.version.clone(),
            provider: manifest.id.clone(),
            status: RegistryStatus::Active,
            capabilities: manifest.capabilities.clone(),
            dependencies: manifest.dependencies.iter().map(|_| Uuid::now_v7()).collect(),
            metadata: serde_json::to_value(manifest)?,
            registered_at: Utc::now(),
            last_seen: Utc::now(),
            heartbeat_interval_secs: Some(30),
        };
        
        self.registry.register(entry).await?;
        Ok(())
    }
    
    /// Check for dependency cycles
async fn check_dependency_cycles(&self, manifest: &crate::manifest::ModuleManifest) -> Result<()> {
        // Simple cycle detection: check if any dependency transitively depends on this module
        for dep in &manifest.dependencies {
            if self.would_create_cycle(&manifest.id, &dep.name).await? {
                return Err(ModuleHostError::DependencyCycle(
                    format!("Adding module {} would create a dependency cycle", manifest.id)
                ).into());
            }
        }
        Ok(())
    }
    
    async fn would_create_cycle(&self, module_id: &str, dependency_id: &str) -> Result<bool> {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![dependency_id.to_string()];
        
        while let Some(current) = stack.pop() {
            if current == module_id {
                return Ok(true);
            }
            
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            
            if let Some(meta) = self.modules.read().await.get(&current) {
                for dep in &meta.manifest.dependencies {
                    if !visited.contains(&dep.name) {
                        stack.push(dep.name.clone());
                    }
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_module_host_basic() {
        let event_bus = Arc::new(james_events::EventBus::new(100));
        event_bus.start().await.unwrap();
        
        let registry = Arc::new(james_registry::Registry::new());
        let capability_registry = Arc::new(james_capabilities::CapabilityRegistry::new());
        
        let host = ModuleHost::new(
            ModuleHostConfig::default(),
            registry,
            capability_registry,
            event_bus,
        );
        
        host.start().await.unwrap();
        assert!(host.modules.read().await.is_empty());
        
        host.stop().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_module_install_uninstall() {
        let event_bus = Arc::new(james_events::EventBus::new(100));
        event_bus.start().await.unwrap();
        
        let host = ModuleHost::new(
            ModuleHostConfig::default(),
            Arc::new(james_registry::Registry::new()),
            Arc::new(james_capabilities::CapabilityRegistry::new()),
            event_bus,
        );
        
        host.start().await.unwrap();
        
let manifest = crate::manifest::ModuleManifest {
            id: "com.example.test-module".to_string(),
            name: "Test Module".to_string(),
            version: "1.0.0".to_string(),
            description: "A test module".to_string(),
            module_type: crate::manifest::ModuleType::Service,
            entry_point: "lib.so".to_string(),
            capabilities: vec!["test.capability".to_string()],
            dependencies: vec![],
            permissions: vec![],
            configuration_schema: None,
            default_config: None,
            author: None,
            homepage: None,
            repository: None,
            license: "MIT".to_string(),
            tags: vec![],
            min_core_version: "0.1.0".to_string(),
            platforms: vec!["windows".to_string()],
        };
        
        let id = host.install_module(manifest).await.unwrap();
        assert_eq!(id, "com.example.test-module");
        
        // Verify module is installed
        let meta = host.get_module("com.example.test-module").await.unwrap();
        assert_eq!(meta.state, ModuleState::Installed);
        
        // Uninstall
        host.uninstall_module("com.example.test-module").await.unwrap();
        assert!(host.get_module("com.example.test-module").await.is_none());
    }
}


