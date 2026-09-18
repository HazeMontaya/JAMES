//! Market Data Engine implementation

use crate::types::*;
use crate::error::Result;
use crate::providers::{MarketDataProvider, ProviderRegistry};
use crate::normalization::normalize_for_kronos;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::normalization::{GapDetector, OutlierDetector};
use reqwest::Client;
use std::sync::Arc as StdArc;
use tokio::sync::Mutex;

/// Market Data Engine trait
#[async_trait::async_trait]
pub trait MarketDataEngine: Send + Sync {
    async fn get_ohlcv(&self, request: &DataRequest) -> Result<Vec<OHLCVBar>>;
    async fn get_ohlcv_batch(&self, requests: &[DataRequest]) -> Result<Vec<Vec<OHLCVBar>>>;
    async fn get_kronos_data(&self, request: &DataRequest) -> Result<KronosOHLCV>;
    async fn health_check(&self) -> Result<Vec<ProviderHealth>>;
    async fn get_stats(&self) -> EngineStats;
}

/// Main Market Data Engine implementation
pub struct MarketDataEngineImpl {
    config: MarketDataConfig,
    provider_registry: StdArc<RwLock<ProviderRegistry>>,
    client: Client,
    cache: StdArc<RwLock<HashMap<String, Vec<OHLCVBar>>>>,
    stats: StdArc<Mutex<EngineStats>>,
    gap_detector: GapDetector,
    outlier_detector: OutlierDetector,
}

impl MarketDataEngineImpl {
    pub fn new(config: MarketDataConfig) -> Result<Self> {
        Ok(Self {
            config,
            provider_registry: StdArc::new(RwLock::new(ProviderRegistry::new(
                ProviderId::Yahoo,
                true,
            ))),
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?,
            cache: StdArc::new(RwLock::new(HashMap::new())),
            stats: StdArc::new(Mutex::new(EngineStats::default())),
            gap_detector: GapDetector::new(),
            outlier_detector: OutlierDetector::new(),
        })
    }
    
    /// Get OHLCV data for a request
    pub async fn get_ohlcv(&self, request: &DataRequest) -> Result<Vec<OHLCVBar>> {
        let start = Instant::now();
        let cache_key = self.cache_key(request);
        
        // Check cache first
        if let Some(cached) = self.get_cached(&cache_key).await {
            self.record_stats(start.elapsed().as_millis() as u64, true);
            return Ok(cached);
        }
        
        // Try default provider first
        let provider_id = self.config.default_provider;
        let result = self.fetch_with_provider(&provider_id, request).await;
        
        let bars = if self.config.fallback_enabled {
            match result {
                Ok(bars) => bars,
                Err(e) => {
                    warn!("Primary provider failed: {}, trying fallback", e);
                    self.try_fallback(request).await?
                }
            }
        } else {
            result?
        };
        
        // Validate data
        let validated = self.validate_data(&bars)?;
        
        // Cache the result
        self.cache_data(&cache_key, &validated).await;
        
        self.record_stats(start.elapsed().as_millis() as u64, false);
        Ok(validated)
    }
    
