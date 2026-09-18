//! JAMES Shared Types
//!
//! Common types shared across all JAMES crates to avoid duplication.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Capability identifier for JAMES capability system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CapabilityId(pub String);

impl CapabilityId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl From<String> for CapabilityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CapabilityId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an adapter instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
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

/// Financial symbol
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Exchange {
    Nyse,
    Nasdaq,
    Binance,
    BinanceUs,
    Bybit,
    Okx,
    Coinbase,
    Kraken,
    Polygon,
    Yahoo,
    Custom(u32),
}

/// Timeframe for OHLCV data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
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
    M1Month,
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
            Self::M1Month => 2592000,
        }
    }
    
    pub fn to_chrono_duration(&self) -> chrono::Duration {
        chrono::Duration::seconds(self.to_seconds() as i64)
    }
}

/// OHLCV bar data
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OHLCVBar {
    pub timestamp: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub amount: Option<Decimal>,
    pub trades: Option<u64>,
}

/// Data request
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DataRequest {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub lookback: Option<usize>,
}

/// Provider identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Yahoo,
    Binance,
    Polygon,
    AlphaVantage,
    TwelveData,
    CoinGecko,
    Custom(u32),
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProviderConfig {
    pub id: ProviderId,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub rate_limit: RateLimit,
    pub enabled: bool,
    pub priority: u32,
    pub supported_exchanges: Vec<Exchange>,
    pub supported_timeframes: Vec<Timeframe>,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RateLimit {
    pub requests_per_second: u32,
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub requests_per_day: u32,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            requests_per_second: 10,
            requests_per_minute: 100,
            requests_per_hour: 1000,
            requests_per_day: 10000,
        }
    }
}

/// Provider health status
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProviderHealth {
    pub provider: ProviderId,
    pub healthy: bool,
    pub last_check: DateTime<Utc>,
    pub latency_ms: u64,
    pub error_rate: f32,
    pub last_error: Option<String>,
}

/// Market data engine configuration
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MarketDataConfig {
    pub providers: Vec<ProviderConfig>,
    pub cache_dir: String,
    pub default_provider: ProviderId,
    pub fallback_enabled: bool,
    pub max_concurrent_requests: usize,
    pub request_timeout_secs: u64,
    pub validation: ValidationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ValidationConfig {
    pub check_gaps: bool,
    pub max_gap_threshold: u64,
    pub check_outliers: bool,
    pub outlier_threshold_std: f64,
    pub min_volume_threshold: Option<Decimal>,
}

/// Market data engine statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EngineStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub total_latency_ms: u64,
    pub avg_latency_ms: f64,
}

/// Data metadata
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DataMetadata {
    pub provider: ProviderId,
    pub fetched_at: DateTime<Utc>,
    pub first_timestamp: DateTime<Utc>,
    pub last_timestamp: DateTime<Utc>,
    pub bar_count: usize,
    pub gaps_detected: usize,
    pub data_quality_score: f32,
}

/// Kronos model variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KronosVariant {
    Mini,
    Small,
    Base,
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

/// Sampling configuration for probabilistic forecasting
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SamplingConfig {
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default = "default_sample_count")]
    pub sample_count: usize,
}

fn default_temperature() -> f32 { 1.0 }
fn default_top_p() -> f32 { 0.9 }
fn default_sample_count() -> usize { 1 }

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            top_p: default_top_p(),
            sample_count: default_sample_count(),
        }
    }
}

/// Forecast request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastRequest {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub lookback: usize,
    pub pred_len: usize,
    pub timestamp: chrono::DateTime<Utc>,
    #[serde(default)]
    pub sampling: SamplingConfig,
    #[serde(default = "default_true")]
    pub include_volume: bool,
    #[serde(default)]
    pub include_amount: bool,
}

fn default_true() -> bool { true }

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
    pub prediction_intervals: Vec<PredictionInterval>,
    pub ensemble_std: f64,
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
    pub timestamp: chrono::DateTime<Utc>,
}

/// Market embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEmbeddings {
    pub symbol: Symbol,
    pub embeddings: Vec<f32>,
    pub timestamp: chrono::DateTime<Utc>,
    pub lookback: usize,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub lookback: usize,
    pub timestamp: chrono::DateTime<Utc>,
}

/// Finetuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneConfig {
    pub symbols: Vec<Symbol>,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub train_start: chrono::DateTime<Utc>,
    pub train_end: chrono::DateTime<Utc>,
    pub val_start: chrono::DateTime<Utc>,
    pub val_end: chrono::DateTime<Utc>,
    pub model_variant: KronosVariant,
    pub epochs: usize,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub device: String,
    pub output_dir: String,
    pub kronos_repo_path: String,
}

/// Finetuning result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneResult {
    pub tokenizer_path: String,
    pub predictor_path: String,
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

/// Capability identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityId(pub String);

impl CapabilityId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl From<String> for CapabilityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CapabilityId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Adapter identity
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

/// Market embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketEmbeddings {
    pub symbol: Symbol,
    pub embeddings: Vec<f32>,
    pub timestamp: chrono::DateTime<Utc>,
    pub lookback: usize,
}

/// Embedding request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub lookback: usize,
    pub timestamp: chrono::DateTime<Utc>,
}

/// Finetuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneConfig {
    pub symbols: Vec<Symbol>,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub train_start: chrono::DateTime<Utc>,
    pub train_end: chrono::DateTime<Utc>,
    pub val_start: chrono::DateTime<Utc>,
    pub val_end: chrono::DateTime<Utc>,
    pub model_variant: KronosVariant,
    pub epochs: usize,
    pub batch_size: usize,
    pub learning_rate: f64,
    pub device: String,
    pub output_dir: String,
    pub kronos_repo_path: String,
}

/// Finetuning result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneResult {
    pub tokenizer_path: String,
    pub predictor_path: String,
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

/// Adapter stats
#[derive(Debug, Default)]
pub struct AdapterStats {
    pub total_predictions: u64,
    pub total_batch_predictions: u64,
    pub total_inference_time_ms: u64,
    pub errors: u64,
    pub last_inference_ms: Option<u64>,
}

/// Adapter capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCapability {
    pub id: CapabilityId,
    pub name: String,
    pub description: String,
}

/// Model handle
#[derive(Debug, Clone)]
pub struct ModelHandle {
    pub variant: KronosVariant,
    pub device: String,
    pub loaded_at: chrono::DateTime<Utc>,
    pub python_object: Option<pyo3::PyObject>,
}

/// Adapter stats
#[derive(Debug, Default)]
pub struct AdapterStats {
    pub total_predictions: u64,
    pub total_batch_predictions: u64,
    pub total_inference_time_ms: u64,
    pub errors: u64,
    pub last_inference_ms: Option<u64>,
}

/// Capability ID wrapper
pub struct CapabilityId(pub String);

impl CapabilityId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl From<String> for CapabilityId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CapabilityId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}