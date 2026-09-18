//! JAMES Adapter for Kronos Financial Time Series Foundation Model
//!
//! Provides integration with Kronos (shiyu-coder/Kronos) for financial forecasting,
//! market embeddings, and domain-specific finetuning.

pub mod adapter;
pub mod config;
pub mod error;
pub mod finetune;
pub mod python_bridge;
pub mod types;

use async_trait::async_trait;
use crate::types::*;

/// Trait for Kronos Adapter implementations
#[async_trait]
pub trait KronosAdapter: Send + Sync {
    fn adapter_id(&self) -> AdapterId;
    fn capabilities(&self) -> Vec<CapabilityId>;
    
    async fn load_model(&self, variant: KronosVariant) -> Result<ModelHandle>;
    async fn forecast(&self, request: ForecastRequest) -> Result<ForecastResult>;
    async fn forecast_batch(&self, requests: Vec<ForecastRequest>) -> Result<Vec<ForecastResult>>;
    async fn get_embeddings(&self, request: EmbeddingRequest) -> Result<MarketEmbeddings>;
    async fn finetune(&self, config: FinetuneConfig) -> Result<FinetuneResult>;
    async fn health(&self) -> AdapterHealth;
}

pub use adapter::KronosAdapterImpl;
pub use config::KronosAdapterConfig;
pub use error::{KronosAdapterError, Result};
pub use finetune::FinetunePipeline;
pub use types::*;