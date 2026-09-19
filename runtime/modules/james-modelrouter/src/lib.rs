//! James-ModelRouter - Model routing and selection for JAMES
//!
//! Provides intelligent model selection based on task, cost, latency, privacy.

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::EventBus;
use james_module_host::{ModuleManifest, ModuleType};
use james_models::{ModelCapability, ModelInfo, ModelType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

/// Routing request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRequest {
    pub task: String,
    pub required_capabilities: Vec<ModelCapability>,
    pub preferred_model_type: Option<ModelType>,
    pub max_latency_ms: Option<u64>,
    pub max_cost_per_1k: Option<f64>,
    pub require_local: bool,
    pub require_streaming: bool,
    pub context_length_needed: Option<usize>,
    pub quality_tier: QualityTier,
}

/// Quality tier preference
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum QualityTier {
    Fast,
    Balanced,
    Best,
}

impl std::fmt::Display for QualityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QualityTier::Fast => write!(f, "Fast"),
            QualityTier::Balanced => write!(f, "Balanced"),
            QualityTier::Best => write!(f, "Best"),
        }
    }
}

/// Routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub model_id: String,
    pub model_name: String,
    pub provider: String,
    pub reason: String,
    pub estimated_latency_ms: Option<u64>,
    pub estimated_cost_per_1k: Option<f64>,
    pub fallback_models: Vec<String>,
}

/// Router configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub default_quality: QualityTier,
    pub prefer_local: bool,
    pub cost_weight: f64,
    pub latency_weight: f64,
    pub quality_weight: f64,
    pub fallback_enabled: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            default_quality: QualityTier::Balanced,
            prefer_local: true,
            cost_weight: 0.3,
            latency_weight: 0.3,
            quality_weight: 0.4,
            fallback_enabled: true,
        }
    }
}

/// Model router module state
pub struct ModelRouterModule {
    config: RouterConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    models_module: Arc<james_models::ModelsModule>, // Will be injected
}

