//! James-Void - Cosmic executive interface for JAMES

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use james_chat::ChatModule;
use james_ai::AiModule;
use james_memory::{MemoryEntry, MemoryModule, MemoryType};
use james_tasks::TasksModule;
use james_webresearch::WebResearchModule;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoidConfig {
    pub title: String,
    pub theme: String,
    pub show_events: bool,
}

impl Default for VoidConfig {
    fn default() -> Self {
        Self { title: "The Void".to_string(), theme: "dark".to_string(), show_events: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoidMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// Extract a task marker of the form `<task:Name>Description</task>` from assistant output.
/// Returns `(name, description)` when present - the deterministic bridge between
/// chat replies and the task system.

pub struct VoidModule {
    config: VoidConfig,
    event_bus: Arc<EventBus>,
    running: Arc<RwLock<bool>>,
    ai: Arc<AiModule>,
    memory: Arc<MemoryModule>,
    tasks: Arc<TasksModule>,
    messages: Arc<RwLock<Vec<VoidMessage>>>,
}

impl VoidModule {
    pub fn new(config: VoidConfig, event_bus: Arc<EventBus>, capability_registry: Arc<CapabilityRegistry>,
         _chat: Arc<ChatModule>, ai: Arc<AiModule>, memory: Arc<MemoryModule>,
         tasks: Arc<TasksModule>, _web_research: Arc<WebResearchModule>) -> Self {
     let _ = capability_registry;
     Self { config, event_bus, running: Arc::new(RwLock::new(false)),
         ai, memory, tasks, messages: Arc::new(RwLock::new(Vec::new())) }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-Void started (theme={})", self.config.theme);
        self.event_bus.publish(Event::new("module.void.started", "james-void")).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-Void stopped");
        self.event_bus.publish(Event::new("module.void.stopped", "james-void")).await?;
        Ok(())
    }

    /// Parse a task marker `<task:Name>description</task>` from an AI reply.
///
/// This is the deterministic interface between chat output and the task
/// system: when the model signals a distinct task, James-Void derives a
/// Task from it. Returns `(name, description)` or `None` when no marker.

pub async fn send_message(&self, content: &str) -> Result<VoidMessage> {
        let user_msg = VoidMessage {
            id: uuid::Uuid::now_v7().to_string(), role: "user".to_string(),
            content: content.to_string(), timestamp: chrono::Utc::now(), metadata: None,
        };
        self.messages.write().await.push(user_msg);

        // Store in memory
        let memory_entry = MemoryEntry {
            id: String::new(),
            memory_type: MemoryType::Episodic,
            content: content.to_string(),
            embedding: None,
            metadata: serde_json::json!({"source": "james-void"}),
            importance: 1.0,
            access_count: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            tags: vec!["chat".to_string()],
            session_id: None,
            agent_id: None,
        };
        if let Err(e) = self.memory.store(memory_entry).await {
            info!("Failed to store in memory: {}", e);
        }

        // Get AI response
        let request = james_ai::InferenceRequest {
            model_id: None,
            messages: vec![james_ai::ChatMessage {
                role: james_ai::MessageRole::User,
                content: content.to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.7),
            max_tokens: None,
            stream: false,
            response_format: None,
            tools: None,
        };
        let response = self.ai.infer(request).await?;
        let response_text = response.choices.first()
            .map(|c| c.message.content.clone())
            .unwrap_or_else(|| "I'm processing your request.".to_string());

        let assistant_msg = VoidMessage {
            id: uuid::Uuid::now_v7().to_string(), role: "assistant".to_string(),
            content: response_text.clone(), timestamp: chrono::Utc::now(), metadata: None,
        };
        self.messages.write().await.push(assistant_msg.clone());

        // Derive task from assistant reply when the model signals one via <task:...>
        if let Some((task_name, task_desc)) = extract_task_marker(&assistant_msg.content) {
            let task = james_tasks::Task {
                id: String::new(),
                name: task_name,
                description: task_desc,
                capability: "void.task".to_string(),
                payload: serde_json::json!({"source": "james-void", "content": content}),
                priority: james_tasks::TaskPriority::Normal,
                status: james_tasks::TaskStatus::Created,
                dependencies: vec![],
                scheduled_at: None,
                started_at: None,
                completed_at: None,
                result: None,
                error: None,
                retries: 0,
                max_retries: 3,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                assigned_agent: None,
            };
            if let Ok(task_id) = self.tasks.create_task(task).await {
                self.event_bus.publish(Event::new("void.task.delegated", "james-void")
                    .with_payload(serde_json::json!({"task_id": task_id, "name": assistant_msg.content})))
                    .await.ok();
            }
        }

        self.event_bus.publish(Event::new("void.message", "james-void")
            .with_payload(serde_json::json!({"role": "assistant", "content": assistant_msg.content}))).await?;

        Ok(assistant_msg)
    }

    pub async fn get_messages(&self) -> Vec<VoidMessage> {
        self.messages.read().await.clone()
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

/// Extract an explicit task marker `<task:Name>Description</task>` from model output.
/// Returns (name, description) when the reply signals a distinct task the user asked
/// James to run â€” the deterministic bridge from chat to the task system.
fn extract_task_marker(content: &str) -> Option<(String, String)> {
    let open = content.find("<task:")?;
    let after = &content[open + "<task:".len()..];
    let name_end = after.find('>')?;
    let name = after[..name_end].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let rest = &after[name_end + 1..];
    let desc = if let Some(close) = rest.find("</task>") {
        rest[..close].trim().to_string()
    } else {
        // Truncated marker at end of stream: keep the remaining content as
        // the description rather than discarding an in-flight reply.
        rest.trim().to_string()
    };
    Some((name, desc))
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.void".to_string(), name: "James-Void".to_string(), version: "0.1.0".to_string(),
        description: "Cosmic executive interface for JAMES".to_string(), module_type: ModuleType::Interface,
        entry_point: "james_void".to_string(),
        capabilities: vec!["void.chat".to_string(), "void.events".to_string(), "void.control".to_string()],
        dependencies: vec![
            james_module_host::ModuleDependency { name: "james.chat".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Chat interface".to_string()) },
            james_module_host::ModuleDependency { name: "james.ai".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("AI inference".to_string()) },
            james_module_host::ModuleDependency { name: "james.memory".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Memory system".to_string()) },
            james_module_host::ModuleDependency { name: "james.tasks".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Task management".to_string()) },
            james_module_host::ModuleDependency { name: "james.webresearch".to_string(), version: "0.1.0".to_string(), optional: true, reason: Some("Web research".to_string()) },
        ],
        permissions: vec![],
        configuration_schema: None, default_config: None,
        author: Some("JAMES Project".to_string()), homepage: None, repository: None,
        license: "MIT".to_string(), tags: vec!["ui".to_string(), "chat".to_string(), "interface".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id, name, desc) in [
        ("void.chat", "Void Chat", "Chat interface"),
        ("void.events", "Void Events", "Live event stream"),
        ("void.control", "Void Control", "System control"),
    ] {
        registry.register(CapabilityDefinition {
            id: id.to_string(), name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("ui".to_string()),
            version: "1.0.0".to_string(), provider: "james.void".to_string(),
            description: desc.to_string(), risk_level: RiskLevel::Low,
            required_permissions: vec![], dependencies: vec![],
            input_schema: None, output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["ui".to_string()], deprecated: false, experimental: false,
        }, "james.void".to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_void_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.void");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }

    #[test]
    fn extract_marker_happy_path() {
        let (name, desc) = extract_task_marker(
            "Done. <task:Summarize meeting>Condense the Q3 notes into 5 bullets</task> -- James"
        ).expect("marker must be found");
        assert_eq!(name, "Summarize meeting");
        assert_eq!(desc, "Condense the Q3 notes into 5 bullets");
    }

    #[test]
    fn extract_marker_optional_description() {
        let (name, desc) = extract_task_marker(
            "<task:Notify team>Please ping the channel</task>"
        ).expect("marker with description");
        assert_eq!(name, "Notify team");
        assert_eq!(desc, "Please ping the channel");
    }

    #[test]
    fn extract_marker_absent() {
        assert!(extract_task_marker("That sounds good, James.").is_none());
    }

    #[test]
    fn extract_marker_unclosed() {
        let (name, desc) = extract_task_marker(
            "<task:Rename file>The name should become `draft.md`"
        ).expect("markers may be unclosed at the end of a stream");
        assert_eq!(name, "Rename file");
        assert_eq!(desc.trim(), "The name should become `draft.md`");
    }
}
