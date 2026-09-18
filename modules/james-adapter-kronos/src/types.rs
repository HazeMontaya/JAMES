//! Core types for Kronos Adapter

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Capability identifier for JAMES capability system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId(pub String);

impl CapabilityId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl From<String> for CapabilityId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for CapabilityId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a Kronos adapter instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterId(pub Uuid);

impl AdapterId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AdapterId {
    fn default() -> Self {
        Self::new()
    }
}

/// Kronos model variants available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KronosVariant {
    /// 4.1M params, context=2048
    Mini,
    /// 24.7M params, context=512
    Small,
    /// 102.3M params, context=512
    Base,
    /// 499.2M params, context=512 (not publicly available)
    Large,
}

impl KronosVariant {
    pub fn param_count(&self) -> u64 {
        match self {
            Self::Mini => 4_100_000,
            Self::Small => 24_700_000,
            Self::Base => 102_300_000,
            Self::Large => 499_200_000,
        }
    }

    pub fn context_length(&self) -> usize {
        match self {
            Self::Mini => 2048,
            Self::Small | Self::Base | Self::Large => 512,
        }
    }

    pub fn hf_model_id(&self) -> &'static str {
        match self {
            Self::Mini => "NeoQuasar/Kronos-mini",
            Self::Small => "NeoQuasar/Kronos-small",
            Self::Base => "NeoQuasar/Kronos-base",
            Self::Large => "NeoQuasar/Kronos-large",
        }
    }

    pub fn hf_tokenizer_id(&self) -> &'static str {
        match self {
            Self::Mini => "NeoQuasar/Kronos-Tokenizer-2k",
            Self::Small | Self::Base | Self::Large => "NeoQuasar/Kronos-Tokenizer-base",
        }
    }
}

impl Default for KronosVariant {
    fn default() -> Self {
        Self::Small
    }
}

/// Financial symbol identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol(pub String);

impl Symbol {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into().to_uppercase())
    }
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for Symbol {
    fn default() -> Self {
        Self(String::new())
    }
}

impl From<String> for Symbol {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for Symbol {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// Exchange identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Exchange {
    Nyse,
    Nasdaq,
    Binance,
    Bybit,
    Okx,
    Coinbase,
    Polygon,
    Yahoo,
    Custom(u32),
}

/// Timeframe for OHLCV data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Timeframe {
    M1,
    M5,
    M15,
    M30,
    H1,
    H4,
    D1,
    W1,
}

impl Timeframe {
    pub fn to_seconds(&self) -> u64 {
        match self {
            Self::M1 => 60,
            Self::M5 => 300,
            Self::M15 => 900,
            Self::M30 => 1800,
            Self::H1 => 3600,
            Self::H4 => 14400,
            Self::D1 => 86400,
            Self::W1 => 604800,
        }
    }
}

/// Sampling configuration for probabilistic forecasting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Temperature for sampling (0.1 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Nucleus sampling probability (0.1 - 1.0)
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    /// Number of samples to generate and average
    #[serde(default = "default_sample_count")]
    pub sample_count: usize,
}

fn default_temperature() -> f32 {
    1.0
}
fn default_top_p() -> f32 {
    0.9
}
fn default_sample_count() -> usize {
    1
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            top_p: default_top_p(),
            sample_count: default_sample_count(),
        }
    }
}

/// OHLCV bar data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCVBar {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub amount: Option<f64>,
}

/// Forecast request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastRequest {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub lookback: usize,
    pub pred_len: usize,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub sampling: SamplingConfig,
    #[serde(default = "default_true")]
    pub include_volume: bool,
    #[serde(default)]
    pub include_amount: bool,
}

fn default_true() -> bool {
    true
}

/// Forecast result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastResult {
    pub symbol: Symbol,
    pub forecast: Vec<OHLCVBar>,
    pub confidence: ConfidenceMetrics,
    pub metadata: ForecastMetadata,
}

/// Confidence metrics for forecast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceMetrics {
    /// Prediction intervals per horizon step
    pub prediction_intervals: Vec<PredictionInterval>,
    /// Standard deviation across ensemble samples
    pub ensemble_std: f64,
    /// Model uncertainty estimate
    pub model_uncertainty: f64,
}

/// Prediction interval for a single horizon step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionInterval {
    pub horizon: usize,
    pub lower: f64,
    pub upper: f64,
    pub confidence_level: f64,
}

/// Forecast metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastMetadata {
    pub model_variant: KronosVariant,
    pub inference_time_ms: u64,
    pub lookback_used: usize,
    pub pred_len: usize,
    pub timestamp: DateTime<Utc>,
}

/// Market embeddings (latent representation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEmbeddings {
    pub symbol: Symbol,
    pub embeddings: Vec<f32>,
    pub timestamp: DateTime<Utc>,
    pub lookback: usize,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub lookback: usize,
    pub timestamp: DateTime<Utc>,
}

/// Finetuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneConfig {
    pub symbols: Vec<Symbol>,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub train_start: DateTime<Utc>,
    pub train_end: DateTime<Utc>,
    pub val_start: DateTime<Utc>,
    pub val_end: DateTime<Utc>,
    pub model_variant: KronosVariant,
    pub epochs: usize,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub device: String,
    pub output_dir: PathBuf,
    pub kronos_repo_path: PathBuf,
}

/// Finetuning result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneResult {
    pub tokenizer_path: PathBuf,
    pub predictor_path: PathBuf,
    pub train_loss: f64,
    pub val_loss: f64,
    pub epochs_completed: usize,
    pub training_time_secs: u64,
}

/// Adapter health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterHealth {
    pub healthy: bool,
    pub model_loaded: bool,
    pub tokenizer_loaded: bool,
    pub device: String,
    pub memory_usage_mb: u64,
    pub last_inference_ms: Option<u64>,
    pub error: Option<String>,
}

/// Model handle for loaded Kronos model
#[derive(Debug, Clone)]
pub struct ModelHandle {
    pub variant: KronosVariant,
    pub device: String,
    pub loaded_at: DateTime<Utc>,
    pub python_object: Option<pyo3::PyObject>, // Hold Python model reference
}