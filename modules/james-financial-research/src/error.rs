//! Error types for Financial Research Orchestrator

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResearchError {
    #[error("Data fetch failed: {0}")]
    DataFetchFailed(String),

    #[error("Indicator calculation failed: {0}")]
    IndicatorCalculationFailed(String),

    #[error("Sentiment analysis failed: {0}")]
    SentimentAnalysisFailed(String),

    #[error("Fundamentals fetch failed: {0}")]
    FundamentalsFetchFailed(String),

    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),

    #[error("Invalid symbol: {0}")]
    InvalidSymbol(String),

    #[error("Insufficient data: {0}")]
    InsufficientData(String),

    #[error("Model inference failed: {0}")]
    ModelInferenceFailed(String),

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

    #[error("Kronos adapter error: {0}")]
    KronosAdapterError(#[from] james_adapter_kronos::error::KronosAdapterError),

    #[error("Market data error: {0}")]
    MarketDataError(#[from] james_market_data::error::MarketDataError),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, ResearchError>;