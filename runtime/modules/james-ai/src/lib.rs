//! James-AI - AI inference capability provider for JAMES
//!
//! Provides the ai.inference capability with provider abstraction.

use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::EventBus;
use james_module_host::{ModuleManifest, ModuleType};
use james_modelrouter::{QualityTier, RoutingDecision, RoutingRequest};
use james_models::{ModelCapability, ModelInfo, ModelType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

/// Inference request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub model_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub response_format: Option<ResponseFormat>,
    pub tools: Option<Vec<Tool>>,
}

/// Chat message for inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

/// Message role
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Response format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    pub format_type: ResponseFormatType,
    pub schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResponseFormatType {
    Text,
    JsonObject,
    JsonSchema,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub tool_type: ToolType,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolType {
    Function,
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub tool_type: ToolType,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Inference response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
    pub created: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// AI provider trait
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse>;
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
    fn provider_name(&self) -> &str;
}

/// AI module state
pub struct AiModule {
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    providers: Arc<RwLock<Vec<Arc<dyn AiProvider>>>>,
    router: Option<Arc<james_modelrouter::ModelRouterModule>>,
}

impl AiModule {
    pub fn new(
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
    ) -> Self {
        Self {
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            providers: Arc::new(RwLock::new(Vec::new())),
            router: None,
        }
    }

    pub fn with_router(mut self, router: Arc<james_modelrouter::ModelRouterModule>) -> Self {
        self.router = Some(router);
        self
    }

    pub async fn register_provider(&self, provider: Arc<dyn AiProvider>) {
        self.providers.write().await.push(provider);
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-AI started with {} providers", self.providers.read().await.len());
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-AI stopped");
        Ok(())
    }

    /// Run inference with optional routing
    pub async fn infer(&self, mut request: InferenceRequest) -> Result<InferenceResponse> {
        // If no model specified and router available, route
        if request.model_id.is_none() {
            if let Some(router) = &self.router {
                let routing_request = RoutingRequest {
                    task: "chat".to_string(),
                    required_capabilities: vec![ModelCapability::Chat],
                    preferred_model_type: Some(ModelType::LLM),
                    max_latency_ms: None,
                    max_cost_per_1k: None,
                    require_local: false,
                    require_streaming: request.stream,
                    context_length_needed: None,
                    quality_tier: QualityTier::Balanced,
                };
                let decision = router.route(routing_request).await?;
                request.model_id = Some(decision.model_id);
            }
        }

        let model_id = request.model_id.clone();

        let providers = self.providers.read().await;

        // Resolve provider: a single provider without an explicit model gets
        // deferred to its own inference defaults; otherwise select by model hint
        let (provider, resolved_model): (Arc<dyn AiProvider>, Option<String>) =
            if model_id.is_none() && providers.len() == 1 {
                let p = providers[0].clone();
                (p, None)
            } else {
                let mid = model_id.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("No model specified and no router available"))?;
                let p = providers.iter()
                    .find(|p| p.provider_name() == "local"
                        || p.provider_name() == "ollama"
                        || p.provider_name().contains(mid))
                    .ok_or_else(|| anyhow::anyhow!("No provider for model: {}", mid))?;
                (p.clone(), Some(mid.clone()))
            };

        // Defer to the provider's default model unless one was explicitly set
        if request.model_id.is_none() {
            request.model_id = resolved_model;
        }
        provider.infer(request).await
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let mut all = Vec::new();
        for provider in self.providers.read().await.iter() {
            all.extend(provider.list_models().await?);
        }
        Ok(all)
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

/// Module manifest
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.ai".to_string(),
        name: "James-AI".to_string(),
        version: "0.1.0".to_string(),
        description: "AI inference capability provider for JAMES".to_string(),
        module_type: ModuleType::AiProvider,
        entry_point: "james_ai".to_string(),
        capabilities: vec!["ai.inference".to_string(), "ai.reasoning".to_string(), "ai.coding".to_string()],
        dependencies: vec![
            james_module_host::ModuleDependency {
                name: "james.modelrouter".to_string(),
                version: "0.1.0".to_string(),
                optional: true,
                reason: Some("Optional for intelligent routing".to_string()),
            },
        ],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "default_model": {"type": "string"},
                "default_temperature": {"type": "number", "default": 0.7},
                "default_max_tokens": {"type": "integer", "default": 4096},
                "enable_routing": {"type": "boolean", "default": true}
            }
        })),
        default_config: Some(serde_json::json!({
            "default_temperature": 0.7,
            "default_max_tokens": 4096,
            "enable_routing": true
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["ai".to_string(), "inference".to_string(), "llm".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Capabilities registration
pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    let caps = vec![
        ("ai.inference", "AI Inference", "Run AI model inference"),
        ("ai.reasoning", "AI Reasoning", "Complex reasoning tasks"),
        ("ai.coding", "AI Coding", "Code generation and analysis"),
    ];

    for (id, name, desc) in caps {
        let cap = CapabilityDefinition {
            id: id.to_string(),
            name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("ai".to_string()),
            version: "1.0.0".to_string(),
            provider: "james.ai".to_string(),
            description: desc.to_string(),
            risk_level: RiskLevel::Low,
            required_permissions: vec![],
            dependencies: vec![],
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "model_id": {"type": "string"},
                    "messages": {"type": "array", "items": {"type": "object"}},
                    "temperature": {"type": "number"},
                    "max_tokens": {"type": "integer"},
                    "stream": {"type": "boolean"}
                },
                "required": ["messages"]
            })),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "model": {"type": "string"},
                    "choices": {"type": "array", "items": {"type": "object"}},
                    "usage": {"type": "object"}
                }
            })),
            execution_target: ExecutionTarget::Local,
            tags: vec!["ai".to_string(), "inference".to_string()],
            deprecated: false,
            experimental: false,
        };
        registry.register(cap, "james.ai".to_string()).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;
    use james_module_host::ModuleManifestValidator;

    #[tokio::test]
    async fn test_ai_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.ai");
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }

    #[tokio::test]
    async fn test_ai_module() {
        let event_bus = Arc::new(EventBus::new(100));
        event_bus.start().await.unwrap();
        let cap_reg = Arc::new(CapabilityRegistry::new());

        let module = AiModule::new(event_bus, cap_reg);
        assert!(!module.is_running().await);

        module.start().await.unwrap();
        assert!(module.is_running().await);

        module.stop().await.unwrap();
        assert!(!module.is_running().await);
    }
}