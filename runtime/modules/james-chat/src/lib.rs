//! James-Chat - Chat interface module for JAMES
//!
//! Provides a high-level chat interface combining text input and output.

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleManifestValidator, ModuleType, ModuleDependency};
use james_textinput::{TextInputConfig, TextInputModule};
use james_textoutput::{TextOutputConfig, TextOutputModule, OutputLevel};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tracing::{info};
use uuid::Uuid;

/// Chat configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    pub input: TextInputConfig,
    pub output: TextOutputConfig,
    pub welcome_message: Option<String>,
    pub exit_commands: Vec<String>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            input: TextInputConfig::default(),
            output: TextOutputConfig::default(),
            welcome_message: Some("Welcome to JAMES Chat. Type 'exit' or 'quit' to leave.".to_string()),
            exit_commands: vec!["exit".to_string(), "quit".to_string(), "bye".to_string()],
        }
    }
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Message role
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Chat session
#[derive(Clone)]
pub struct ChatSession {
    pub id: String,
    pub messages: Arc<RwLock<Vec<ChatMessage>>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Chat module state
pub struct ChatModule {
    config: ChatConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    input_module: Arc<TextInputModule>,
    output_module: Arc<TextOutputModule>,
    current_session: Arc<RwLock<Option<ChatSession>>>,
    message_tx: mpsc::Sender<ChatMessage>,
    message_rx: Arc<RwLock<Option<mpsc::Receiver<ChatMessage>>>>,
}

impl ChatModule {
    pub fn new(
        config: ChatConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
    ) -> (Self, mpsc::Receiver<ChatMessage>) {
        let (tx, rx) = mpsc::channel(100);

        let input_module = Arc::new(TextInputModule::new(
            config.input.clone(),
            event_bus.clone(),
            capability_registry.clone(),
        ).0);

        let output_module = Arc::new(TextOutputModule::new(
            config.output.clone(),
            event_bus.clone(),
            capability_registry.clone(),
        ));

        let module = Self {
            config,
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            input_module,
            output_module,
            current_session: Arc::new(RwLock::new(None)),
            message_tx: tx,
            message_rx: Arc::new(RwLock::new(None)),
        };

        (module, rx)
    }

    /// Start the chat module
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;

        // Start sub-modules
        self.input_module.start().await?;
        self.output_module.start().await?;

        // Create new session
        let session = ChatSession {
            id: Uuid::now_v7().to_string(),
            messages: Arc::new(RwLock::new(Vec::new())),
            created_at: chrono::Utc::now(),
        };
        *self.current_session.write().await = Some(session.clone());

        // Show welcome message
        if let Some(welcome) = &self.config.welcome_message {
            self.output_module.write(welcome, OutputLevel::System).await?;
        }

        // Emit started event
        self.event_bus
            .publish(Event::new("module.chat.started", "james-chat")
                .with_payload(serde_json::json!({
                    "session_id": session.id,
                    "capabilities": ["chat.interface", "text.input", "text.output"],
                })))
            .await?;

        info!("James-Chat started with session {}", session.id);
        Ok(())
    }

    /// Stop the chat module
    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;

        // Stop sub-modules
        self.input_module.stop().await?;
        self.output_module.stop().await?;

        let session_id = self.current_session.read().await.as_ref().map(|s| s.id.clone());
        *self.current_session.write().await = None;

        if let Some(id) = session_id {
            self.event_bus
                .publish(Event::new("module.chat.stopped", "james-chat")
                    .with_payload(serde_json::json!({
                        "session_id": id,
                    })))
                .await?;
        }

