//! Binance provider implementation

use async_trait::async_trait;
use crate::providers::{MarketDataProvider, RateLimiter};
use crate::types::*;
use crate::error::Result;
use reqwest::Client;
use serde::Deserialize;
use chrono::{DateTime, Utc, TimeZone};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromStr; // For Decimal::from_str
use std::sync::Arc;

/// Binance API response structures - direct array of klines
type BinanceKline = (
    i64,      // Open time
    String,   // Open
    String,   // High
    String,   // Low
    String,   // Close
    String,   // Volume
    i64,      // Close time
    String,   // Quote asset volume
    i64,      // Number of trades
    String,   // Taker buy base asset volume
    String,   // Taker buy quote asset volume
    String,   // Ignore
);

/// Binance provider
pub struct BinanceProvider {
    client: reqwest::Client,
    rate_limiter: std::sync::Arc<RateLimiter>,
    config: super::ProviderConfig,
}

impl BinanceProvider {
    pub fn new(config: super::ProviderConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        let rate_limiter = std::sync::Arc::new(RateLimiter::new(&config.rate_limit));

        Self {
            client,
            rate_limiter: std::sync::Arc::new(RateLimiter::new(&config.rate_limit)),
            config,
        }
    }

    fn format_symbol(&self, symbol: &Symbol, exchange: &crate::types::Exchange) -> String {
        let sym = symbol.0.to_uppercase();
        // Binance uses format like BTCUSDT
        match exchange {
            crate::types::Exchange::Binance => {
                // Remove any separator and uppercase
                symbol.0.replace("/", "").replace("-", "").replace("_", "").to_uppercase()
            }
            _ => symbol.0.to_uppercase(),
        }
    }

    fn timeframe_to_binance(&self, tf: &crate::types::Timeframe) -> String {
        match tf {
            crate::types::Timeframe::M1 => "1m".to_string(),
            crate::types::Timeframe::M5 => "5m".to_string(),
            crate::types::Timeframe::M15 => "15m".to_string(),
            crate::types::Timeframe::M30 => "30m".to_string(),
            crate::types::Timeframe::H1 => "1h".to_string(),
            crate::types::Timeframe::H4 => "4h".to_string(),
            crate::types::Timeframe::D1 => "1d".to_string(),
            crate::types::Timeframe::W1 => "1w".to_string(),
            crate::types::Timeframe::M1Month => "1M".to_string(),
        }
    }

    fn build_url(&self, request: &crate::types::DataRequest) -> String {
        let symbol = self.format_symbol(&request.symbol, &request.exchange);
        let interval = self.timeframe_to_binance(&request.timeframe);
        
        let mut url = format!("https://api.binance.com/api/v3/klines?symbol={}&interval={}", symbol, interval);
        
        if let Some(start) = request.start_time {
            url.push_str(&format!("&startTime={}", start.timestamp_millis()));
        }
        if let Some(end) = request.end_time {
            url.push_str(&format!("&endTime={}", end.timestamp_millis()));
        }
        if let Some(limit) = request.limit {
            url.push_str(&format!("&limit={}", limit.min(1000)));
        }
        if let Some(lookback) = request.lookback {
            // Binance max limit is 1000
            let limit = request.lookback.unwrap_or(500).min(1000);
            url.push_str(&format!("&limit={}", limit));
        }
        
        url
    }

    async fn parse_response(&self, response: Vec<BinanceKline>, symbol: crate::types::Symbol, timeframe: crate::types::Timeframe) -> Result<Vec<OHLCVBar>> {
        let mut bars = Vec::new();
        
        for kline in response {
            let open_time = kline.0;
            let open = Decimal::from_str(&kline.1).unwrap_or_default();
            let high = Decimal::from_str(&kline.2).unwrap_or_default();
            let low = Decimal::from_str(&kline.3).unwrap_or_default();
            let close = Decimal::from_str(&kline.4).unwrap_or_default();
            let volume = Decimal::from_str(&kline.5).unwrap_or_default();
            let amount = Decimal::from_str(&kline.7).unwrap_or_default(); // Quote asset volume
            let trades = kline.8 as u64;
            
            let ts = chrono::Utc.timestamp_millis_opt(open_time).single()
                .ok_or_else(|| crate::error::MarketDataError::ParseError("Invalid timestamp".to_string()))?;

            bars.push(OHLCVBar {
                timestamp: ts,
                open,
                high,
                low,
                close,
                volume,
                amount: Some(amount),
                trades: Some(trades),
            });
        }
        
        Ok(bars)
    }
}

#[async_trait::async_trait]
impl super::MarketDataProvider for BinanceProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Binance
    }

    fn name(&self) -> &str {
        "Binance"
    }

    fn supported_exchanges(&self) -> &[Exchange] {
        &[Exchange::Binance]
    }

    fn supported_timeframes(&self) -> &[Timeframe] {
        &[
            Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::M30,
            Timeframe::H1, Timeframe::H4, Timeframe::D1, Timeframe::W1, Timeframe::M1Month,
        ]
    }

    fn rate_limit(&self) -> &RateLimit {
        &self.config.rate_limit
    }

    fn get_rate_limiter(&self) -> Option<&RateLimiter> {
        Some(&self.rate_limiter)
    }

    async fn fetch_ohlcv(&self, request: &crate::types::DataRequest) -> Result<Vec<crate::types::OHLCVBar>> {
        self.rate_limiter.acquire().await?;
        
        let url = self.build_url(request);
        
        let response = self.client.get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await?;

        let data: Vec<BinanceKline> = response.json().await?;
        self.parse_response(data, request.symbol.clone(), request.timeframe).await
    }

    async fn health_check(&self) -> Result<crate::types::ProviderHealth> {
        let start = std::time::Instant::now();
        
        let test_request = crate::types::DataRequest {
            symbol: Symbol::new("BTCUSDT"),
            exchange: Exchange::Binance,
            timeframe: Timeframe::D1,
            start_time: Some(chrono::Utc::now() - chrono::Duration::days(1)),
            end_time: Some(chrono::Utc::now()),
            limit: Some(1),
            lookback: None,
        };

        let result = self.fetch_ohlcv(&test_request).await;
        let latency = std::time::Instant::now().duration_since(std::time::Instant::now()).as_millis() as u64;

        match result {
            Ok(_) => Ok(crate::types::ProviderHealth {
                provider: ProviderId::Binance,
                healthy: true,
                last_check: chrono::Utc::now(),
                latency_ms: 0, // Would be calculated properly
                error_rate: 0.0,
                last_error: None,
            }),
            Err(e) => Ok(crate::types::ProviderHealth {
                provider: ProviderId::Binance,
                healthy: false,
                last_check: chrono::Utc::now(),
                latency_ms: 0,
                error_rate: 1.0,
                last_error: Some(e.to_string()),
            }),
        }
    }
}