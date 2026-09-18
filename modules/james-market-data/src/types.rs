//! Core types for Market Data Engine

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Market data provider identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl ProviderId {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderId::Yahoo => "yahoo",
            ProviderId::Binance => "binance",
            ProviderId::Polygon => "polygon",
            ProviderId::AlphaVantage => "alphavantage",
            ProviderId::TwelveData => "twelvedata",
            ProviderId::CoinGecko => "coingecko",
            ProviderId::Custom(_) => "custom",
        }
    }
}

/// Market data provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Financial symbol
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCVBar {
    pub timestamp: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub amount: Option<Decimal>, // quote volume (close * volume)
    pub trades: Option<u64>,     // number of trades
}

/// Normalized OHLCV data for Kronos
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KronosOHLCV {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub bars: Vec<OHLCVBar>,
    pub metadata: DataMetadata,
}

/// Metadata about the data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMetadata {
    pub provider: ProviderId,
    pub fetched_at: DateTime<Utc>,
    pub first_timestamp: DateTime<Utc>,
    pub last_timestamp: DateTime<Utc>,
    pub bar_count: usize,
    pub gaps_detected: usize,
    pub data_quality_score: f32, // 0.0 - 1.0
}

/// Data request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRequest {
    pub symbol: Symbol,
    pub exchange: Exchange,
    pub timeframe: Timeframe,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub lookback: Option<usize>,
}

/// Market data engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataConfig {
    pub providers: Vec<ProviderConfig>,
    pub cache_dir: String,
    pub default_provider: ProviderId,
    pub fallback_enabled: bool,
    pub max_concurrent_requests: usize,
    pub request_timeout_secs: u64,
    pub validation: ValidationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub check_gaps: bool,
    pub max_gap_threshold: u64, // seconds
    pub check_outliers: bool,
    pub outlier_threshold_std: f64,
    pub min_volume_threshold: Option<Decimal>,
}

/// Provider health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider: ProviderId,
    pub healthy: bool,
    pub last_check: DateTime<Utc>,
    pub latency_ms: u64,
    pub error_rate: f32,
    pub last_error: Option<String>,
}

/// Market data engine statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub total_latency_ms: u64,
    pub avg_latency_ms: f64,
}