//! James-TTS - Text-to-speech for JAMES

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub provider: String,
    pub voice: String,
    pub rate: u32,
    pub volume: u32,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self { provider: "local".to_string(), voice: "default".to_string(), rate: 150, volume: 100 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioOutput {
    pub audio_data: Vec<u8>,
    pub format: String,
    pub sample_rate: u32,
    pub duration_ms: u64,
}

pub struct TtsModule {
    config: TtsConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
}

impl TtsModule {
    pub fn new(config: TtsConfig, event_bus: Arc<EventBus>, capability_registry: Arc<CapabilityRegistry>) -> Self {
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)) }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-TTS started (provider={}, voice={})", self.config.provider, self.config.voice);
        self.event_bus.publish(Event::new("module.tts.started", "james-tts")).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-TTS stopped");
        self.event_bus.publish(Event::new("module.tts.stopped", "james-tts")).await?;
        Ok(())
    }

    pub async fn synthesize(&self, _text: &str) -> Result<AudioOutput> {
        Ok(AudioOutput {
            audio_data: Vec::new(), format: "wav".to_string(),
            sample_rate: 22050, duration_ms: 0,
        })
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.tts".to_string(), name: "James-TTS".to_string(), version: "0.1.0".to_string(),
        description: "Text-to-speech for JAMES".to_string(), module_type: ModuleType::Service,
        entry_point: "james_tts".to_string(),
        capabilities: vec!["tts.synthesize".to_string()],
        dependencies: vec![], permissions: vec![],
        configuration_schema: None, default_config: None,
        author: Some("JAMES Project".to_string()), homepage: None, repository: None,
        license: "MIT".to_string(), tags: vec!["tts".to_string(), "voice".to_string(), "speech".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    registry.register(CapabilityDefinition {
        id: "tts.synthesize".to_string(), name: "TTS Synthesize".to_string(),
        category: james_capabilities::CapabilityCategory::Custom("voice".to_string()),
        version: "1.0.0".to_string(), provider: "james.tts".to_string(),
        description: "Synthesize text to speech".to_string(), risk_level: RiskLevel::Low,
        required_permissions: vec![], dependencies: vec![],
        input_schema: None, output_schema: None,
        execution_target: ExecutionTarget::Local,
        tags: vec!["tts".to_string()], deprecated: false, experimental: false,
    }, "james.tts".to_string()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_tts_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.tts");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}