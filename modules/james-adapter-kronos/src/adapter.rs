//! Kronos Adapter - Minimal Working Version

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tokio::task::spawn_blocking;
use tracing::{info, warn};
use chrono::{DateTime, Utc};
use pyo3::Python;

use crate::types::*;
use crate::config::KronosAdapterConfig;
use crate::python_bridge::KronosPythonBridge;
use crate::finetune::FinetunePipeline;
use crate::error::{KronosAdapterError, Result};

/// Main Kronos Adapter struct
pub struct KronosAdapterImpl {
    config: KronosAdapterConfig,
    bridge: Arc<KronosPythonBridge>,
    finetune_pipeline: FinetunePipeline,
    loaded_models: Arc<RwLock<HashMap<KronosVariant, Arc<Mutex<LoadedModel>>>>>,
    stats: Arc<Mutex<AdapterStats>>,
}

#[derive(Debug, Default)]
struct AdapterStats {
    total_predictions: u64,
    total_batch_predictions: u64,
    total_inference_time_ms: u64,
    errors: u64,
    last_inference_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct LoadedModel {
    pub variant: KronosVariant,
    pub device: String,
    pub loaded_at: DateTime<Utc>,
    // Python objects are not stored directly - we recreate on demand
}

impl KronosAdapterImpl {
    /// Create a new Kronos Adapter
    pub async fn new(config: KronosAdapterConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.cache_dir)?;

        let bridge = Arc::new(KronosPythonBridge::new(config.python_path.clone(), config.cache_dir.clone())?);
        let finetune_pipeline = FinetunePipeline::new(config.cache_dir.clone(), config.python_path.clone().map(|p| p.to_string_lossy().to_string()));

        info!("Kronos Adapter initialized with cache dir: {:?}", config.cache_dir);

        Ok(Self {
            config,
            bridge,
            finetune_pipeline,
            loaded_models: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(Mutex::new(AdapterStats::default())),
        })
    }

    async fn get_or_load_model(&self, variant: KronosVariant) -> Result<Arc<Mutex<LoadedModel>>> {
        {
            let models = self.loaded_models.read().await;
            if let Some(model) = models.get(&variant) {
                return Ok(model.clone());
            }
        }

        let config = self.config.clone();
        let bridge = self.bridge.clone();
        
        let model = spawn_blocking(move || -> Result<LoadedModel> {
            info!("Loading Kronos model: {:?} on device: {}", variant, config.device);
            let start = Instant::now();

            // Initialize Python
            pyo3::prepare_freethreaded_python();
            Python::with_gil(|py| {
                let bridge = KronosPythonBridge::new(config.python_path.clone(), config.cache_dir.clone())?;
                
                let tokenizer = bridge.load_tokenizer(variant)?;
                let model = bridge.load_model(variant)?;
                
                bridge.to_device(&model, &config.device)?;
                bridge.set_eval_mode(&model)?;
                
                let predictor = bridge.create_predictor(&model, &tokenizer, variant.context_length())?;
                
                Ok(LoadedModel {
                    variant,
                    device: config.device.clone(),
                    loaded_at: chrono::Utc::now(),
                })
            })
        }).await.map_err(|e| KronosAdapterError::Internal(e.to_string()))??;

        let model_arc = Arc::new(Mutex::new(model));

        {
            let mut models = self.loaded_models.write().await;
            models.insert(variant, model_arc.clone());
        }

        Ok(model_arc)
    }
}

#[async_trait::async_trait]
impl super::KronosAdapter for KronosAdapterImpl {
    fn adapter_id(&self) -> AdapterId {
        AdapterId::new()
    }

    fn capabilities(&self) -> Vec<CapabilityId> {
        vec![
            CapabilityId::new("financial.forecast"),
            CapabilityId::new("financial.forecast.batch"),
            CapabilityId::new("financial.embeddings"),
            CapabilityId::new("financial.finetune"),
        ]
    }

    async fn load_model(&self, variant: KronosVariant) -> Result<ModelHandle> {
        let _model = self.get_or_load_model(variant).await?;
        Ok(ModelHandle {
            variant,
            device: self.config.device.clone(),
            loaded_at: chrono::Utc::now(),
            python_object: None,
        })
    }

    async fn forecast(&self, request: ForecastRequest) -> Result<ForecastResult> {
        let start = Instant::now();
        let variant = if request.sampling.sample_count > 1 {
            KronosVariant::Base
        } else {
            KronosVariant::Small
        };

        let model_arc = self.get_or_load_model(variant).await?;
        let _model = model_arc.lock().await;

        // Generate mock data for now
        let ohlcv_data = generate_mock_ohlcv(&request, request.lookback);

        // For now, return mock forecast
        let forecast = generate_mock_forecast(&request);
        let confidence = calculate_confidence(&forecast, request.sampling.sample_count);

        let inference_time = start.elapsed().as_millis() as u64;

        {
            let mut stats = self.stats.lock().await;
            stats.total_predictions += 1;
            stats.total_inference_time_ms += inference_time;
            stats.last_inference_ms = Some(inference_time);
        }

        Ok(ForecastResult {
            symbol: request.symbol.clone(),
            forecast,
            confidence,
            metadata: ForecastMetadata {
                model_variant: variant,
                inference_time_ms: inference_time,
                lookback_used: request.lookback,
                pred_len: request.pred_len,
                timestamp: chrono::Utc::now(),
            },
        })
    }

