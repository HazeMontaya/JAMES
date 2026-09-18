//! Error types for Market Data Engine

use thiserror::Error;

#[derive(Error, Debug)]
pub enum MarketDataError {
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Provider unavailable: {0}")]
    ProviderUnavailable(String),

    #[error("Rate limit exceeded for provider: {0}")]
    RateLimitExceeded(String),

    #[error("Invalid symbol: {0}")]
    InvalidSymbol(String),

    #[error("Invalid timeframe: {0}")]
    InvalidTimeframe(String),

    #[error("Data not found: {0}")]
    DataNotFound(String),

    #[error("Data validation failed: {0}")]
    ValidationFailed(String),

    #[error("Gap detected in data: {0}")]
    GapDetected(String),

    #[error("Outlier detected: {0}")]
    OutlierDetected(String),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Decimal error: {0}")]
    DecimalError(#[from] rust_decimal::Error),

    #[error("Parse int error: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("Parse float error: {0}")]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("TOML deserialization error: {0}")]
    TomlDeserializeError(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerializeError(#[from] toml::ser::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, MarketDataError>;