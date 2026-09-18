//! Configuration for Financial Research Orchestrator

use serde::{Deserialize, Serialize};
use james_adapter_kronos::{Symbol, Timeframe, KronosVariant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchConfig {
    pub symbols: Vec<Symbol>,
    pub default_timeframe: Timeframe,
    pub lookback: usize,
    pub kronos_variant: KronosVariant,
    pub enable_kronos: bool,
    pub enable_indicators: bool,
    pub enable_sentiment: bool,
    pub enable_fundamentals: bool,
    pub max_concurrent_requests: usize,
    pub cache_ttl_secs: u64,
    pub news_sources: Vec<NewsSource>,
    pub fundamentals_providers: Vec<FundamentalsProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum NewsSource {
    AlphaVantage,
    NewsAPI,
    Polygon,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum FundamentalsProvider {
    AlphaVantage,
    Polygon,
    TwelveData,
    Custom(String),
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            symbols: vec![Symbol::new("AAPL"), Symbol::new("MSFT"), Symbol::new("NVDA")],
            default_timeframe: Timeframe::D1,
            lookback: 400,
            kronos_variant: KronosVariant::Small,
            enable_kronos: true,
            enable_indicators: true,
            enable_sentiment: true,
            enable_fundamentals: true,
            max_concurrent_requests: 10,
            cache_ttl_secs: 3600,
            news_sources: vec![NewsSource::AlphaVantage, NewsSource::Polygon],
            fundamentals_providers: vec![FundamentalsProvider::AlphaVantage, FundamentalsProvider::Polygon],
        }
    }
}