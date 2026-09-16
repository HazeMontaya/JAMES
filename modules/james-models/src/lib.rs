//! James-Models - Model management and metadata for JAMES
//!
//! Provides model metadata storage, discovery, and registry.

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleManifestValidator, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info};
use uuid::Uuid;

/// Model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_type: ModelType,
    pub capabilities: Vec<ModelCapability>,
    pub context_length: usize,
    pub parameters: Option<String>,
    pub quantization: Option<String>,
    pub size_bytes: Option<u64>,
    pub path: Option<String>,
    pub endpoint: Option<String>,
    pub api_key_required: bool,
    pub cost_per_1k_input: Option<f64>,
    pub cost_per_1k_output: Option<f64>,
    pub metadata: serde_json::Value,
}

/// Model type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelType {
    LLM,
    Embedding,
    ImageGeneration,
    SpeechToText,
    TextToSpeech,
    Vision,
    Custom,
}

/// Model capability
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelCapability {
    Chat,
    Completion,
    Reasoning,
    Coding,
    FunctionCalling,
    Vision,
    Streaming,
    JsonMode,
}

/// Models module state
pub struct ModelsModule {
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    models: Arc<RwLock<Vec<ModelInfo>>>,
}

impl ModelsModule {
    pub fn new(
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
    ) -> Self {
        Self {
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            models: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-Models started");
        self.discover_local_models().await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-Models stopped");
        Ok(())
    }

    async fn discover_local_models(&self) -> Result<()> {
        // Scan for local models (ollama, llama.cpp, etc.)
        // This is a placeholder - real implementation would scan directories
        Ok(())
    }

    pub async fn register_model(&self, model: ModelInfo) -> Result<()> {
        self.models.write().await.push(model);
        Ok(())
    }

    pub async fn get_model(&self, id: &str) -> Option<ModelInfo> {
        self.models.read().await.iter().find(|m| m.id == id).cloned()
    }

    pub async fn list_models(&self, model_type: Option<ModelType>) -> Vec<ModelInfo> {
        let models = self.models.read().await;
        if let Some(mt) = model_type {
            models.iter().filter(|m| m.model_type == mt).cloned().collect()
        } else {
            models.clone()
        }
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

/// Module manifest
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.models".to_string(),
        name: "James-Models".to_string(),
        version: "0.1.0".to_string(),
        description: "Model management and metadata for JAMES".to_string(),
        module_type: ModuleType::Service,
        entry_point: "james_models".to_string(),
        capabilities: vec!["models.registry".to_string(), "models.discovery".to_string()],
        dependencies: vec![],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "scan_directories": {"type": "array", "items": {"type": "string"}},
                "ollama_endpoint": {"type": "string", "default": "http://localhost:11434"},
                "auto_discover": {"type": "boolean", "default": true}
            }
        })),
        default_config: Some(serde_json::json!({
            "scan_directories": [],
            "ollama_endpoint": "http://localhost:11434",
            "auto_discover": true
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["models".to_string(), "ai".to_string(), "registry".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Capabilities registration
pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    let cap1 = CapabilityDefinition {
        id: "models.registry".to_string(),
        name: "Models Registry".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("models".to_string()),
        version: "1.0.0".to_string(),
        provider: "james.models".to_string(),
        description: "Model metadata registry and lookup".to_string(),
        risk_level: RiskLevel::Low,
        required_permissions: vec![],
        dependencies: vec![],
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "model_id": {"type": "string"},
                "model_type": {"type": "string"}
            }
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "models": {"type": "array", "items": {"type": "object"}}
            }
        })),
        execution_target: ExecutionTarget::Local,
        tags: vec!["models".to_string(), "registry".to_string()],
        deprecated: false,
        experimental: false,
    };
    registry.register(cap1, "james.models".to_string()).await?;

    let cap2 = CapabilityDefinition {
        id: "models.discovery".to_string(),
        name: "Models Discovery".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("models".to_string()),
        version: "1.0.0".to_string(),
        provider: "james.models".to_string(),
        description: "Discover available models from local and remote sources".to_string(),
        risk_level: RiskLevel::Low,
        required_permissions: vec![],
        dependencies: vec![],
        input_schema: None,
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "discovered": {"type": "array", "items": {"type": "object"}}
            }
        })),
        execution_target: ExecutionTarget::Local,
        tags: vec!["models".to_string(), "discovery".to_string()],
        deprecated: false,
        experimental: false,
    };
    registry.register(cap2, "james.models".to_string()).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;

    #[tokio::test]
    async fn test_models_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.models");
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }

    #[tokio::test]
    async fn test_models_module() {
        let event_bus = Arc::new(EventBus::new(100));
        event_bus.start().await.unwrap();
        let cap_reg = Arc::new(CapabilityRegistry::new());

        let module = ModelsModule::new(event_bus, cap_reg);
        assert!(!module.is_running().await);

        module.start().await.unwrap();
        assert!(module.is_running().await);

        module.stop().await.unwrap();
        assert!(!module.is_running().await);
    }
}