        info!("James-Chat stopped");
        Ok(())
    }

    /// Process a user message
    pub async fn process_user_message(&self, text: &str) -> Result<()> {
        let session = self.current_session.read().await.clone();
        let Some(session) = session else {
            return Err(anyhow::anyhow!("No active chat session"));
        };

        if !*self.running.read().await {
            return Err(anyhow::anyhow!("Chat module not running"));
        }

        // Check for exit commands
        let trimmed = text.trim().to_lowercase();
        if self.config.exit_commands.iter().any(|cmd| cmd == &trimmed) {
            self.output_module.write("Goodbye!", OutputLevel::System).await?;
            self.stop().await?;
            return Ok(());
        }

        // Create user message
        let user_msg = ChatMessage {
            id: Uuid::now_v7().to_string(),
            role: MessageRole::User,
            content: text.to_string(),
            timestamp: chrono::Utc::now(),
        };

        // Store message
        session.messages.write().await.push(user_msg.clone());

        // Echo to output
        self.output_module.write(&format!("You: {}", text), OutputLevel::Info).await?;

        // Emit user message event
        self.event_bus
            .publish(Event::new("chat.message.user", "james-chat")
                .with_payload(serde_json::json!({
                    "session_id": session.id,
                    "message_id": user_msg.id,
                    "content": text,
                })))
            .await?;

        // Forward to message channel for AI processing
        let _ = self.message_tx.send(user_msg).await;

        Ok(())
    }

    /// Add assistant response
    pub async fn add_assistant_message(&self, content: &str) -> Result<()> {
        let session = self.current_session.read().await.clone();
        let Some(session) = session else {
            return Err(anyhow::anyhow!("No active chat session"));
        };

        let assistant_msg = ChatMessage {
            id: Uuid::now_v7().to_string(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
        };

        // Store message
        session.messages.write().await.push(assistant_msg.clone());

        // Display
        self.output_module.write(&format!("Assistant: {}", content), OutputLevel::Success).await?;

        // Emit assistant message event
        self.event_bus
            .publish(Event::new("chat.message.assistant", "james-chat")
                .with_payload(serde_json::json!({
                    "session_id": session.id,
                    "message_id": assistant_msg.id,
                    "content": content,
                })))
            .await?;

        Ok(())
    }

    /// Get current session
    pub async fn get_session(&self) -> Option<ChatSession> {
        self.current_session.read().await.clone()
    }

    /// Get message history
    pub async fn get_history(&self, limit: usize) -> Vec<ChatMessage> {
        if let Some(session) = self.current_session.read().await.as_ref() {
            let messages = session.messages.read().await;
            messages.iter().rev().take(limit).cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Check if running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Get input sender for external input
    pub fn input_sender(&self) -> mpsc::Sender<String> {
        self.input_module.input_sender()
    }

    /// Get output module for direct writes
    pub fn output_module(&self) -> Arc<TextOutputModule> {
        self.output_module.clone()
    }
}

/// Module manifest for James-Chat
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.chat".to_string(),
        name: "James-Chat".to_string(),
        version: "0.1.0".to_string(),
        description: "Chat interface module for JAMES".to_string(),
        module_type: ModuleType::Interface,
        entry_point: "james_chat".to_string(),
        capabilities: vec![
            "chat.interface".to_string(),
            "text.input".to_string(),
            "text.output".to_string(),
        ],
        dependencies: vec![
            ModuleDependency {
                name: "james.textinput".to_string(),
                version: "0.1.0".to_string(),
                optional: false,
                reason: Some("Required for text input".to_string()),
            },
            ModuleDependency {
                name: "james.textoutput".to_string(),
                version: "0.1.0".to_string(),
                optional: false,
                reason: Some("Required for text output".to_string()),
            },
        ],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "object",
                    "properties": {
                        "echo": {"type": "boolean", "default": true},
                        "prompt": {"type": "string", "default": "> "},
                        "history_size": {"type": "integer", "default": 1000}
                    }
                },
                "output": {
                    "type": "object",
                    "properties": {
                        "colored": {"type": "boolean", "default": true},
                        "timestamp_format": {"type": "string", "default": "%H:%M:%S"},
                        "max_line_length": {"type": "integer", "default": 10000}
                    }
                },
                "welcome_message": {"type": "string"},
                "exit_commands": {"type": "array", "items": {"type": "string"}}
            }
        })),
        default_config: Some(serde_json::json!({
            "input": {"echo": true, "prompt": "> ", "history_size": 1000},
            "output": {"colored": true, "timestamp_format": "%H:%M:%S", "max_line_length": 10000},
            "welcome_message": "Welcome to JAMES Chat. Type 'exit' or 'quit' to leave.",
            "exit_commands": ["exit", "quit", "bye"]
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["interface".to_string(), "chat".to_string(), "text".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Module capabilities registration
pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    // Chat interface capability
    let chat_cap = CapabilityDefinition {
        id: "chat.interface".to_string(),
        name: "Chat Interface".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("interface".to_string()),
        version: "1.0.0".to_string(),
        provider: "james.chat".to_string(),
        description: "Interactive chat interface with history".to_string(),
        risk_level: RiskLevel::Low,
        required_permissions: vec![],
        dependencies: vec!["text.input".to_string(), "text.output".to_string()],
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "message": {"type": "string"},
                "session_id": {"type": "string"}
            },
            "required": ["message"]
        })),
        output_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": {"type": "string"},
                "response": {"type": "string"},
                "message_id": {"type": "string"}
            }
        })),
        execution_target: ExecutionTarget::Local,
        tags: vec!["chat".to_string(), "interface".to_string()],
        deprecated: false,
        experimental: false,
    };
    registry.register(chat_cap, "james.chat".to_string()).await?;

    // NOTE: text.input and text.output are owned by james-textinput / james-textoutput.
    // james-chat depends on them and must not register duplicates.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;

    #[tokio::test]
    async fn test_chat_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.chat");
        assert_eq!(manifest.capabilities.len(), 3);
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }

    #[tokio::test]
    async fn test_chat_module_creation() {
        let event_bus = Arc::new(EventBus::new(100));
        event_bus.start().await.unwrap();

        let capability_registry = Arc::new(CapabilityRegistry::new());
        let config = ChatConfig::default();

        let (module, _rx) = ChatModule::new(config, event_bus, capability_registry);
        assert!(!module.is_running().await);

        module.start().await.unwrap();
        assert!(module.is_running().await);

        module.process_user_message("Hello").await.unwrap();

        let history = module.get_history(10).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[0].role, MessageRole::User);

        module.add_assistant_message("Hi there!").await.unwrap();

        let history = module.get_history(10).await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Hi there!");
        assert_eq!(history[0].role, MessageRole::Assistant);

        module.stop().await.unwrap();
        assert!(!module.is_running().await);
    }
}