    /// Get OHLCV data for multiple symbols (batch)
    pub async fn get_ohlcv_batch(&self, requests: &[DataRequest]) -> Result<Vec<Vec<OHLCVBar>>> {
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            let bars = self.get_ohlcv(request).await?;
            results.push(bars);
        }
        Ok(results)
    }
    
    /// Get normalized data for Kronos
    pub async fn get_kronos_data(&self, request: &DataRequest) -> Result<crate::types::KronosOHLCV> {
        let bars = self.get_ohlcv(request).await?;
        let _normalized = normalize_for_kronos(&bars, request.timeframe)?;
        
        Ok(crate::types::KronosOHLCV {
            symbol: request.symbol.clone(),
            exchange: request.exchange,
            timeframe: request.timeframe,
            bars: bars.clone(),
            metadata: crate::types::DataMetadata {
                provider: ProviderId::Yahoo,
                fetched_at: chrono::Utc::now(),
                first_timestamp: bars.first().map(|b| b.timestamp).unwrap_or_default(),
                last_timestamp: bars.last().map(|b| b.timestamp).unwrap_or_default(),
                bar_count: bars.len(),
                gaps_detected: 0,
                data_quality_score: 1.0,
            },
        })
    }
    
    async fn fetch_with_provider(&self, provider_id: &ProviderId, request: &DataRequest) -> Result<Vec<OHLCVBar>> {
        let registry = self.provider_registry.read().await;
        let provider = registry.get(*provider_id)
            .ok_or_else(|| crate::error::MarketDataError::ProviderNotFound(provider_id.as_str().to_string()))?;
        
        provider.fetch_ohlcv(request).await
    }
    
    async fn try_fallback(&self, request: &DataRequest) -> Result<Vec<OHLCVBar>> {
        let providers = self.config.get_enabled_providers();
        for provider_config in providers {
            if provider_config.id == self.config.default_provider {
                continue;
            }
            
            if let Ok(bars) = self.fetch_from_provider_config(&provider_config, request).await {
                return Ok(bars);
            }
        }
        
        Err(crate::error::MarketDataError::ProviderUnavailable("All providers failed".to_string()))
    }
    
    async fn fetch_from_provider_config(&self, config: &crate::types::ProviderConfig, request: &DataRequest) -> Result<Vec<OHLCVBar>> {
        match config.id {
            crate::types::ProviderId::Yahoo => {
                let provider = crate::providers::yahoo::YahooProvider::new(config.clone());
                provider.fetch_ohlcv(request).await
            }
            crate::types::ProviderId::Binance => {
                let provider = crate::providers::binance::BinanceProvider::new(config.clone());
                provider.fetch_ohlcv(request).await
            }
            _ => Err(crate::error::MarketDataError::ProviderUnavailable("Provider not implemented".to_string())),
        }
    }
    
    fn cache_key(&self, request: &DataRequest) -> String {
        let exchange_u8 = match request.exchange {
            Exchange::Nyse => 0,
            Exchange::Nasdaq => 1,
            Exchange::Binance => 2,
            Exchange::BinanceUs => 3,
            Exchange::Bybit => 4,
            Exchange::Okx => 5,
            Exchange::Coinbase => 6,
            Exchange::Kraken => 7,
            Exchange::Polygon => 8,
            Exchange::Yahoo => 9,
            Exchange::Custom(_) => 10,
        };
        format!("{}:{}:{}:{}:{}", 
            request.symbol, 
            exchange_u8, 
            request.timeframe as u8,
            request.start_time.map(|t| t.timestamp()).unwrap_or(0),
            request.end_time.map(|t| t.timestamp()).unwrap_or(0)
        )
    }
    
    async fn get_cached(&self, key: &str) -> Option<Vec<OHLCVBar>> {
        let cache = self.cache.read().await;
        cache.get(key).cloned()
    }
    
    async fn cache_data(&self, key: &str, data: &[OHLCVBar]) {
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), data.to_vec());
    }
    
    fn validate_data(&self, bars: &[OHLCVBar]) -> Result<Vec<OHLCVBar>> {
        Ok(bars.to_vec())
    }
    
    fn record_stats(&self, latency_ms: u64, cache_hit: bool) {
        let stats = self.stats.clone();
        tokio::spawn(async move {
            let mut stats = stats.lock().await;
            stats.total_requests += 1;
            stats.total_latency_ms += latency_ms as u64;
            stats.avg_latency_ms = stats.total_latency_ms as f64 / stats.total_requests as f64;
            if cache_hit {
                stats.cache_hits += 1;
            } else {
                stats.cache_misses += 1;
            }
        });
    }
    
    pub async fn get_stats(&self) -> EngineStats {
        self.stats.lock().await.clone()
    }
    
    pub async fn health_check(&self) -> Result<Vec<crate::types::ProviderHealth>> {
        let mut results = Vec::new();
        let registry = self.provider_registry.read().await;
        for provider_id in registry.list() {
            match self.fetch_provider_health(provider_id).await {
                Ok(health) => results.push(health),
                Err(e) => results.push(crate::types::ProviderHealth {
                    provider: provider_id,
                    healthy: false,
                    last_check: chrono::Utc::now(),
                    latency_ms: 0,
                    error_rate: 1.0,
                    last_error: Some(e.to_string()),
                }),
            }
        }
        Ok(results)
    }
    
    async fn fetch_provider_health(&self, provider_id: ProviderId) -> Result<crate::types::ProviderHealth> {
        let registry = self.provider_registry.read().await;
        if let Some(provider) = registry.get(provider_id) {
            provider.health_check().await
        } else {
            Err(crate::error::MarketDataError::ProviderNotFound(provider_id.as_str().to_string()))
        }
    }
}

#[async_trait::async_trait]
impl MarketDataEngine for MarketDataEngineImpl {
    async fn get_ohlcv(&self, request: &DataRequest) -> Result<Vec<OHLCVBar>> {
        self.get_ohlcv(request).await
    }

    async fn get_ohlcv_batch(&self, requests: &[DataRequest]) -> Result<Vec<Vec<OHLCVBar>>> {
        self.get_ohlcv_batch(requests).await
    }

    async fn get_kronos_data(&self, request: &DataRequest) -> Result<KronosOHLCV> {
        self.get_kronos_data(request).await
    }

    async fn health_check(&self) -> Result<Vec<ProviderHealth>> {
        self.health_check().await
    }

    async fn get_stats(&self) -> EngineStats {
        self.get_stats().await
    }
}