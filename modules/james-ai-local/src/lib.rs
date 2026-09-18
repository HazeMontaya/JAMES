//! James-AI-Local - Local AI inference provider for JAMES
//!
//! Supports Ollama, llama.cpp, and other local inference backends.

use anyhow::Result;
use async_trait::async_trait;
use james_ai::{AiProvider, ChatMessage, FunctionCall, InferenceRequest, InferenceResponse, MessageRole, ToolCall, ToolType, Usage};
use james_models::{ModelCapability, ModelInfo, ModelType};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ollama API structures
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: Option<OllamaOptions>,
    format: Option<String>,
    tools: Option<Vec<OllamaTool>>,
}

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: Option<f32>,
    num_predict: Option<i32>,
}

#[derive(Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunction,
}

#[derive(Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: OllamaMessage,
    done: bool,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

/// Local AI provider
pub struct LocalAiProvider {
    client: Client,
    endpoint: String,
}

impl LocalAiProvider {
    pub fn new(endpoint: Option<String>) -> Self {
        let endpoint = endpoint.unwrap_or_else(|| "http://localhost:11434".to_string());
        Self {
            client: Client::new(),
            endpoint,
        }
    }

    async fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.endpoint);
        let resp = self.client.get(&url).send().await?;
        let json: serde_json::Value = resp.json().await?;

        let mut models = Vec::new();
        if let Some(models_array) = json.get("models").and_then(|v| v.as_array()) {
            for m in models_array {
                let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let size = m.get("size").and_then(|v| v.as_u64());
                let modified = m.get("modified_at").and_then(|v| v.as_str());

                models.push(ModelInfo {
                    id: format!("ollama:{}", name),
                    name: name.to_string(),
                    provider: "ollama".to_string(),
                    model_type: ModelType::LLM,
                    capabilities: vec![ModelCapability::Chat, ModelCapability::Completion, ModelCapability::Streaming],
                    context_length: 4096, // Default, would need model-specific
                    parameters: None,
                    quantization: None,
                    size_bytes: size,
                    path: None,
                    endpoint: Some(self.endpoint.clone()),
                    api_key_required: false,
                    cost_per_1k_input: None,
                    cost_per_1k_output: None,
                    metadata: serde_json::json!({
                        "source": "ollama",
                        "modified_at": modified
                    }),
                });
            }
        }
        Ok(models)
    }
}

#[async_trait]
impl AiProvider for LocalAiProvider {
    async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
        let model_id = request.model_id.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Model ID required for local inference"))?;

        // Strip provider prefix if present
        let model_name = model_id.strip_prefix("ollama:").unwrap_or(model_id);

