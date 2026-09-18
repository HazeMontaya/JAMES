//! Types for Financial Research Orchestrator

use james_market_data::types::*;
use james_adapter_kronos::types::*;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a research report
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReportId(pub Uuid);

impl ReportId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ReportId {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export commonly used types
pub use james_market_data::types::{Symbol, Exchange, Timeframe, OHLCVBar, DataRequest, ProviderId};
pub use james_adapter_kronos::types::{KronosVariant, ForecastRequest, ForecastResult, EmbeddingRequest, MarketEmbeddings, FinetuneConfig, FinetuneResult, AdapterHealth, AdapterId, CapabilityId, ModelHandle};
pub use james_market_data::types::KronosOHLCV;