impl ModelRouterModule {
    pub fn new(
        config: RouterConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        models_module: Arc<james_models::ModelsModule>,
    ) -> Self {
        Self {
            config,
            event_bus: event_bus.clone(),
            capability_registry: capability_registry.clone(),
            running: Arc::new(RwLock::new(false)),
            models_module,
        }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-ModelRouter started");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-ModelRouter stopped");
        Ok(())
    }

    /// Replace the router's model registry with a freshly discovered set.
    pub async fn sync_models(&self, models: Vec<ModelInfo>) -> Result<()> {
        self.models_module.replace_models(models).await
    }

    /// Route a request to the best model
    pub async fn route(&self, request: RoutingRequest) -> Result<RoutingDecision> {
        let models = self.models_module.list_models(request.preferred_model_type).await;

        if models.is_empty() {
            return Err(anyhow::anyhow!("No models available"));
        }

        // Filter by requirements
        let mut candidates: Vec<ModelInfo> = models.into_iter()
            .filter(|m| {
                // Check required capabilities
                if !request.required_capabilities.is_empty() {
                    for cap in &request.required_capabilities {
                        if !m.capabilities.contains(cap) {
                            return false;
                        }
                    }
                }
                // Check local requirement
                if request.require_local && m.endpoint.is_some() {
                    return false;
                }
                // Check streaming
                if request.require_streaming && !m.capabilities.contains(&ModelCapability::Streaming) {
                    return false;
                }
                // Check context length
                if let Some(needed) = request.context_length_needed {
                    if m.context_length < needed {
                        return false;
                    }
                }
                // Check cost
                if let Some(max_cost) = request.max_cost_per_1k {
                    if let Some(cost) = m.cost_per_1k_input {
                        if cost > max_cost {
                            return false;
                        }
                    }
                }
                true
            })
            .collect();

        if candidates.is_empty() {
            return Err(anyhow::anyhow!("No models match requirements"));
        }

        // Score candidates
        candidates.sort_by(|a, b| {
            let score_a = self.score_model(a, &request);
            let score_b = self.score_model(b, &request);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        let best = candidates.first().unwrap().clone();
        let fallbacks: Vec<String> = candidates.iter().skip(1).take(3).map(|m| m.id.clone()).collect();

        Ok(RoutingDecision {
            model_id: best.id.clone(),
            model_name: best.name.clone(),
            provider: best.provider.clone(),
            reason: format!("Selected based on {}", request.quality_tier),
            estimated_latency_ms: None,
            estimated_cost_per_1k: best.cost_per_1k_input,
            fallback_models: fallbacks,
        })
    }

    fn score_model(&self, model: &ModelInfo, request: &RoutingRequest) -> f64 {
        let mut score = 0.0;

        // Cost score (lower is better)
        if let Some(cost) = model.cost_per_1k_input {
            score += (1.0 - (cost / 10.0).min(1.0)) * self.config.cost_weight;
        } else if model.endpoint.is_none() {
            score += 1.0 * self.config.cost_weight; // Local models are free
        }

        // Latency score (local preferred)
        if model.endpoint.is_none() {
            score += 1.0 * self.config.latency_weight;
        }

        // Quality score based on capabilities
        let quality_score = model.capabilities.len() as f64 / 8.0; // Max 8 capabilities
        score += quality_score * self.config.quality_weight;

        // Bonus for required capabilities
        for cap in &request.required_capabilities {
            if model.capabilities.contains(cap) {
                score += 0.1;
            }
        }

        score
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

/// Module manifest
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.modelrouter".to_string(),
        name: "James-ModelRouter".to_string(),
        version: "0.1.0".to_string(),
        description: "Intelligent model routing and selection for JAMES".to_string(),
        module_type: ModuleType::Service,
        entry_point: "james_modelrouter".to_string(),
        capabilities: vec!["ai.model_routing".to_string()],
        dependencies: vec![
            james_module_host::ModuleDependency {
                name: "james.models".to_string(),
                version: "0.1.0".to_string(),
                optional: false,
                reason: Some("Required for model registry".to_string()),
            },
        ],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "default_quality": {"type": "string", "enum": ["fast", "balanced", "best"], "default": "balanced"},
                "prefer_local": {"type": "boolean", "default": true},
                "cost_weight": {"type": "number", "default": 0.3},
                "latency_weight": {"type": "number", "default": 0.3},
                "quality_weight": {"type": "number", "default": 0.4},
                "fallback_enabled": {"type": "boolean", "default": true}
            }
        })),
        default_config: Some(serde_json::json!({
            "default_quality": "balanced",
            "prefer_local": true,
            "cost_weight": 0.3,
            "latency_weight": 0.3,
            "quality_weight": 0.4,
            "fallback_enabled": true
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["ai".to_string(), "routing".to_string(), "models".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Capabilities registration
pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    let cap = CapabilityDefinition {
        id: "ai.model_routing".to_string(),
        name: "AI Model Routing".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("ai".to_string()),
        version: "1.0.0".to_string(),
        provider: "james.modelrouter".to_string(),
        description: "Intelligent model selection based on task requirements".to_string(),
        risk_level: RiskLevel::Low,
        required_permissions: vec![],
        dependencies: vec!["models.registry".to_string()],
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "task": {"type": "string"},
                "required_capabilities": {"type": "array", "items": {"type": "string"}},
                "max_cost_per_1k": {"type": "number"},
                "require_local": {"type": "boolean"},
                "quality_tier": {"type": "string", "enum": ["fast", "balanced", "best"]}
            },
            "required": ["task"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "model_id": {"type": "string"},
                "model_name": {"type": "string"},
                "provider": {"type": "string"},
                "reason": {"type": "string"},
                "fallback_models": {"type": "array", "items": {"type": "string"}}
            }
        })),
        execution_target: ExecutionTarget::Local,
        tags: vec!["ai".to_string(), "routing".to_string()],
        deprecated: false,
        experimental: false,
    };
    registry.register(cap, "james.modelrouter".to_string()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;
    use james_module_host::ModuleManifestValidator;

    #[tokio::test]
    async fn test_router_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.modelrouter");
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }
}