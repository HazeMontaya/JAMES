//! JAMES Market Data Engine
//!
//! Multi-provider OHLCV data engine with Kronos-compatible normalization

pub mod config;
pub mod engine;
pub mod error;
pub mod normalization;
pub mod providers;
pub mod types;

pub use types::MarketDataConfig;
pub use engine::{MarketDataEngine, MarketDataEngineImpl};
pub use error::{MarketDataError, Result};
pub use providers::{MarketDataProvider, ProviderRegistry, YahooProvider, BinanceProvider};
pub use types::*;