//! Market data providers

use async_trait::async_trait;
use crate::types::*;
use crate::error::Result;

/// Trait for market data providers
#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    fn provider_id(&self) -> ProviderId;
    fn name(&self) -> &str;
    fn supported_exchanges(&self) -> &[Exchange];
    fn supported_timeframes(&self) -> &[Timeframe];
    fn rate_limit(&self) -> &crate::types::RateLimit;
    
    async fn fetch_ohlcv(&self, request: &crate::types::DataRequest) -> Result<Vec<crate::types::OHLCVBar>>;
    async fn health_check(&self) -> Result<crate::types::ProviderHealth>;
    fn get_rate_limiter(&self) -> Option<&RateLimiter>;
}

/// Rate limiter for API requests
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use std::collections::VecDeque;

pub struct RateLimiter {
    max_per_second: u32,
    max_per_minute: u32,
    max_per_hour: u32,
    max_per_day: u32,
    requests: Mutex<VecDeque<Instant>>,
}

impl RateLimiter {
    pub fn new(rate_limit: &crate::types::RateLimit) -> Self {
        Self {
            max_per_second: rate_limit.requests_per_second,
            max_per_minute: rate_limit.requests_per_minute,
            max_per_hour: rate_limit.requests_per_hour,
            max_per_day: rate_limit.requests_per_day,
            requests: Mutex::new(VecDeque::new()),
        }
    }

    pub async fn acquire(&self) -> Result<()> {
        let mut requests = self.requests.lock().await;
        let now = Instant::now();
        
        // Remove old entries
        let day_ago = now - Duration::from_secs(86400);
        while let Some(&front) = requests.front() {
            if front < day_ago {
                requests.pop_front();
            } else {
                break;
            }
        }

        // Check limits
        let mut sec_count = 0;
        let mut min_count = 0;
        let mut hour_count = 0;
        let mut day_count = 0;

        for &req_time in requests.iter() {
            let elapsed = now.duration_since(req_time);
            if elapsed < Duration::from_secs(1) { sec_count += 1; }
            if elapsed < Duration::from_secs(60) { min_count += 1; }
            if elapsed < Duration::from_secs(3600) { hour_count += 1; }
            day_count += 1;
        }

        if sec_count >= self.max_per_second as usize {
            return Err(crate::error::MarketDataError::RateLimitExceeded("per second".to_string()));
        }
        if min_count >= self.max_per_minute as usize {
            return Err(crate::error::MarketDataError::RateLimitExceeded("per minute".to_string()));
        }
        if hour_count >= self.max_per_hour as usize {
            return Err(crate::error::MarketDataError::RateLimitExceeded("per hour".to_string()));
        }
        if day_count >= self.max_per_day as usize {
            return Err(crate::error::MarketDataError::RateLimitExceeded("per day".to_string()));
        }

        requests.push_back(now);
        Ok(())
    }
}

/// Provider registry for managing multiple providers
pub struct ProviderRegistry {
    providers: Vec<Box<dyn MarketDataProvider>>,
    default_provider: ProviderId,
    fallback_enabled: bool,
}

impl ProviderRegistry {
    pub fn new(default_provider: ProviderId, fallback_enabled: bool) -> Self {
        Self {
            providers: Vec::new(),
            default_provider,
            fallback_enabled,
        }
    }

    pub fn register(&mut self, provider: Box<dyn MarketDataProvider>) {
        self.providers.push(provider);
    }

    pub fn get(&self, id: ProviderId) -> Option<&Box<dyn MarketDataProvider>> {
        self.providers.iter().find(|p| p.provider_id() == id)
    }

    pub fn get_default(&self) -> Option<&Box<dyn MarketDataProvider>> {
        self.get(self.default_provider)
    }

    pub fn list(&self) -> Vec<ProviderId> {
        self.providers.iter().map(|p| p.provider_id()).collect()
    }
    
    pub fn providers(&self) -> Vec<ProviderId> {
        self.list()
    }
}

// Provider modules
pub mod yahoo;
pub mod binance;

// Re-export providers
pub use yahoo::YahooProvider;
pub use binance::BinanceProvider;