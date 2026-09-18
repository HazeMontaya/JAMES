//! Configuration for Market Data Engine

use std::path::PathBuf;
use crate::types::*;

impl Default for MarketDataConfig {
    fn default() -> Self {
        Self {
            providers: vec![
                ProviderConfig {
                    id: ProviderId::Yahoo,
                    name: "Yahoo Finance".to_string(),
                    base_url: "https://query1.finance.yahoo.com".to_string(),
                    api_key: None,
                    rate_limit: RateLimit::default(),
                    enabled: true,
                    priority: 1,
                    supported_exchanges: vec![Exchange::Nyse, Exchange::Nasdaq, Exchange::Yahoo],
                    supported_timeframes: vec![
                        Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::M30,
                        Timeframe::H1, Timeframe::H4, Timeframe::D1, Timeframe::W1, Timeframe::M1Month,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::Binance,
                    name: "Binance".to_string(),
                    base_url: "https://api.binance.com".to_string(),
                    api_key: None,
                    rate_limit: RateLimit {
                        requests_per_second: 20,
                        requests_per_minute: 1200,
                        requests_per_hour: 50000,
                        requests_per_day: 1000000,
                    },
                    enabled: true,
                    priority: 2,
                    supported_exchanges: vec![Exchange::Binance],
                    supported_timeframes: vec![
                        Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::M30,
                        Timeframe::H1, Timeframe::H4, Timeframe::D1, Timeframe::W1, Timeframe::M1Month,
                    ],
                },
                ProviderConfig {
                    id: ProviderId::Polygon,
                    name: "Polygon.io".to_string(),
                    base_url: "https://api.polygon.io".to_string(),
                    api_key: None,
                    rate_limit: RateLimit {
                        requests_per_second: 5,
                        requests_per_minute: 100,
                        requests_per_hour: 1000,
                        requests_per_day: 5000,
                    },
                    enabled: false, // Requires API key
                    priority: 3,
                    supported_exchanges: vec![Exchange::Nyse, Exchange::Nasdaq, Exchange::Polygon],
                    supported_timeframes: vec![
                        Timeframe::M1, Timeframe::M5, Timeframe::M15, Timeframe::M30,
                        Timeframe::H1, Timeframe::H4, Timeframe::D1,
                    ],
                },
            ],
            cache_dir: "./cache/market_data".to_string(),
            default_provider: ProviderId::Yahoo,
            fallback_enabled: true,
            max_concurrent_requests: 10,
            request_timeout_secs: 30,
            validation: ValidationConfig {
                check_gaps: true,
                max_gap_threshold: 3600, // 1 hour
                check_outliers: true,
                outlier_threshold_std: 3.0,
                min_volume_threshold: None,
            },
        }
    }
}

impl MarketDataConfig {
    pub fn load_from_file(path: &std::path::Path) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save_to_file(&self, path: &std::path::Path) -> crate::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn get_provider(&self, id: ProviderId) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    pub fn get_enabled_providers(&self) -> Vec<&ProviderConfig> {
        self.providers.iter()
            .filter(|p| p.enabled)
            .collect()
    }

    pub fn get_providers_for_exchange(&self, exchange: Exchange) -> Vec<&ProviderConfig> {
        self.providers.iter()
            .filter(|p| p.enabled && p.supported_exchanges.contains(&exchange))
            .collect()
    }
}