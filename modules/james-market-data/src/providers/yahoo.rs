//! Yahoo Finance provider implementation

use async_trait::async_trait;
use crate::providers::{MarketDataProvider, RateLimiter};
use crate::types::*;
use crate::error::Result;
use reqwest::Client;
use serde::Deserialize;
use chrono::{DateTime, Utc, TimeZone};
use rust_decimal::Decimal;
use std::sync::Arc;
use crate::types::RateLimit;

/// Yahoo Finance API response structures
#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooResult>>,
    error: Option<YahooError>,
}

#[derive(Debug, Deserialize)]
struct YahooResult {
    meta: YahooMeta,
    timestamp: Vec<i64>,
    indicators: YahooIndicators,
}

#[derive(Debug, Deserialize)]
struct YahooMeta {
    symbol: String,
    #[serde(rename = "exchangeName")]
    exchange_name: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "currency")]
    currency: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    open: Option<Vec<Option<f64>>>,
    high: Option<Vec<Option<f64>>>,
    low: Option<Vec<Option<f64>>>,
    close: Option<Vec<Option<f64>>>,
    volume: Option<Vec<Option<f64>>>,
}

#[derive(Debug, Deserialize)]
struct YahooError {
    code: String,
    description: String,
}

/// Yahoo Finance provider
pub struct YahooProvider {
    client: Client,
    rate_limiter: Arc<RateLimiter>,
    config: super::ProviderConfig,
}

impl YahooProvider {
    pub fn new(config: super::ProviderConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        let rate_limiter = Arc::new(RateLimiter::new(&config.rate_limit));

        Self {
            client,
            rate_limiter,
            config,
        }
    }

    fn build_url(&self, request: &crate::types::DataRequest) -> String {
        let symbol = self.format_symbol(&request.symbol, &request.exchange);
        let interval = self.timeframe_to_yahoo(&request.timeframe);
        
        let mut url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}", symbol);
        
        if let Some(start) = request.start_time {
            url.push_str(&format!("&period1={}", start.timestamp()));
        }
        if let Some(end) = request.end_time {
            url.push_str(&format!("&period2={}", end.timestamp()));
        }
        if let Some(limit) = request.limit {
            url.push_str(&format!("&limit={}", limit));
        }
        if let Some(lookback) = request.lookback {
            // Calculate period1 based on lookback
            let interval_secs = request.timeframe.to_seconds();
            let start = chrono::Utc::now() - chrono::Duration::seconds((lookback as i64) * request.timeframe.to_seconds() as i64);
            url.push_str(&format!("&period1={}", start.timestamp()));
        }
        
        url.push_str(&format!("&interval={}", interval));
        url.push_str("&includePrePost=false&events=div%7Csplit");
        
        url
    }

    fn format_symbol(&self, symbol: &Symbol, exchange: &Exchange) -> String {
        let sym = symbol.0.to_uppercase();
        match exchange {
            Exchange::Nyse => format!("{}", sym),
            Exchange::Nasdaq => format!("{}", sym),
            Exchange::Yahoo => format!("{}", sym),
            _ => format!("{}", sym),
        }
    }

    fn timeframe_to_yahoo(&self, tf: &Timeframe) -> String {
        match tf {
            Timeframe::M1 => "1m".to_string(),
            Timeframe::M5 => "5m".to_string(),
            Timeframe::M15 => "15m".to_string(),
            Timeframe::M30 => "30m".to_string(),
            Timeframe::H1 => "1h".to_string(),
            Timeframe::H4 => "4h".to_string(),
            Timeframe::D1 => "1d".to_string(),
            Timeframe::W1 => "1wk".to_string(),
            Timeframe::M1Month => "1mo".to_string(),
        }
    }

    async fn parse_response(&self, response: YahooChartResponse, symbol: Symbol, timeframe: Timeframe) -> Result<Vec<OHLCVBar>> {
        let result = response.chart.result
            .ok_or_else(|| crate::error::MarketDataError::ApiError("No data returned".to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| crate::error::MarketDataError::DataNotFound("No results".to_string()))?;

        let timestamps = result.timestamp;
        let quote = result.indicators.quote.into_iter().next()
            .ok_or_else(|| crate::error::MarketDataError::ParseError("No quote data".to_string()))?;

        let opens = quote.open.unwrap_or_default();
        let highs = quote.high.unwrap_or_default();
        let lows = quote.low.unwrap_or_default();
        let closes = quote.close.unwrap_or_default();
        let volumes = quote.volume.unwrap_or_default();

        let mut bars = Vec::new();
        for i in 0..timestamps.len() {
            let ts = Utc.timestamp_opt(timestamps[i], 0).single()
                .ok_or_else(|| crate::error::MarketDataError::ParseError("Invalid timestamp".to_string()))?;

            let open = opens.get(i).and_then(|v| *v).ok_or_else(|| crate::error::MarketDataError::ParseError("Missing open".to_string()))?;
            let high = highs.get(i).and_then(|v| *v).ok_or_else(|| crate::error::MarketDataError::ParseError("Missing high".to_string()))?;
            let low = lows.get(i).and_then(|v| *v).ok_or_else(|| crate::error::MarketDataError::ParseError("Missing low".to_string()))?;
            let close = closes.get(i).and_then(|v| *v).ok_or_else(|| crate::error::MarketDataError::ParseError("Missing close".to_string()))?;
            let volume = volumes.get(i).and_then(|v| *v).unwrap_or(0.0);

            bars.push(OHLCVBar {
                timestamp: Utc.timestamp_opt(timestamps[i], 0).single().unwrap(),
                open: Decimal::from_f64_retain(open).unwrap_or_default(),
                high: Decimal::from_f64_retain(high).unwrap_or_default(),
                low: Decimal::from_f64_retain(low).unwrap_or_default(),
                close: Decimal::from_f64_retain(close).unwrap_or_default(),
                volume: Decimal::from_f64_retain(volume).unwrap_or_default(),
                amount: None,
                trades: None,
            });
        }

        Ok(bars)
    }
}

#[async_trait::async_trait]
impl super::MarketDataProvider for YahooProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Yahoo
    }

    fn name(&self) -> &str {
        "Yahoo Finance"
    }

    fn supported_exchanges(&self) -> &[Exchange] {
        &[Exchange::Nyse, Exchange::Nasdaq, Exchange::Yahoo]
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

        let text = response.text().await?;
        let response: YahooChartResponse = serde_json::from_str(&text)?;
        
        // Check for API error
        if let Some(error) = response.chart.error {
            return Err(crate::error::MarketDataError::ApiError(format!("{}: {}", error.code, error.description)));
        }

        let symbol = request.symbol.clone();
        let timeframe = request.timeframe;
        self.parse_response(response, symbol, timeframe).await
    }

    async fn health_check(&self) -> Result<crate::types::ProviderHealth> {
        let start = std::time::Instant::now();
        
        // Try to fetch a simple symbol
        let test_request = crate::types::DataRequest {
            symbol: Symbol::new("AAPL"),
            exchange: Exchange::Nasdaq,
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
                provider: ProviderId::Yahoo,
                healthy: true,
                last_check: chrono::Utc::now(),
                latency_ms: latency,
                error_rate: 0.0,
                last_error: None,
            }),
            Err(e) => Ok(crate::types::ProviderHealth {
                provider: ProviderId::Yahoo,
                healthy: false,
                last_check: chrono::Utc::now(),
                latency_ms: latency,
                error_rate: 1.0,
                last_error: Some(e.to_string()),
            }),
        }
    }
}