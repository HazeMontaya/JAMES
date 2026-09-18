//! Research Orchestrator

use crate::error::Result;
use crate::config::ResearchConfig;
use crate::indicators::TechnicalIndicators;
use crate::sentiment::{NewsSentimentEngine, SentimentAnalysis};
use crate::fundamentals::{FundamentalsEngine, FundamentalData};
use crate::synthesis::{ResearchSynthesizer, TechnicalAnalysis, ForecastResult, ResearchReport, Recommendation, RiskLevel};
use james_adapter_kronos::{KronosAdapter, ForecastRequest, Symbol as AdapterSymbol, Exchange as AdapterExchange, Timeframe as AdapterTimeframe, Symbol};
use james_market_data::{MarketDataEngine, types::DataRequest, types::Exchange as MarketExchange, types::Timeframe as MarketTimeframe};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// Research orchestrator that combines all analysis components
pub struct ResearchOrchestrator {
    config: ResearchConfig,
    indicators: TechnicalIndicators,
    sentiment_engine: NewsSentimentEngine,
    fundamentals_engine: FundamentalsEngine,
    kronos_adapter: Box<dyn KronosAdapter + Send + Sync>,
    market_data: Box<dyn james_market_data::MarketDataEngine>,
}

impl ResearchOrchestrator {
    pub fn new(
        config: ResearchConfig,
        kronos_adapter: Box<dyn KronosAdapter + Send + Sync>,
        market_data: Box<dyn james_market_data::MarketDataEngine>,
    ) -> Self {
        let sentiment_engine = NewsSentimentEngine::new(config.news_sources.clone());
        let fundamentals_engine = FundamentalsEngine::new(config.fundamentals_providers.clone());

        Self {
            config,
            indicators: TechnicalIndicators,
            sentiment_engine,
            fundamentals_engine,
            kronos_adapter,
            market_data,
        }
    }

    /// Convert james_adapter_kronos Timeframe to james_market_data Timeframe
    fn convert_timeframe(tf: AdapterTimeframe) -> MarketTimeframe {
        match tf {
            AdapterTimeframe::M1 => MarketTimeframe::M1,
            AdapterTimeframe::M5 => MarketTimeframe::M5,
            AdapterTimeframe::M15 => MarketTimeframe::M15,
            AdapterTimeframe::M30 => MarketTimeframe::M30,
            AdapterTimeframe::H1 => MarketTimeframe::H1,
            AdapterTimeframe::H4 => MarketTimeframe::H4,
            AdapterTimeframe::D1 => MarketTimeframe::D1,
            AdapterTimeframe::W1 => MarketTimeframe::W1,
        }
    }

    /// Convert james_adapter_kronos Symbol to james_market_data Symbol
    fn convert_symbol(s: &Symbol) -> james_market_data::types::Symbol {
        james_market_data::types::Symbol::new(&s.0)
    }

    /// Convert james_adapter_kronos Exchange to james_market_data Exchange
    fn convert_exchange(e: AdapterExchange) -> MarketExchange {
        match e {
            AdapterExchange::Nyse => MarketExchange::Nyse,
            AdapterExchange::Nasdaq => MarketExchange::Nasdaq,
            AdapterExchange::Binance => MarketExchange::Binance,
            AdapterExchange::Bybit => MarketExchange::Bybit,
            AdapterExchange::Okx => MarketExchange::Okx,
            AdapterExchange::Coinbase => MarketExchange::Coinbase,
            AdapterExchange::Polygon => MarketExchange::Polygon,
            AdapterExchange::Yahoo => MarketExchange::Yahoo,
            AdapterExchange::Custom(id) => MarketExchange::Custom(id),
        }
    }

    /// Run comprehensive research analysis for a symbol
    pub async fn research(&self, symbol: &Symbol) -> Result<ResearchReport> {
        let request = DataRequest {
            symbol: Self::convert_symbol(symbol),
            exchange: Self::convert_exchange(AdapterExchange::Yahoo),
            timeframe: Self::convert_timeframe(self.config.default_timeframe),
            start_time: None,
            end_time: None,
            limit: Some(self.config.lookback),
            lookback: Some(self.config.lookback),
        };

        let bars = self.market_data.get_ohlcv(&request).await?;
        
        if bars.is_empty() {
            return Err(crate::error::ResearchError::InsufficientData(
                format!("No data available for {}", symbol)
            ));
        }

        let (technical, kronos_forecast, sentiment, fundamentals) = tokio::join!(
            self.run_technical_analysis(&bars),
            self.run_kronos_forecast(symbol),
            self.run_sentiment_analysis(symbol),
            self.run_fundamentals_analysis(symbol),
        );

        let technical = technical?;
        let kronos_forecast = kronos_forecast?;
        let sentiment = sentiment?;
        let fundamentals = fundamentals?;

        let report = self.synthesize(symbol, bars, technical, kronos_forecast, sentiment, fundamentals).await?;
        
        Ok(report)
    }

