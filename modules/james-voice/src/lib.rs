//! James-Voice - Voice interface orchestration for JAMES

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use james_stt::SttModule;
use james_tts::TtsModule;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    pub wake_word: Option<String>,
    pub auto_speak: bool,
}

impl Default for VoiceConfig {
    fn default() -> Self { Self { wake_word: Some("Hey James".to_string()), auto_speak: true } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceMessage {
    pub input_text: Option<String>,
    pub input_audio: Option<Vec<u8>>,
    pub output_text: Option<String>,
    pub output_audio: Option<Vec<u8>>,
    pub language: String,
}

pub struct VoiceModule {
    config: VoiceConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    stt: Arc<SttModule>,
    tts: Arc<TtsModule>,
}

impl VoiceModule {
    pub fn new(config: VoiceConfig, event_bus: Arc<EventBus>, capability_registry: Arc<CapabilityRegistry>, stt: Arc<SttModule>, tts: Arc<TtsModule>) -> Self {
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)), stt, tts }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-Voice started (wake_word={:?})", self.config.wake_word);
        self.event_bus.publish(Event::new("module.voice.started", "james-voice")).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-Voice stopped");
        self.event_bus.publish(Event::new("module.voice.stopped", "james-voice")).await?;
        Ok(())
    }

    pub async fn process_voice_input(&self, audio_data: &[u8]) -> Result<VoiceMessage> {
        let transcription = self.stt.transcribe(audio_data).await?;
        Ok(VoiceMessage {
            input_audio: Some(audio_data.to_vec()),
            input_text: Some(transcription.text),
            output_text: None, output_audio: None,
            language: transcription.language,
        })
    }

    pub async fn speak(&self, text: &str) -> Result<VoiceMessage> {
        let audio = self.tts.synthesize(text).await?;
        Ok(VoiceMessage {
            input_text: Some(text.to_string()), input_audio: None,
            output_text: Some(text.to_string()), output_audio: Some(audio.audio_data),
            language: "en".to_string(),
        })
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.voice".to_string(), name: "James-Voice".to_string(), version: "0.1.0".to_string(),
        description: "Voice interface orchestration for JAMES".to_string(), module_type: ModuleType::Interface,
        entry_point: "james_voice".to_string(),
        capabilities: vec!["voice.input".to_string(), "voice.output".to_string(), "voice.wakeword".to_string()],
        dependencies: vec![
            james_module_host::ModuleDependency { name: "james.stt".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Speech recognition".to_string()) },
            james_module_host::ModuleDependency { name: "james.tts".to_string(), version: "0.1.0".to_string(), optional: false, reason: Some("Speech synthesis".to_string()) },
        ],
        permissions: vec![],
        configuration_schema: None, default_config: None,
        author: Some("JAMES Project".to_string()), homepage: None, repository: None,
        license: "MIT".to_string(), tags: vec!["voice".to_string(), "speech".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id, name, desc) in [
        ("voice.input", "Voice Input", "Process voice input"),
        ("voice.output", "Voice Output", "Generate voice output"),
        ("voice.wakeword", "Voice Wake Word", "Wake word detection"),
    ] {
        registry.register(CapabilityDefinition {
            id: id.to_string(), name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("voice".to_string()),
            version: "1.0.0".to_string(), provider: "james.voice".to_string(),
            description: desc.to_string(), risk_level: RiskLevel::Low,
            required_permissions: vec![], dependencies: vec![],
            input_schema: None, output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["voice".to_string()], deprecated: false, experimental: false,
        }, "james.voice".to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_voice_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.voice");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}