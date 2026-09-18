//! Configuration for Kronos Adapter

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KronosAdapterConfig {
    /// Cache directory for downloaded models
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,

    /// Default model variant to load
    #[serde(default)]
    pub default_variant: KronosVariantConfig,

    /// Device to use for inference
    #[serde(default = "default_device")]
    pub device: String,

    /// Maximum batch size for batch inference
    #[serde(default = "default_max_batch")]
    pub max_batch_size: usize,

    /// Enable model compilation (torch.compile)
    #[serde(default)]
    pub compile_model: bool,

    /// Model loading timeout (seconds)
    #[serde(default = "default_load_timeout")]
    pub load_timeout_secs: u64,

    /// Inference timeout (seconds)
    #[serde(default = "default_inference_timeout")]
    pub inference_timeout_secs: u64,

    /// Python environment path (optional, uses system python if not set)
    pub python_path: Option<PathBuf>,

    /// HuggingFace token for private models (optional)
    pub hf_token: Option<String>,

    /// Enable debug logging
    #[serde(default)]
    pub debug: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KronosVariantConfig {
    Mini,
    Small,
    Base,
    Large,
}

impl Default for KronosVariantConfig {
    fn default() -> Self {
        Self::Small
    }
}

impl From<KronosVariantConfig> for crate::types::KronosVariant {
    fn from(v: KronosVariantConfig) -> Self {
        match v {
            KronosVariantConfig::Mini => Self::Mini,
            KronosVariantConfig::Small => Self::Small,
            KronosVariantConfig::Base => Self::Base,
            KronosVariantConfig::Large => Self::Large,
        }
    }
}

fn default_cache_dir() -> PathBuf {
    dirs_next::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("james")
        .join("kronos")
}

fn default_device() -> String {
    // Auto-detect: prefer CUDA, then MPS, then CPU
    if cfg!(target_os = "macos") {
        "mps".to_string()
    } else {
        "cuda".to_string()
    }
}

fn default_max_batch() -> usize {
    8
}

fn default_load_timeout() -> u64 {
    300
}

fn default_inference_timeout() -> u64 {
    60
}

impl Default for KronosAdapterConfig {
    fn default() -> Self {
        Self {
            cache_dir: default_cache_dir(),
            default_variant: Default::default(),
            device: default_device(),
            max_batch_size: default_max_batch(),
            compile_model: false,
            load_timeout_secs: default_load_timeout(),
            inference_timeout_secs: default_inference_timeout(),
            python_path: None,
            hf_token: None,
            debug: false,
        }
    }
}

impl KronosAdapterConfig {
    pub fn load_from_file(path: &PathBuf) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: &PathBuf) -> crate::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}