        let messages: Vec<OllamaMessage> = request.messages.into_iter().map(|m| OllamaMessage {
            role: match m.role {
                MessageRole::System => "system".to_string(),
                MessageRole::User => "user".to_string(),
                MessageRole::Assistant => "assistant".to_string(),
                MessageRole::Tool => "tool".to_string(),
            },
            content: m.content,
            name: m.name,
            tool_calls: m.tool_calls.map(|calls| calls.into_iter().map(|tc| OllamaToolCall {
                function: OllamaFunctionCall {
                    name: tc.function.name,
                    arguments: serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null),
                },
            }).collect()),
            tool_call_id: m.tool_call_id,
        }).collect();

        let tools = request.tools.map(|tools| tools.into_iter().map(|t| OllamaTool {
            tool_type: "function".to_string(),
            function: OllamaFunction {
                name: t.function.name,
                description: t.function.description,
                parameters: t.function.parameters,
            },
        }).collect());

        let ollama_req = OllamaChatRequest {
            model: model_name.to_string(),
            messages,
            stream: false,
            options: Some(OllamaOptions {
                temperature: request.temperature,
                num_predict: request.max_tokens.map(|t| t as i32),
            }),
            format: request.response_format.map(|rf| match rf.format_type {
                james_ai::ResponseFormatType::JsonObject => "json".to_string(),
                james_ai::ResponseFormatType::JsonSchema => "json".to_string(),
                _ => "".to_string(),
            }),
            tools,
        };

        let url = format!("{}/api/chat", self.endpoint);
        let resp = self.client.post(&url)
            .json(&ollama_req)
            .send()
            .await?;

        let ollama_resp: OllamaChatResponse = resp.json().await?;

        // Convert back to our types
        let message = ChatMessage {
            role: match ollama_resp.message.role.as_str() {
                "system" => MessageRole::System,
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "tool" => MessageRole::Tool,
                _ => MessageRole::Assistant,
            },
            content: ollama_resp.message.content,
            name: ollama_resp.message.name,
            tool_calls: ollama_resp.message.tool_calls.map(|calls| calls.into_iter().map(|tc| ToolCall {
                id: Uuid::now_v7().to_string(),
                tool_type: ToolType::Function,
                function: FunctionCall {
                    name: tc.function.name,
                    arguments: serde_json::to_string(&tc.function.arguments).unwrap_or_default(),
                },
            }).collect()),
            tool_call_id: ollama_resp.message.tool_call_id,
        };

        let choice = james_ai::Choice {
            index: 0,
            message,
            finish_reason: if ollama_resp.done { Some("stop".to_string()) } else { None },
        };

        let usage = Usage {
            prompt_tokens: ollama_resp.prompt_eval_count.unwrap_or(0),
            completion_tokens: ollama_resp.eval_count.unwrap_or(0),
            total_tokens: ollama_resp.prompt_eval_count.unwrap_or(0) + ollama_resp.eval_count.unwrap_or(0),
        };

        Ok(InferenceResponse {
            id: Uuid::now_v7().to_string(),
            model: ollama_resp.model,
            choices: vec![choice],
            usage,
            created: chrono::Utc::now().timestamp() as u64,
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        self.fetch_models().await
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }
}

/// Module manifest
pub fn manifest() -> james_module_host::ModuleManifest {
    james_module_host::ModuleManifest {
        id: "james.ai.local".to_string(),
        name: "James-AI-Local".to_string(),
        version: "0.1.0".to_string(),
        description: "Local AI inference provider (Ollama/llama.cpp) for JAMES".to_string(),
        module_type: james_module_host::ModuleType::AiProvider,
        entry_point: "james_ai_local".to_string(),
        capabilities: vec!["ai.inference".to_string(), "ai.reasoning".to_string(), "ai.coding".to_string()],
        dependencies: vec![
            james_module_host::ModuleDependency {
                name: "james.ai".to_string(),
                version: "0.1.0".to_string(),
                optional: false,
                reason: Some("Core AI capability".to_string()),
            },
        ],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "endpoint": {"type": "string", "default": "http://localhost:11434"},
                "auto_pull": {"type": "boolean", "default": false},
                "timeout_seconds": {"type": "integer", "default": 300}
            }
        })),
        default_config: Some(serde_json::json!({
            "endpoint": "http://localhost:11434",
            "auto_pull": false,
            "timeout_seconds": 300
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["ai".to_string(), "local".to_string(), "ollama".to_string(), "inference".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Capabilities registration
pub async fn register_capabilities(registry: &james_capabilities::CapabilityRegistry) -> Result<()> {
    let caps = vec![
        ("ai.inference", "AI Inference (Local)", "Local AI model inference via Ollama"),
        ("ai.reasoning", "AI Reasoning (Local)", "Local complex reasoning"),
        ("ai.coding", "AI Coding (Local)", "Local code generation"),
    ];

    for (id, name, desc) in caps {
        let cap = james_capabilities::CapabilityDefinition {
            id: id.to_string(),
            name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("ai".to_string()),
            version: "1.0.0".to_string(),
            provider: "james.ai.local".to_string(),
            description: desc.to_string(),
            risk_level: james_capabilities::RiskLevel::Low,
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
            execution_target: james_capabilities::ExecutionTarget::Local,
            tags: vec!["ai".to_string(), "local".to_string(), "ollama".to_string()],
            deprecated: false,
            experimental: false,
        };
        registry.register(cap, "james.ai.local".to_string()).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;
    use james_capabilities::CapabilityRegistry;

    #[tokio::test]
    async fn test_local_ai_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.ai.local");
        assert!(james_module_host::ModuleManifestValidator::validate(&manifest).is_ok());
    }
}