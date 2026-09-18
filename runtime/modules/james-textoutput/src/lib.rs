//! James-TextOutput - Text output interface module for JAMES
//!
//! Provides the `text.output` capability for writing text to stdout or other sinks.

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info};
use uuid::Uuid;

/// Text output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextOutputConfig {
    pub colored: bool,
    pub timestamp_format: String,
    pub max_line_length: usize,
}

impl Default for TextOutputConfig {
    fn default() -> Self {
        Self {
            colored: true,
            timestamp_format: "%H:%M:%S".to_string(),
            max_line_length: 10000,
        }
    }
}

/// Text output event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextOutputEvent {
    pub session_id: String,
    pub text: String,
    pub level: OutputLevel,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Output level for styling
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Success,
    System,
}

/// Text output module state
pub struct TextOutputModule {
    config: TextOutputConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    session_id: String,
    output_buffer: Arc<RwLock<Vec<TextOutputEvent>>>,
}

impl TextOutputModule {
    pub fn new(
        config: TextOutputConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
    ) -> Self {
        let session_id = Uuid::now_v7().to_string();
        Self {
            config,
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            session_id,
            output_buffer: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the text output module
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-TextOutput started");

        self.event_bus
            .publish(Event::new("module.textoutput.started", "james-textoutput")
                .with_payload(serde_json::json!({
                    "session_id": self.session_id,
                    "capability": "text.output",
                })))
            .await?;

        Ok(())
    }

    /// Stop the text output module
    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-TextOutput stopped");

        self.event_bus
            .publish(Event::new("module.textoutput.stopped", "james-textoutput")
                .with_payload(serde_json::json!({
                    "session_id": self.session_id,
                })))
            .await?;

        Ok(())
    }

    /// Write text output
    pub async fn write(&self, text: &str, level: OutputLevel) -> Result<()> {
        if !*self.running.read().await {
            return Err(anyhow::anyhow!("TextOutput module not running"));
        }

        let event = TextOutputEvent {
            session_id: self.session_id.clone(),
            text: text.to_string(),
            level,
            timestamp: chrono::Utc::now(),
        };

        // Store in buffer
        self.output_buffer.write().await.push(event.clone());

        // Emit to event bus
        self.event_bus
            .publish(Event::new("text.output", "james-textoutput")
                .with_payload(serde_json::to_value(&event)?))
            .await?;

        // Also print to stdout for CLI mode
        self.print_to_stdout(&event).await;

        Ok(())
    }

    /// Print to stdout with formatting
    async fn print_to_stdout(&self, event: &TextOutputEvent) {
        let timestamp = event.timestamp.format(&self.config.timestamp_format).to_string();
        let prefix = match event.level {
            OutputLevel::Error => "\x1b[31m[ERROR]\x1b[0m",
            OutputLevel::Warn => "\x1b[33m[WARN]\x1b[0m",
            OutputLevel::Info => "\x1b[36m[INFO]\x1b[0m",
            OutputLevel::Debug => "\x1b[90m[DEBUG]\x1b[0m",
            OutputLevel::Trace => "\x1b[90m[TRACE]\x1b[0m",
            OutputLevel::Success => "\x1b[32m[OK]\x1b[0m",
            OutputLevel::System => "\x1b[35m[SYS]\x1b[0m",
        };

        let line = format!("{} [{}] {}", timestamp, prefix, event.text);
        println!("{}", line);
    }

    /// Check if running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get recent output history
    pub async fn get_history(&self, limit: usize) -> Vec<TextOutputEvent> {
        let buffer = self.output_buffer.read().await;
        buffer.iter().rev().take(limit).cloned().collect()
    }
}

/// Module manifest for James-TextOutput
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.textoutput".to_string(),
        name: "James-TextOutput".to_string(),
        version: "0.1.0".to_string(),
        description: "Text output interface module for JAMES".to_string(),
        module_type: ModuleType::Interface,
        entry_point: "james_textoutput".to_string(),
        capabilities: vec!["text.output".to_string()],
        dependencies: vec![],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "colored": {"type": "boolean", "default": true},
                "timestamp_format": {"type": "string", "default": "%H:%M:%S"},
                "max_line_length": {"type": "integer", "default": 10000}
            }
        })),
        default_config: Some(serde_json::json!({
            "colored": true,
            "timestamp_format": "%H:%M:%S",
            "max_line_length": 10000
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["interface".to_string(), "output".to_string(), "text".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Module capabilities registration
pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    let cap = CapabilityDefinition {
        id: "text.output".to_string(),
        name: "Text Output".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("interface".to_string()),
        version: "1.0.0".to_string(),
        provider: "james.textoutput".to_string(),
        description: "Write text output to user".to_string(),
        risk_level: RiskLevel::Low,
        required_permissions: vec![],
        dependencies: vec![],
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "level": {"type": "string", "enum": ["trace", "debug", "info", "warn", "error", "success", "system"]}
            },
            "required": ["text"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {"type": "string"},
                "text": {"type": "string"},
                "level": {"type": "string"},
                "timestamp": {"type": "string", "format": "date-time"}
            }
        })),
        execution_target: ExecutionTarget::Local,
        tags: vec!["output".to_string(), "text".to_string()],
        deprecated: false,
        experimental: false,
    };
    registry.register(cap, "james.textoutput".to_string()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;
    use james_module_host::ModuleManifestValidator;

    #[tokio::test]
    async fn test_textoutput_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.textoutput");
        assert_eq!(manifest.capabilities, vec!["text.output"]);
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }

    #[tokio::test]
    async fn test_textoutput_module_creation() {
        let event_bus = Arc::new(EventBus::new(100));
        event_bus.start().await.unwrap();

        let capability_registry = Arc::new(CapabilityRegistry::new());
        let config = TextOutputConfig::default();

        let module = TextOutputModule::new(config, event_bus, capability_registry);
        assert!(!module.is_running().await);

        module.start().await.unwrap();
        assert!(module.is_running().await);

        module.write("Test message", OutputLevel::Info).await.unwrap();

        let history = module.get_history(10).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].text, "Test message");
        assert_eq!(history[0].level, OutputLevel::Info);

        module.stop().await.unwrap();
        assert!(!module.is_running().await);
    }
}