//! James-TextInput - Text input interface module for JAMES
//!
//! Provides the `text.input` capability for reading text from stdin or other sources.

use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleManifestValidator, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{info};
use uuid::Uuid;

/// Text input configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextInputConfig {
    pub echo: bool,
    pub prompt: String,
    pub history_size: usize,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            echo: true,
            prompt: "> ".to_string(),
            history_size: 1000,
        }
    }
}

/// Text input event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextInputEvent {
    pub session_id: String,
    pub text: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Text input module state
pub struct TextInputModule {
    config: TextInputConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    input_tx: mpsc::Sender<String>,
    input_rx: Arc<RwLock<Option<mpsc::Receiver<String>>>>,
    session_id: String,
}

impl TextInputModule {
    pub fn new(
        config: TextInputConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
    ) -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(100);
        let session_id = Uuid::now_v7().to_string();

        let module = Self {
            config,
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            input_tx: tx,
            input_rx: Arc::new(RwLock::new(Some(rx))),
            session_id,
        };

        let rx = module.input_rx.blocking_write().take().unwrap();
        (module, rx)
    }

    /// Start the text input loop
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-TextInput started");

        // Emit capability registered event
        self.event_bus
            .publish(Event::new("module.textinput.started", "james-textinput")
                .with_payload(serde_json::json!({
                    "session_id": self.session_id,
                    "capability": "text.input",
                })))
            .await?;

        Ok(())
    }

    /// Stop the text input loop
    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-TextInput stopped");

        self.event_bus
            .publish(Event::new("module.textinput.stopped", "james-textinput")
                .with_payload(serde_json::json!({
                    "session_id": self.session_id,
                })))
            .await?;

        Ok(())
    }

    /// Get the input sender for external input injection (e.g., from UI)
    pub fn input_sender(&self) -> mpsc::Sender<String> {
        self.input_tx.clone()
    }

    /// Check if running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

/// Module manifest for James-TextInput
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.textinput".to_string(),
        name: "James-TextInput".to_string(),
        version: "0.1.0".to_string(),
        description: "Text input interface module for JAMES".to_string(),
        module_type: ModuleType::Interface,
        entry_point: "james_textinput".to_string(),
        capabilities: vec!["text.input".to_string()],
        dependencies: vec![],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "echo": {"type": "boolean", "default": true},
                "prompt": {"type": "string", "default": "> "},
                "history_size": {"type": "integer", "default": 1000}
            }
        })),
        default_config: Some(serde_json::json!({
            "echo": true,
            "prompt": "> ",
            "history_size": 1000
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["interface".to_string(), "input".to_string(), "text".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Module capabilities registration
pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    let cap = CapabilityDefinition {
        id: "text.input".to_string(),
        name: "Text Input".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("interface".to_string()),
        version: "1.0.0".to_string(),
        provider: "james.textinput".to_string(),
        description: "Read text input from user".to_string(),
        risk_level: RiskLevel::Low,
        required_permissions: vec![],
        dependencies: vec![],
        input_schema: None,
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "session_id": {"type": "string"},
                "timestamp": {"type": "string", "format": "date-time"}
            }
        })),
        execution_target: ExecutionTarget::Local,
        tags: vec!["input".to_string(), "text".to_string()],
        deprecated: false,
        experimental: false,
    };
    registry.register(cap, "james.textinput".to_string()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;

    #[tokio::test]
    async fn test_textinput_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.textinput");
        assert_eq!(manifest.capabilities, vec!["text.input"]);
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }

    #[tokio::test]
    async fn test_textinput_module_creation() {
        let event_bus = Arc::new(EventBus::new(100));
        event_bus.start().await.unwrap();

        let capability_registry = Arc::new(CapabilityRegistry::new());
        let config = TextInputConfig::default();

        let (module, _rx) = TextInputModule::new(config, event_bus, capability_registry);
        assert!(!module.is_running().await);

        module.start().await.unwrap();
        assert!(module.is_running().await);

        module.stop().await.unwrap();
        assert!(!module.is_running().await);
    }
}