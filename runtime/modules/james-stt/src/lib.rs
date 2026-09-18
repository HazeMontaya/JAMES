//! James-STT - Speech-to-text for JAMES

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub provider: String,
    pub model: String,
    pub language: String,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self { provider: "local".to_string(), model: "whisper-base".to_string(), language: "en".to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub text: String,
    pub language: String,
    pub confidence: f64,
    pub duration_ms: u64,
}

pub struct SttModule {
    config: SttConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
}

impl SttModule {
    pub fn new(config: SttConfig, event_bus: Arc<EventBus>, capability_registry: Arc<CapabilityRegistry>) -> Self {
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)) }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-STT started (provider={}, model={})", self.config.provider, self.config.model);
        self.event_bus.publish(Event::new("module.stt.started", "james-stt")).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-STT stopped");
        self.event_bus.publish(Event::new("module.stt.stopped", "james-stt")).await?;
        Ok(())
    }

    pub async fn transcribe(&self, _audio_data: &[u8]) -> Result<Transcription> {
        Ok(Transcription {
            text: String::new(), language: self.config.language.clone(),
            confidence: 0.0, duration_ms: 0,
        })
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.stt".to_string(), name: "James-STT".to_string(), version: "0.1.0".to_string(),
        description: "Speech-to-text for JAMES".to_string(), module_type: ModuleType::Service,
        entry_point: "james_stt".to_string(),
        capabilities: vec!["stt.transcribe".to_string()],
        dependencies: vec![], permissions: vec![],
        configuration_schema: None, default_config: None,
        author: Some("JAMES Project".to_string()), homepage: None, repository: None,
        license: "MIT".to_string(), tags: vec!["stt".to_string(), "voice".to_string(), "speech".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    registry.register(CapabilityDefinition {
        id: "stt.transcribe".to_string(), name: "STT Transcribe".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("voice".to_string()),
        version: "1.0.0".to_string(), provider: "james.stt".to_string(),
        description: "Transcribe speech to text".to_string(), risk_level: RiskLevel::Low,
        required_permissions: vec![], dependencies: vec![],
        input_schema: None, output_schema: None,
        execution_target: ExecutionTarget::Local,
        tags: vec!["stt".to_string()], deprecated: false, experimental: false,
    }, "james.stt".to_string()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_stt_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.stt");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}