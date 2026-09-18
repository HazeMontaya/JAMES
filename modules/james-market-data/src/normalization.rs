//! Data normalization for Kronos compatibility

use crate::types::*;
use crate::error::Result;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromStr;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Duration};
use std::collections::HashMap;

/// Normalize OHLCV data for Kronos input format
pub fn normalize_for_kronos(bars: &[OHLCVBar], timeframe: Timeframe) -> Result<Vec<OHLCVBar>> {
    if bars.is_empty() {
        return Ok(Vec::new());
    }
    
    let mut normalized: Vec<OHLCVBar> = Vec::with_capacity(bars.len());
    let expected_interval = timeframe.to_seconds();
    
    for (i, bar) in bars.iter().enumerate() {
        let mut normalized_bar = bar.clone();
        
        // Ensure timestamp aligns with timeframe
        let expected_ts = if i == 0 {
            bar.timestamp
        } else {
            let prev_ts = normalized[i - 1].timestamp;
            prev_ts + Duration::seconds(expected_interval as i64)
        };
        
        // Adjust timestamp if it's significantly off
        let diff = (bar.timestamp - expected_ts).num_seconds().abs();
        if diff > expected_interval as i64 / 2 {
            normalized_bar.timestamp = expected_ts;
        }
        
        // Ensure OHLC consistency (high >= max(open, close), low <= min(open, close))
        let high = normalized_bar.high.max(normalized_bar.open.max(normalized_bar.close));
        let low = normalized_bar.low.min(normalized_bar.open.min(normalized_bar.close));
        
        normalized_bar.high = high;
        normalized_bar.low = low;
        
        // Ensure volume is non-negative
        if normalized_bar.volume < Decimal::ZERO {
            normalized_bar.volume = Decimal::ZERO;
        }
        
        // Calculate amount if missing (close * volume)
        if normalized_bar.amount.is_none() || normalized_bar.amount.unwrap() == Decimal::ZERO {
            normalized_bar.amount = Some(normalized_bar.close * normalized_bar.volume);
        }
        
        normalized.push(normalized_bar);
    }
    
    Ok(normalized)
}

/// Data validator for quality checks
pub struct DataValidator {
    max_gap_threshold: u64,
    outlier_threshold_std: f64,
    min_volume_threshold: Option<Decimal>,
}

impl DataValidator {
    pub fn new() -> Self {
        Self {
            max_gap_threshold: 3600,
            outlier_threshold_std: 3.0,
            min_volume_threshold: None,
        }
    }
    
    pub fn with_gap_threshold(mut self, threshold: u64) -> Self {
        self.max_gap_threshold = threshold;
        self
    }
    
    pub fn with_outlier_threshold(mut self, threshold: f64) -> Self {
        self.outlier_threshold_std = threshold;
        self
    }
    
    pub fn with_min_volume(mut self, threshold: Decimal) -> Self {
        self.min_volume_threshold = Some(threshold);
        self
    }
    
    pub fn validate(&self, bars: &[OHLCVBar]) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        
        if bars.is_empty() {
            issues.push(ValidationIssue::EmptyData);
            return issues;
        }
        
        // Check for gaps
        // ... (simplified for now)
        
        issues
    }
}

/// Validation issue types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationIssue {
    EmptyData,
    Gap {
        expected: DateTime<Utc>,
        actual: DateTime<Utc>,
        gap_seconds: i64,
    },
    Outlier {
        index: usize,
        value: f64,
        mean: f64,
        std_dev: f64,
        threshold: f64,
    },
    LowVolume {
        index: usize,
        volume: Decimal,
        threshold: Decimal,
    },
    OHLCInconsistency {
        index: usize,
        high: Decimal,
        low: Decimal,
        open: Decimal,
        close: Decimal,
    },
}

/// Gap detector and filler
pub struct GapDetector {
    max_gap_threshold: u64,
}

impl GapDetector {
    pub fn new() -> Self {
        Self {
            max_gap_threshold: 3600,
        }
    }
    
