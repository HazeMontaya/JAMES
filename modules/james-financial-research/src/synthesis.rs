//! Research Synthesis Engine

use james_adapter_kronos::Symbol;
use crate::error::Result;
use crate::sentiment::SentimentAnalysis;
use crate::fundamentals::FundamentalData;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

// Re-export ForecastResult for use by orchestrator
pub use james_adapter_kronos::ForecastResult;

/// Research synthesis engine
pub struct ResearchSynthesizer;

impl ResearchSynthesizer {
    pub fn new() -> Self {
        Self
    }

    pub fn synthesize(
        &self,
        symbol: &Symbol,
        current_price: Decimal,
        price_change_24h: Option<Decimal>,
        technical: &TechnicalAnalysis,
        kronos_forecast: &ForecastResult,
        sentiment: &SentimentAnalysis,
        fundamentals: &FundamentalData,
    ) -> Result<ResearchReport> {
        let thesis = self.generate_thesis(technical, sentiment, fundamentals);
        let recommendation = self.determine_recommendation(technical, sentiment, fundamentals);
        let risk_level = self.assess_risk(technical, fundamentals);
        let confidence = self.calculate_confidence(technical, sentiment, fundamentals);

        Ok(ResearchReport {
            symbol: symbol.clone(),
            timestamp: chrono::Utc::now(),
            current_price,
            price_change_24h,
            technical: technical.clone(),
            kronos_forecast: kronos_forecast.clone(),
            sentiment: sentiment.clone(),
            fundamentals: fundamentals.clone(),
            thesis,
            recommendation,
            risk_level,
            confidence,
        })
    }

    fn generate_thesis(
        &self,
        technical: &TechnicalAnalysis,
        sentiment: &SentimentAnalysis,
        fundamentals: &FundamentalData,
    ) -> String {
        let mut parts = Vec::new();

        if let (Some(rsi), Some(sma_20), Some(sma_50)) = (technical.rsi_14, technical.sma_20, technical.sma_50) {
            if rsi > Decimal::from(70) {
                parts.push("RSI indicates overbought conditions".to_string());
            } else if rsi < Decimal::from(30) {
                parts.push("RSI indicates oversold conditions".to_string());
            }

            if sma_20 > sma_50 {
                parts.push("Short-term trend is bullish (SMA20 > SMA50)".to_string());
            } else {
                parts.push("Short-term trend is bearish (SMA20 < SMA50)".to_string());
            }
        }

        if sentiment.score > Decimal::ZERO {
            parts.push("News sentiment is positive".to_string());
        }

        if let Some(pe) = fundamentals.pe_ratio {
            if pe < Decimal::from(15) {
                parts.push("Valuation appears attractive (P/E < 15)".to_string());
            } else if pe > Decimal::from(30) {
                parts.push("Valuation appears stretched (P/E > 30)".to_string());
            }
        }

        if parts.is_empty() {
            "Insufficient data for definitive thesis".to_string()
        } else {
            parts.join(". ")
        }
    }

    fn determine_recommendation(
        &self,
        technical: &TechnicalAnalysis,
        sentiment: &SentimentAnalysis,
        fundamentals: &FundamentalData,
    ) -> Recommendation {
        let mut score = 0;

        if let (Some(rsi), Some(sma_20), Some(sma_50)) = (technical.rsi_14, technical.sma_20, technical.sma_50) {
            if rsi < Decimal::from(30) && sma_20 > sma_50 {
                score += 2;
            } else if rsi > Decimal::from(70) && sma_20 < sma_50 {
                score -= 2;
            } else if sma_20 > sma_50 {
                score += 1;
            } else {
                score -= 1;
            }
        }

        if sentiment.score > Decimal::ZERO {
            score += 1;
        } else if sentiment.score < Decimal::ZERO {
            score -= 1;
        }

        if let Some(pe) = fundamentals.pe_ratio {
            if pe < Decimal::from(15) {
                score += 1;
            } else if pe > Decimal::from(30) {
                score -= 1;
            }
        }

        match score {
            3.. => Recommendation::StrongBuy,
            1..=2 => Recommendation::Buy,
            0 => Recommendation::Hold,
            -2..=-1 => Recommendation::Sell,
            _ => Recommendation::StrongSell,
        }
    }

    fn assess_risk(
        &self,
        technical: &TechnicalAnalysis,
        fundamentals: &FundamentalData,
    ) -> RiskLevel {
        let mut risk_score = 0;

        if let Some(atr) = technical.atr_14 {
            if atr > Decimal::from(5) {
                risk_score += 2;
            } else if atr > Decimal::from(2) {
                risk_score += 1;
            }
        }

        if let Some(debt_equity) = fundamentals.debt_to_equity {
            if debt_equity > Decimal::from(2) {
                risk_score += 2;
            } else if debt_equity > Decimal::from(1) {
                risk_score += 1;
            }
        }

        match risk_score {
            0 => RiskLevel::Low,
            1..=2 => RiskLevel::Medium,
            3..=4 => RiskLevel::High,
            _ => RiskLevel::Critical,
        }
    }

    fn calculate_confidence(
        &self,
        technical: &TechnicalAnalysis,
        sentiment: &SentimentAnalysis,
        fundamentals: &FundamentalData,
    ) -> f64 {
        let mut confidence = 0.5;

        let tech_fields = [
            technical.rsi_14.is_some(),
            technical.sma_20.is_some(),
            technical.sma_50.is_some(),
            technical.macd.is_some(),
        ].iter().filter(|&&b| b).count();
        confidence += tech_fields as f64 * 0.1;

        confidence += sentiment.confidence * 0.2;

        let fund_fields = [
            fundamentals.pe_ratio.is_some(),
            fundamentals.market_cap.is_some(),
            fundamentals.profit_margin.is_some(),
            fundamentals.debt_to_equity.is_some(),
        ].iter().filter(|&&b| b).count();
        confidence += fund_fields as f64 * 0.05;

        confidence.min(1.0)
    }
}

/// Technical analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalAnalysis {
    pub sma_20: Option<rust_decimal::Decimal>,
    pub sma_50: Option<rust_decimal::Decimal>,
    pub sma_200: Option<rust_decimal::Decimal>,
    pub ema_12: Option<rust_decimal::Decimal>,
    pub ema_26: Option<rust_decimal::Decimal>,
    pub rsi_14: Option<rust_decimal::Decimal>,
    pub macd: Option<rust_decimal::Decimal>,
    pub macd_signal: Option<rust_decimal::Decimal>,
    pub macd_histogram: Option<rust_decimal::Decimal>,
    pub bb_upper: Option<rust_decimal::Decimal>,
    pub bb_middle: Option<rust_decimal::Decimal>,
    pub bb_lower: Option<rust_decimal::Decimal>,
    pub atr_14: Option<rust_decimal::Decimal>,
    pub stoch_k: Option<rust_decimal::Decimal>,
    pub stoch_d: Option<rust_decimal::Decimal>,
}

/// Research recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Recommendation {
    StrongBuy,
    Buy,
    Hold,
    Sell,
    StrongSell,
}

/// Risk level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Complete research report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchReport {
    pub symbol: Symbol,
    pub timestamp: DateTime<Utc>,
    pub current_price: rust_decimal::Decimal,
    pub price_change_24h: Option<rust_decimal::Decimal>,
    pub technical: TechnicalAnalysis,
    pub kronos_forecast: ForecastResult,
    pub sentiment: SentimentAnalysis,
    pub fundamentals: FundamentalData,
    pub thesis: String,
    pub recommendation: Recommendation,
    pub risk_level: RiskLevel,
    pub confidence: f64,
}