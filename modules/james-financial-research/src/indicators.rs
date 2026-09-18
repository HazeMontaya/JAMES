//! Technical Indicators Library

use crate::error::Result;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::VecDeque;

/// Technical indicators calculator
pub struct TechnicalIndicators;

impl TechnicalIndicators {
    /// Simple Moving Average
    pub fn sma(prices: &[Decimal], period: usize) -> Vec<Option<Decimal>> {
        let mut result = Vec::with_capacity(prices.len());
        let mut sum = Decimal::ZERO;
        let mut window = VecDeque::new();

        for price in prices {
            window.push_back(*price);
            sum += *price;

            if window.len() > period {
                if let Some(old) = window.pop_front() {
                    sum -= old;
                }
            }

            if window.len() == period {
                result.push(Some(sum / Decimal::from(period)));
            } else {
                result.push(None);
            }
        }

        result
    }

    /// Exponential Moving Average
    pub fn ema(prices: &[Decimal], period: usize) -> Vec<Option<Decimal>> {
        if prices.is_empty() || period == 0 {
            return vec![None; prices.len()];
        }

        let mut result = Vec::with_capacity(prices.len());
        let alpha = Decimal::from(2) / Decimal::from(period + 1);
        let mut ema = None;

        for price in prices {
            ema = match ema {
                Some(prev) => Some(alpha * *price + (Decimal::ONE - alpha) * prev),
                None => Some(*price),
            };
            result.push(ema);
        }

        result
    }

    /// Relative Strength Index (RSI)
    pub fn rsi(prices: &[Decimal], period: usize) -> Vec<Option<Decimal>> {
        if prices.len() < period + 1 {
            return vec![None; prices.len()];
        }

        let mut gains = VecDeque::with_capacity(period);
        let mut losses = VecDeque::with_capacity(period);
        let mut result = Vec::with_capacity(prices.len());

        result.push(None);

        for i in 1..prices.len() {
            let change = prices[i] - prices[i - 1];
            
            if change >= Decimal::ZERO {
                gains.push_back(change);
                losses.push_back(Decimal::ZERO);
            } else {
                gains.push_back(Decimal::ZERO);
                losses.push_back(-change);
            }

            if gains.len() > period {
                gains.pop_front();
                losses.pop_front();
            }

            if gains.len() == period {
                let avg_gain: Decimal = gains.iter().sum::<Decimal>() / Decimal::from(period);
                let avg_loss: Decimal = losses.iter().sum::<Decimal>() / Decimal::from(period);

                let rsi = if avg_loss == Decimal::ZERO {
                    Decimal::from(100)
                } else {
                    let rs = avg_gain / avg_loss;
                    Decimal::from(100) - (Decimal::from(100) / (Decimal::ONE + rs))
                };
                result.push(Some(rsi));
            } else {
                result.push(None);
            }
        }

        result
    }

    /// Moving Average Convergence Divergence (MACD)
    pub fn macd(prices: &[Decimal], fast_period: usize, slow_period: usize, signal_period: usize) -> (Vec<Option<Decimal>>, Vec<Option<Decimal>>, Vec<Option<Decimal>>) {
        let fast_ema = Self::ema(prices, fast_period);
        let slow_ema = Self::ema(prices, slow_period);

        let mut macd = Vec::with_capacity(prices.len());
        let mut signal = Vec::with_capacity(prices.len());
        let mut histogram = Vec::with_capacity(prices.len());

        for (fast, slow) in fast_ema.iter().zip(slow_ema.iter()) {
            match (fast, slow) {
                (Some(f), Some(s)) => {
                    let macd_val = f - s;
                    macd.push(Some(macd_val));
                }
                _ => macd.push(None),
            }
        }

        let macd_values: Vec<Decimal> = macd.iter().filter_map(|v| *v).collect();
        let signal_ema = Self::ema(&macd_values, signal_period);
        
        let macd_count = macd.iter().filter(|v| v.is_some()).count();
        let pad_count = macd.len() - signal_ema.len();
        let mut padded_signal = vec![None; pad_count];
        padded_signal.extend(signal_ema);
        signal = padded_signal;

        for (m, s) in macd.iter().zip(signal.iter()) {
            match (m, s) {
                (Some(m_val), Some(s_val)) => histogram.push(Some(m_val - s_val)),
                _ => histogram.push(None),
            }
        }

        (macd, signal, histogram)
    }

    /// Bollinger Bands
    pub fn bollinger_bands(prices: &[Decimal], period: usize, std_dev_multiplier: Decimal) -> (Vec<Option<Decimal>>, Vec<Option<Decimal>>, Vec<Option<Decimal>>) {
        let sma = Self::sma(prices, period);
        let mut upper = Vec::with_capacity(prices.len());
        let mut lower = Vec::with_capacity(prices.len());

        for i in 0..prices.len() {
            if i + 1 >= period {
                let window = &prices[i + 1 - period..=i];
                let mean = sma[i].unwrap_or(Decimal::ZERO);
                
                let variance: Decimal = window.iter()
                    .map(|p| {
                        let diff = *p - mean;
                        diff * diff
                    })
                    .sum::<Decimal>() / Decimal::from(period);
                let std_dev = {
                    let var_f64 = variance.to_f64().unwrap_or(0.0);
                    Decimal::from_f64_retain(var_f64.sqrt()).unwrap_or(Decimal::ZERO)
                };
                
                upper.push(Some(mean + std_dev_multiplier * std_dev));
                lower.push(Some(mean - std_dev_multiplier * std_dev));
            } else {
                upper.push(None);
                lower.push(None);
            }
        }

        (sma, upper, lower)
    }

    /// Average True Range (ATR)
    pub fn atr(high: &[Decimal], low: &[Decimal], close: &[Decimal], period: usize) -> Vec<Option<Decimal>> {
        if high.len() != low.len() || low.len() != close.len() {
            return vec![None; high.len()];
        }

        let mut true_ranges = Vec::with_capacity(high.len());
        true_ranges.push(None);

        for i in 1..high.len() {
            let tr1 = high[i] - low[i];
            let tr2 = (high[i] - close[i - 1]).abs();
            let tr3 = (low[i] - close[i - 1]).abs();
            
            let tr = tr1.max(tr2).max(tr3);
            true_ranges.push(Some(tr));
        }

        Self::ema(&true_ranges.iter().filter_map(|v| *v).collect::<Vec<_>>(), period)
    }

    /// Stochastic Oscillator
    pub fn stochastic(high: &[Decimal], low: &[Decimal], close: &[Decimal], k_period: usize, d_period: usize) -> (Vec<Option<Decimal>>, Vec<Option<Decimal>>) {
        let mut k_values = Vec::with_capacity(high.len());
        
        for i in 0..high.len() {
            if i + 1 >= k_period {
                let window_high = &high[i + 1 - k_period..=i];
                let window_low = &low[i + 1 - k_period..=i];
                
                let highest = *window_high.iter().max().unwrap();
                let lowest = *window_low.iter().min().unwrap();
                let current_close = close[i];
                
                if highest != lowest {
                    let k = (current_close - lowest) / (highest - lowest) * Decimal::from(100);
                    k_values.push(Some(k));
                } else {
                    k_values.push(Some(Decimal::from(50)));
                }
            } else {
                k_values.push(None);
            }
        }

        let k_valid: Vec<Decimal> = k_values.iter().filter_map(|v| *v).collect();
        let d_sma = Self::sma(&k_valid, d_period);
        
        let pad_count = k_values.len() - d_sma.len();
        let mut d_padded = vec![None; pad_count];
        d_padded.extend(d_sma);

        (k_values, d_padded)
    }
}