    async fn run_technical_analysis(&self, bars: &[james_market_data::types::OHLCVBar]) -> Result<TechnicalAnalysis> {
        let closes: Vec<rust_decimal::Decimal> = bars.iter().map(|b| b.close).collect();
        let highs: Vec<rust_decimal::Decimal> = bars.iter().map(|b| b.high).collect();
        let lows: Vec<rust_decimal::Decimal> = bars.iter().map(|b| b.low).collect();

        let sma_20 = TechnicalIndicators::sma(&closes, 20);
        let sma_50 = TechnicalIndicators::sma(&closes, 50);
        let sma_200 = TechnicalIndicators::sma(&closes, 200);
        let ema_12 = TechnicalIndicators::ema(&closes, 12);
        let ema_26 = TechnicalIndicators::ema(&closes, 26);
        let rsi_14 = TechnicalIndicators::rsi(&closes, 14);
        let (macd_line, signal_line, histogram) = TechnicalIndicators::macd(&closes, 12, 26, 9);
        let (bb_mid, bb_upper, bb_lower) = TechnicalIndicators::bollinger_bands(&closes, 20, rust_decimal::Decimal::from(2));
        let atr_14 = TechnicalIndicators::atr(&highs, &lows, &closes, 14);
        let (stoch_k, stoch_d) = TechnicalIndicators::stochastic(&highs, &lows, &closes, 14, 3);

        Ok(TechnicalAnalysis {
            sma_20: sma_20.last().and_then(|x| *x),
            sma_50: sma_50.last().and_then(|x| *x),
            sma_200: sma_200.last().and_then(|x| *x),
            ema_12: ema_12.last().and_then(|x| *x),
            ema_26: ema_26.last().and_then(|x| *x),
            rsi_14: rsi_14.last().and_then(|x| *x),
            macd: macd_line.last().and_then(|x| *x),
            macd_signal: signal_line.last().and_then(|x| *x),
            macd_histogram: histogram.last().and_then(|x| *x),
            bb_upper: bb_upper.last().and_then(|x| *x),
            bb_middle: bb_mid.last().and_then(|x| *x),
            bb_lower: bb_lower.last().and_then(|x| *x),
            atr_14: atr_14.last().and_then(|x| *x),
            stoch_k: stoch_k.last().and_then(|x| *x),
            stoch_d: stoch_d.last().and_then(|x| *x),
        })
    }

    async fn run_kronos_forecast(&self, symbol: &Symbol) -> Result<ForecastResult> {
        let request = ForecastRequest {
            symbol: AdapterSymbol::new(symbol.0.clone()),
            exchange: AdapterExchange::Yahoo,
            timeframe: self.config.default_timeframe,
            lookback: self.config.lookback,
            pred_len: 120,
            timestamp: chrono::Utc::now(),
            sampling: Default::default(),
            include_volume: true,
            include_amount: false,
        };

        self.kronos_adapter.forecast(request).await.map_err(Into::into)
    }

    async fn run_sentiment_analysis(&self, symbol: &Symbol) -> Result<SentimentAnalysis> {
        self.sentiment_engine.analyze_sentiment(symbol, 7).await
    }

    async fn run_fundamentals_analysis(&self, symbol: &Symbol) -> Result<FundamentalData> {
        self.fundamentals_engine.fetch_fundamentals(symbol).await
    }

    async fn synthesize(
        &self,
        symbol: &Symbol,
        bars: Vec<james_market_data::types::OHLCVBar>,
        technical: TechnicalAnalysis,
        kronos_forecast: ForecastResult,
        sentiment: SentimentAnalysis,
        fundamentals: FundamentalData,
    ) -> Result<ResearchReport> {
        let synthesizer = ResearchSynthesizer::new();
        let current_price = bars.last().map(|b| b.close).unwrap_or_default();
        let price_change_24h = if bars.len() >= 2 {
            let prev = bars[bars.len() - 2].close;
            let curr = bars.last().unwrap().close;
            Some((curr - prev) / prev * rust_decimal::Decimal::from(100))
        } else {
            None
        };

        let report = ResearchSynthesizer::new().synthesize(
            symbol,
            current_price,
            price_change_24h,
            &technical,
            &kronos_forecast,
            &sentiment,
            &fundamentals,
        )?;

        Ok(ResearchReport {
            symbol: symbol.clone(),
            timestamp: chrono::Utc::now(),
            current_price,
            price_change_24h,
            technical,
            kronos_forecast,
            sentiment,
            fundamentals,
            thesis: String::new(),
            recommendation: Recommendation::Hold,
            risk_level: crate::synthesis::RiskLevel::Low,
            confidence: 0.0,
        })
    }
}