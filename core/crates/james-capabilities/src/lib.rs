use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use schemars::{JsonSchema, schema_for};
use dashmap::DashMap;

use james_events::{EventEnvelope, builtin_events, create_system_event, EventBus};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Hash)]
pub enum CapabilityCategory {
    System,
    File,
    Process,
    Network,
    Browser,
    Voice,
    Ai,
    Device,
    SmartHome,
    Automation,
    Database,
    Security,
    Custom(String),
}

impl std::str::FromStr for CapabilityCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "system" => Ok(Self::System),
            "file" => Ok(Self::File),
            "process" => Ok(Self::Process),
            "network" => Ok(Self::Network),
            "browser" => Ok(Self::Browser),
            "voice" => Ok(Self::Voice),
            "ai" => Ok(Self::Ai),
            "device" => Ok(Self::Device),
            "smarthome" | "smart_home" | "smart-home" => Ok(Self::SmartHome),
            "automation" => Ok(Self::Automation),
            "database" => Ok(Self::Database),
            "security" => Ok(Self::Security),
            _ if lower.starts_with("custom:") => {
                Ok(Self::Custom(s["custom:".len()..].to_string()))
            }
            other => Err(format!("unknown capability category: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ExecutionTarget {
    Local,
    Remote(String),
    Container(String),
    Wasm(String),
    Native,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityDefinition {
    pub id: String,
    pub name: String,
    pub category: CapabilityCategory,
    pub version: String,
    pub provider: String,
    pub description: String,
    pub risk_level: RiskLevel,
    pub required_permissions: Vec<String>,
    pub dependencies: Vec<String>,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    pub execution_target: ExecutionTarget,
    pub tags: Vec<String>,
    pub deprecated: bool,
    pub experimental: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredCapability {
    pub definition: CapabilityDefinition,
    pub registered_at: DateTime<Utc>,
    pub registered_by: String,
    pub status: CapabilityStatus,
    pub usage_count: u64,
    pub last_used: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CapabilityStatus {
    Available,
    Unavailable,
    Deprecated,
    Experimental,
    Disabled,
}

pub struct CapabilityRegistry {
    capabilities: DashMap<String, RegisteredCapability>,
    by_category: DashMap<CapabilityCategory, Vec<String>>,
    by_provider: DashMap<String, Vec<String>>,
    event_bus: Option<Arc<EventBus>>,
    running: Arc<RwLock<bool>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            capabilities: DashMap::new(),
            by_category: DashMap::new(),
            by_provider: DashMap::new(),
            event_bus: None,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        self.register_builtin_capabilities().await?;
        info!("Capability Registry started with {} built-in capabilities", self.capabilities.len());
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("Capability Registry stopped");
        Ok(())
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    pub async fn register(&self, definition: CapabilityDefinition, registered_by: impl Into<String>) -> Result<()> {
        if self.capabilities.contains_key(&definition.id) {
            anyhow::bail!("Capability '{}' already registered", definition.id);
        }

        let capability = RegisteredCapability {
            definition: definition.clone(),
            registered_at: Utc::now(),
            registered_by: registered_by.into(),
            status: CapabilityStatus::Available,
            usage_count: 0,
            last_used: None,
        };

        self.by_category
            .entry(definition.category.clone())
            .or_default()
            .push(definition.id.clone());
        
        self.by_provider
            .entry(definition.provider.clone())
            .or_default()
            .push(definition.id.clone());

        self.capabilities.insert(definition.id.clone(), capability);

        if let Some(bus) = &self.event_bus {
            let event = create_system_event(builtin_events::CAPABILITY_REGISTERED, "capability-registry")
                .with_payload(serde_json::to_value(&definition)?);
            let _ = bus.publish_envelope(EventEnvelope::new(event)).await;
        }

        Ok(())
    }

    pub async fn unregister(&self, id: &str) -> Result<bool> {
        if let Some((_, cap)) = self.capabilities.remove(id) {
            if let Some(mut vec) = self.by_category.get_mut(&cap.definition.category) {
                vec.retain(|x| x != id);
            }
            
            if let Some(mut vec) = self.by_provider.get_mut(&cap.definition.provider) {
                vec.retain(|x| x != id);
            }

            if let Some(bus) = &self.event_bus {
                let event = create_system_event(builtin_events::CAPABILITY_REMOVED, "capability-registry")
                    .with_payload(serde_json::json!({ "capability_id": id }));
                let _ = bus.publish_envelope(EventEnvelope::new(event)).await;
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get(&self, id: &str) -> Option<RegisteredCapability> {
        self.capabilities.get(id).map(|c| c.clone())
    }

    pub fn get_definition(&self, id: &str) -> Option<CapabilityDefinition> {
        self.capabilities.get(id).map(|c| c.definition.clone())
    }

    pub fn list_all(&self) -> Vec<RegisteredCapability> {
        self.capabilities.iter().map(|c| c.clone()).collect()
    }

    pub fn list_by_category(&self, category: CapabilityCategory) -> Vec<RegisteredCapability> {
        self.by_category
            .get(&category)
            .map(|ids| ids.iter().filter_map(|id| self.capabilities.get(id).map(|c| c.clone())).collect())
            .unwrap_or_default()
    }

    pub fn list_by_provider(&self, provider: &str) -> Vec<RegisteredCapability> {
        self.by_provider
            .get(provider)
            .map(|ids| ids.iter().filter_map(|id| self.capabilities.get(id).map(|c| c.clone())).collect())
            .unwrap_or_default()
    }

    pub fn list_available(&self) -> Vec<RegisteredCapability> {
        self.capabilities
            .iter()
            .filter(|c| c.status == CapabilityStatus::Available)
            .map(|c| c.clone())
            .collect()
    }

    pub fn update_status(&self, id: &str, status: CapabilityStatus) -> Result<bool> {
        if let Some(mut cap) = self.capabilities.get_mut(id) {
            cap.status = status;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn record_usage(&self, id: &str) -> Result<bool> {
        if let Some(mut cap) = self.capabilities.get_mut(id) {
            cap.usage_count += 1;
            cap.last_used = Some(Utc::now());
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn count(&self) -> usize {
        self.capabilities.len()
    }

    pub fn get_input_schema(&self, id: &str) -> Option<serde_json::Value> {
        self.capabilities.get(id).and_then(|c| c.definition.input_schema.clone())
    }

    pub fn get_output_schema(&self, id: &str) -> Option<serde_json::Value> {
        self.capabilities.get(id).and_then(|c| c.definition.output_schema.clone())
    }

    pub fn validate_input(&self, id: &str, input: &serde_json::Value) -> Result<()> {
        if let Some(schema) = self.get_input_schema(id) {
            let validator = jsonschema::validator_for(&schema)?;
            let result = validator.validate(input);
            if let Err(errors) = result {
                let error_msgs: Vec<String> = errors.into_iter().map(|e| e.to_string()).collect();
                anyhow::bail!("Input validation failed for capability '{}': {}", id, error_msgs.join(", "));
            }
        }
        Ok(())
    }

    pub fn validate_output(&self, id: &str, output: &serde_json::Value) -> Result<()> {
        if let Some(schema) = self.get_output_schema(id) {
            let validator = jsonschema::validator_for(&schema)?;
            let result = validator.validate(output);
            if let Err(errors) = result {
                let error_msgs: Vec<String> = errors.into_iter().map(|e| e.to_string()).collect();
                anyhow::bail!("Output validation failed for capability '{}': {}", id, error_msgs.join(", "));
            }
        }
        Ok(())
    }

    async fn register_builtin_capabilities(&self) -> Result<()> {
        let builtins = vec![
            CapabilityDefinition {
                id: "system.files.read".to_string(),
                name: "Read Files".to_string(),
                category: CapabilityCategory::File,
                version: "1.0.0".to_string(),
                provider: "james-core".to_string(),
                description: "Read files from the local filesystem".to_string(),
                risk_level: RiskLevel::Low,
                required_permissions: vec!["filesystem.read".to_string()],
                dependencies: vec![],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "encoding": { "type": "string", "enum": ["utf-8", "base64", "binary"], "default": "utf-8" }
                    },
                    "required": ["path"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string" },
                        "size": { "type": "number" },
                        "modified_at": { "type": "string", "format": "date-time" }
                    },
                    "required": ["content"]
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["filesystem".to_string(), "read".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "system.files.write".to_string(),
                name: "Write Files".to_string(),
                category: CapabilityCategory::File,
                version: "1.0.0".to_string(),
                provider: "james-core".to_string(),
                description: "Write files to the local filesystem".to_string(),
                risk_level: RiskLevel::Medium,
                required_permissions: vec!["filesystem.write".to_string()],
                dependencies: vec![],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" },
                        "encoding": { "type": "string", "enum": ["utf-8", "base64", "binary"], "default": "utf-8" },
                        "create_dirs": { "type": "boolean", "default": true }
                    },
                    "required": ["path", "content"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "written": { "type": "boolean" },
                        "size": { "type": "number" }
                    },
                    "required": ["written"]
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["filesystem".to_string(), "write".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "system.process.start".to_string(),
                name: "Start Process".to_string(),
                category: CapabilityCategory::Process,
                version: "1.0.0".to_string(),
                provider: "james-core".to_string(),
                description: "Start a local process".to_string(),
                risk_level: RiskLevel::High,
                required_permissions: vec!["process.execute".to_string()],
                dependencies: vec![],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "args": { "type": "array", "items": { "type": "string" } },
                        "working_dir": { "type": "string" },
                        "env": { "type": "object", "additionalProperties": { "type": "string" } },
                        "timeout_secs": { "type": "number", "default": 60 }
                    },
                    "required": ["command"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "exit_code": { "type": "integer" },
                        "stdout": { "type": "string" },
                        "stderr": { "type": "string" },
                        "duration_ms": { "type": "number" }
                    },
                    "required": ["exit_code", "stdout", "stderr"]
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["process".to_string(), "execute".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "browser.navigate".to_string(),
                name: "Browser Navigate".to_string(),
                category: CapabilityCategory::Browser,
                version: "1.0.0".to_string(),
                provider: "james-automation".to_string(),
                description: "Navigate to a URL in a browser".to_string(),
                risk_level: RiskLevel::Medium,
                required_permissions: vec!["browser.control".to_string()],
                dependencies: vec![],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "format": "uri" },
                        "wait_until": { "type": "string", "enum": ["load", "domcontentloaded", "networkidle"], "default": "networkidle" },
                        "timeout_secs": { "type": "number", "default": 30 }
                    },
                    "required": ["url"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "url": { "type": "string" },
                        "screenshot": { "type": "string", "format": "base64" }
                    }
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["browser".to_string(), "navigate".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "voice.transcribe".to_string(),
                name: "Voice Transcription (STT)".to_string(),
                category: CapabilityCategory::Voice,
                version: "1.0.0".to_string(),
                provider: "james-voice".to_string(),
                description: "Transcribe audio to text".to_string(),
                risk_level: RiskLevel::Low,
                required_permissions: vec!["audio.input".to_string()],
                dependencies: vec!["ai.local.inference".to_string()],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "audio_data": { "type": "string", "format": "base64" },
                        "language": { "type": "string", "default": "auto" },
                        "model": { "type": "string", "default": "whisper-base" }
                    },
                    "required": ["audio_data"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "language": { "type": "string" },
                        "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
                    },
                    "required": ["text"]
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["voice".to_string(), "stt".to_string(), "transcription".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "voice.synthesize".to_string(),
                name: "Voice Synthesis (TTS)".to_string(),
                category: CapabilityCategory::Voice,
                version: "1.0.0".to_string(),
                provider: "james-voice".to_string(),
                description: "Synthesize text to speech".to_string(),
                risk_level: RiskLevel::Low,
                required_permissions: vec!["audio.output".to_string()],
                dependencies: vec!["ai.local.inference".to_string()],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "voice": { "type": "string", "default": "default" },
                        "speed": { "type": "number", "default": 1.0, "minimum": 0.5, "maximum": 2.0 },
                        "format": { "type": "string", "enum": ["wav", "mp3", "ogg"], "default": "wav" }
                    },
                    "required": ["text"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "audio_data": { "type": "string", "format": "base64" },
                        "duration_ms": { "type": "number" },
                        "format": { "type": "string" }
                    },
                    "required": ["audio_data"]
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["voice".to_string(), "tts".to_string(), "synthesis".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "ai.local.inference".to_string(),
                name: "Local AI Inference".to_string(),
                category: CapabilityCategory::Ai,
                version: "1.0.0".to_string(),
                provider: "james-ai".to_string(),
                description: "Run local AI model inference".to_string(),
                risk_level: RiskLevel::Medium,
                required_permissions: vec!["ai.inference".to_string()],
                dependencies: vec![],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model": { "type": "string" },
                        "prompt": { "type": "string" },
                        "parameters": { "type": "object" },
                        "stream": { "type": "boolean", "default": false }
                    },
                    "required": ["model", "prompt"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "response": { "type": "string" },
                        "tokens_used": { "type": "number" },
                        "model": { "type": "string" },
                        "finish_reason": { "type": "string" }
                    },
                    "required": ["response"]
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["ai".to_string(), "llm".to_string(), "inference".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "device.bluetooth.scan".to_string(),
                name: "Bluetooth Scan".to_string(),
                category: CapabilityCategory::Device,
                version: "1.0.0".to_string(),
                provider: "james-device".to_string(),
                description: "Scan for Bluetooth devices".to_string(),
                risk_level: RiskLevel::Low,
                required_permissions: vec!["bluetooth.scan".to_string()],
                dependencies: vec![],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "duration_secs": { "type": "number", "default": 10 },
                        "filter": { "type": "object" }
                    }
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "devices": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "address": { "type": "string" },
                                    "name": { "type": "string" },
                                    "rssi": { "type": "number" },
                                    "services": { "type": "array", "items": { "type": "string" } }
                                }
                            }
                        }
                    }
                })),
                execution_target: ExecutionTarget::Local,
                tags: vec!["bluetooth".to_string(), "scan".to_string()],
                deprecated: false,
                experimental: false,
            },
            CapabilityDefinition {
                id: "smart_home.home_assistant.call_service".to_string(),
                name: "Home Assistant Call Service".to_string(),
                category: CapabilityCategory::SmartHome,
                version: "1.0.0".to_string(),
                provider: "james-smart-home".to_string(),
                description: "Call a Home Assistant service".to_string(),
                risk_level: RiskLevel::Medium,
                required_permissions: vec!["smart_home.control".to_string()],
                dependencies: vec![],
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "domain": { "type": "string" },
                        "service": { "type": "string" },
                        "entity_id": { "type": "string" },
                        "service_data": { "type": "object" }
                    },
                    "required": ["domain", "service"]
                })),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "success": { "type": "boolean" },
                        "state": { "type": "object" }
                    }
                })),
                execution_target: ExecutionTarget::Remote("home-assistant".to_string()),
                tags: vec!["smart-home".to_string(), "home-assistant".to_string()],
                deprecated: false,
                experimental: false,
            },
        ];

        for cap in builtins {
            self.register(cap, "james-core").await.ok();
        }

        Ok(())
    }

    pub fn get_schema(&self, id: &str) -> Option<schemars::schema::RootSchema> {
        if let Some(_cap) = self.capabilities.get(id) {
            Some(schema_for!(CapabilityDefinition))
        } else {
            None
        }
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capability_registry_register() {
        let registry = CapabilityRegistry::new();
        registry.start().await.unwrap();

        let cap = CapabilityDefinition {
            id: "test.capability".to_string(),
            name: "Test Capability".to_string(),
            category: CapabilityCategory::Custom("test".to_string()),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "A test capability".to_string(),
            risk_level: RiskLevel::Low,
            required_permissions: vec!["test.permission".to_string()],
            dependencies: vec![],
            input_schema: Some(serde_json::json!({"type": "object"})),
            output_schema: Some(serde_json::json!({"type": "object"})),
            execution_target: ExecutionTarget::Local,
            tags: vec!["test".to_string()],
            deprecated: false,
            experimental: false,
        };

        registry.register(cap, "test").await.unwrap();

        let retrieved = registry.get("test.capability").unwrap();
        assert_eq!(retrieved.definition.id, "test.capability");
        assert_eq!(retrieved.definition.risk_level, RiskLevel::Low);
        assert_eq!(retrieved.status, CapabilityStatus::Available);
        assert_eq!(retrieved.usage_count, 0);
    }

    #[tokio::test]
    async fn test_capability_registry_unregister() {
        let registry = CapabilityRegistry::new();
        registry.start().await.unwrap();

        let cap = CapabilityDefinition {
            id: "test.capability".to_string(),
            name: "Test Capability".to_string(),
            category: CapabilityCategory::Custom("test".to_string()),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "A test capability".to_string(),
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

        registry.register(cap, "test").await.unwrap();
        let unregistered = registry.unregister("test.capability").await.unwrap();
        assert!(unregistered);
        assert!(registry.get("test.capability").is_none());
    }

    #[tokio::test]
    async fn test_capability_registry_list_by_category() {
        let registry = CapabilityRegistry::new();
        registry.start().await.unwrap();

        // Builtins (see register_builtin_capabilities): File x2 (read/write),
        // Voice x2 (transcribe/synthesize). No System builtins exist.
        let file_caps = registry.list_by_category(CapabilityCategory::File);
        assert!(file_caps.len() >= 2);

        let voice_caps = registry.list_by_category(CapabilityCategory::Voice);
        assert!(voice_caps.len() >= 2);
    }

    #[tokio::test]
    async fn test_capability_registry_usage_tracking() {
        let registry = CapabilityRegistry::new();
        registry.start().await.unwrap();

        let cap = CapabilityDefinition {
            id: "test.usage".to_string(),
            name: "Test Usage".to_string(),
            category: CapabilityCategory::Custom("test".to_string()),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "Test usage tracking".to_string(),
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

        registry.register(cap, "test").await.unwrap();
        assert_eq!(registry.get("test.usage").unwrap().usage_count, 0);

        registry.record_usage("test.usage").unwrap();
        assert_eq!(registry.get("test.usage").unwrap().usage_count, 1);

        registry.record_usage("test.usage").unwrap();
        assert_eq!(registry.get("test.usage").unwrap().usage_count, 2);
        assert!(registry.get("test.usage").unwrap().last_used.is_some());
    }

    #[tokio::test]
    async fn test_capability_registry_validation() {
        let registry = CapabilityRegistry::new();
        registry.start().await.unwrap();

        let cap = CapabilityDefinition {
            id: "test.validation".to_string(),
            name: "Test Validation".to_string(),
            category: CapabilityCategory::Custom("test".to_string()),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "Test validation".to_string(),
            risk_level: RiskLevel::Low,
            required_permissions: vec![],
            dependencies: vec![],
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "minLength": 1 },
                    "value": { "type": "number", "minimum": 0 }
                },
                "required": ["name"]
            })),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": { "type": "string" }
                }
            })),
            execution_target: ExecutionTarget::Local,
            tags: vec![],
            deprecated: false,
            experimental: false,
        };

        registry.register(cap, "test").await.unwrap();

        let valid = registry.validate_input("test.validation", &serde_json::json!({
            "name": "test",
            "value": 42
        }));
        assert!(valid.is_ok());

        let invalid = registry.validate_input("test.validation", &serde_json::json!({
            "value": -1
        }));
        assert!(invalid.is_err());
    }
}