    pub fn with_threshold(mut self, threshold: u64) -> Self {
        self.max_gap_threshold = threshold;
        self
    }
    
    pub fn fill_gaps(&self, bars: Vec<OHLCVBar>, max_gap: u64) -> Result<Vec<OHLCVBar>> {
        if bars.len() < 2 {
            return Ok(bars);
        }
        
        let expected_interval = if bars.len() >= 2 {
            (bars[1].timestamp - bars[0].timestamp).num_seconds()
        } else {
            60
        };
        
        let max_gap_duration = Duration::seconds(max_gap as i64);
        let mut result = Vec::with_capacity(bars.len() * 2);
        
        for i in 0..bars.len() {
            result.push(bars[i].clone());
            
            if i + 1 < bars.len() {
                let gap = bars[i + 1].timestamp - bars[i].timestamp;
                let gap_seconds = gap.num_seconds();
                
                if gap > Duration::seconds(1) && gap <= Duration::seconds(max_gap as i64) {
                    // Fill the gap with interpolated bars
                    let num_missing = (gap.num_seconds() / 60) as usize;
                    for j in 1..num_missing {
                        let interp_ts = bars[i].timestamp + Duration::seconds((j * 60) as i64);
                        let interp_ratio = j as f64 / num_missing as f64;
                        
                        let open = interpolate_decimal(bars[i].open, bars[i + 1].open, interp_ratio);
                        let high = interpolate_decimal(bars[i].high, bars[i + 1].high, interp_ratio);
                        let low = interpolate_decimal(bars[i].low, bars[i + 1].low, interp_ratio);
                        let close = interpolate_decimal(bars[i].close, bars[i + 1].close, interp_ratio);
                        let volume = interpolate_decimal(bars[i].volume, bars[i + 1].volume, interp_ratio);
                        
                        result.push(OHLCVBar {
                            timestamp: bars[i].timestamp + Duration::seconds((j * 60) as i64),
                            open,
                            high,
                            low,
                            close,
                            volume,
                            amount: None,
                            trades: None,
                        });
                    }
                }
            }
        }
        
        Ok(result)
    }
}

fn interpolate_decimal(a: Decimal, b: Decimal, ratio: f64) -> Decimal {
    let a_f = a.to_f64().unwrap_or(0.0);
    let b_f = b.to_f64().unwrap_or(0.0);
    let interp = a_f + (b_f - a_f) * ratio;
    Decimal::from_f64_retain(interp).unwrap_or_default()
}

/// Outlier detector
pub struct OutlierDetector {
    threshold_std: f64,
}

impl OutlierDetector {
    pub fn new() -> Self {
        Self {
            threshold_std: 3.0,
        }
    }
    
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold_std = threshold;
        self
    }
    
    pub fn remove_outliers(&self, bars: Vec<OHLCVBar>, _threshold_std: f64) -> Result<Vec<OHLCVBar>> {
        if bars.len() < 3 {
            return Ok(bars);
        }
        
        // Calculate returns
        let mut returns = Vec::new();
        for i in 1..bars.len() {
            let prev_close = bars[i - 1].close;
            let curr_close = bars[i].close;
            if prev_close != Decimal::ZERO {
                let ret = (curr_close - prev_close) / prev_close;
                returns.push(ret.to_f64().unwrap_or(0.0));
            } else {
                returns.push(0.0);
            }
        }
        
        // Calculate mean and std
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let std_dev = variance.sqrt();
        
        // Filter outliers
        let mut result = Vec::new();
        result.push(bars[0].clone());
        
        for i in 1..bars.len() {
            let ret = (bars[i].close - bars[i - 1].close) / bars[i - 1].close;
            let ret_f = ret.to_f64().unwrap_or(0.0);
            
            let mean = returns.iter().sum::<f64>() / returns.len() as f64;
            let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
            let std_dev = variance.sqrt();
            
            if (ret_f - mean).abs() <= 3.0 * std_dev {
                result.push(bars[i].clone());
            }
        }
        
        Ok(result)
    }
}