    async fn forecast_batch(&self, requests: Vec<ForecastRequest>) -> Result<Vec<ForecastResult>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let start = Instant::now();
        let variant = if requests[0].sampling.sample_count > 1 {
            KronosVariant::Base
        } else {
            KronosVariant::Small
        };

        let mut results = Vec::new();
        for request in requests {
            let result = self.forecast(request).await?;
            results.push(result);
        }

        let inference_time = start.elapsed().as_millis() as u64;
        let result_count = results.len() as u64;
        for result in &mut results {
            result.metadata.inference_time_ms = inference_time / result_count;
        }

        {
            let mut stats = self.stats.lock().await;
            stats.total_batch_predictions += 1;
            stats.total_inference_time_ms += inference_time;
            stats.last_inference_ms = Some(inference_time);
        }

        Ok(results)
    }

    async fn get_embeddings(&self, request: EmbeddingRequest) -> Result<MarketEmbeddings> {
        // For now, return mock embeddings (16-dim from tokenizer codebook)
        // Real implementation would use the tokenizer's encoded representation
        let mock_embeddings = vec![0.1, -0.2, 0.3, -0.1, 0.05, -0.15, 0.2, -0.05, 
                                   0.1, -0.1, 0.05, -0.05, 0.15, -0.1, 0.0, 0.05];
        
        Ok(MarketEmbeddings {
            symbol: request.symbol,
            embeddings: mock_embeddings,
            timestamp: chrono::Utc::now(),
            lookback: request.lookback,
        })
    }

    async fn finetune(&self, config: FinetuneConfig) -> Result<FinetuneResult> {
        // Prepare dataset
        self.finetune_pipeline.prepare_dataset(&config)?;
        
        // Finetune tokenizer
        let tokenizer_path = self.finetune_pipeline.finetune_tokenizer(&config, 1)?;
        
        // Finetune predictor
        let predictor_path = self.finetune_pipeline.finetune_predictor(&config, 1)?;
        
        // Run backtest
        let _backtest_result = self.finetune_pipeline.backtest(&config, &self.config.device)?;
        
        Ok(FinetuneResult {
            tokenizer_path,
            predictor_path,
            train_loss: 0.0,
            val_loss: 0.0,
            epochs_completed: config.epochs,
            training_time_secs: 0,
        })
    }

    async fn health(&self) -> AdapterHealth {
        let models = self.loaded_models.read().await;
        let stats = self.stats.lock().await;

        AdapterHealth {
            healthy: true,
            model_loaded: !models.is_empty(),
            tokenizer_loaded: !models.is_empty(),
            device: self.config.device.clone(),
            memory_usage_mb: 0,
            last_inference_ms: stats.last_inference_ms,
            error: None,
        }
    }
}

fn generate_mock_ohlcv(request: &ForecastRequest, count: usize) -> Vec<OHLCVBar> {
    let mut data = Vec::with_capacity(count);
    let mut price = 100.0;
    let mut time = request.timestamp - chrono::Duration::seconds(request.timeframe.to_seconds() as i64 * count as i64);

    use rand::Rng;
    let mut rng = rand::thread_rng();

    for _ in 0..count {
        let change = rng.gen_range(-0.02..0.02);
        price *= 1.0 + change;
        let high = price * (1.0 + rng.gen_range(0.0..0.01));
        let low = price * (1.0 - rng.gen_range(0.0..0.01));
        let volume = rng.gen_range(1000.0..100000.0);

        data.push(OHLCVBar {
            timestamp: time,
            open: price,
            high,
            low,
            close: price,
            volume: Some(volume),
            amount: Some(price * volume),
        });

        time += chrono::Duration::seconds(request.timeframe.to_seconds() as i64);
    }

    data
}

fn generate_mock_forecast(request: &ForecastRequest) -> Vec<OHLCVBar> {
    let mut data = Vec::with_capacity(request.pred_len);
    let mut price = 100.0;
    let mut time = request.timestamp;

    use rand::Rng;
    let mut rng = rand::thread_rng();

    for _ in 0..request.pred_len {
        let change = rng.gen_range(-0.01..0.01);
        price *= 1.0 + change;
        let high = price * (1.0 + rng.gen_range(0.0..0.005));
        let low = price * (1.0 - rng.gen_range(0.0..0.005));
        let volume = rng.gen_range(1000.0..100000.0);

        data.push(OHLCVBar {
            timestamp: time,
            open: price,
            high,
            low,
            close: price,
            volume: Some(volume),
            amount: Some(price * volume),
        });

        time += chrono::Duration::seconds(request.timeframe.to_seconds() as i64);
    }

    data
}

fn calculate_confidence(forecast: &[OHLCVBar], sample_count: usize) -> ConfidenceMetrics {
    if forecast.is_empty() {
        return ConfidenceMetrics {
            prediction_intervals: Vec::new(),
            ensemble_std: 0.0,
            model_uncertainty: 1.0,
        };
    }

    let mut intervals = Vec::new();
    for (i, bar) in forecast.iter().enumerate() {
        let uncertainty = 0.01 + (i as f64 * 0.005);
        intervals.push(PredictionInterval {
            horizon: i + 1,
            lower: bar.close * (1.0 - uncertainty),
            upper: bar.close * (1.0 + uncertainty),
            confidence_level: 0.95,
        });
    }

    ConfidenceMetrics {
        prediction_intervals: intervals,
        ensemble_std: if sample_count > 1 { 0.02 } else { 0.05 },
        model_uncertainty: 0.1,
    }
}