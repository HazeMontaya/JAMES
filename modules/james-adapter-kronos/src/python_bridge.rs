//! Python bridge for Kronos model inference with local caching

use pyo3::prelude::*;
use pyo3::types::PyModule;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};

use crate::types::*;
use crate::error::{KronosAdapterError, Result};

/// Synchronous Python bridge for Kronos operations
pub struct KronosPythonBridge {
    cache_dir: PathBuf,
}

impl KronosPythonBridge {
    pub fn new(python_path: Option<std::path::PathBuf>, cache_dir: PathBuf) -> Result<Self> {
        pyo3::prepare_freethreaded_python();
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    fn tokenizer_cache_path(&self, variant: KronosVariant) -> PathBuf {
        self.cache_dir.join(format!("tokenizer_{}.pt", variant as u8))
    }

    fn model_cache_path(&self, variant: KronosVariant) -> PathBuf {
        self.cache_dir.join(format!("model_{}.pt", variant as u8))
    }

    fn predictor_cache_path(&self, variant: KronosVariant) -> PathBuf {
        self.cache_dir.join(format!("predictor_{}.pt", variant as u8))
    }

    /// Load tokenizer from cache or HuggingFace Hub
    pub fn load_tokenizer(&self, variant: KronosVariant) -> Result<Py<PyAny>> {
        let cache_path = self.tokenizer_cache_path(variant);
        
        if cache_path.exists() {
            Python::with_gil(|py| -> PyResult<Py<PyAny>> {
                let torch = py.import("torch")?;
                let tokenizer = torch.call_method1("load", (cache_path.to_str().unwrap(),))?;
                Ok(tokenizer.into())
            }).map_err(|e| KronosAdapterError::PythonBridgeError(e.to_string()))
        } else {
            Python::with_gil(|py| -> PyResult<Py<PyAny>> {
                let tokenizer_id = variant.hf_tokenizer_id();
                let model_module = py.import("model")?;
                let tokenizer_class = model_module.getattr("KronosTokenizer")?;
                let tokenizer = tokenizer_class.call_method1("from_pretrained", (tokenizer_id,))?;
                
                // Save to cache
                let torch = py.import("torch")?;
                torch.call_method1("save", (tokenizer, cache_path.to_str().unwrap()))?;
                
                Ok(tokenizer.into())
            }).map_err(|e| KronosAdapterError::PythonBridgeError(e.to_string()))
        }
    }

    /// Load model from cache or HuggingFace Hub
    pub fn load_model(&self, variant: KronosVariant) -> Result<Py<PyAny>> {
        let cache_path = self.model_cache_path(variant);
        
        if cache_path.exists() {
            Python::with_gil(|py| -> PyResult<Py<PyAny>> {
                let torch = py.import("torch")?;
                let model = torch.call_method1("load", (cache_path.to_str().unwrap(),))?;
                Ok(model.into())
            }).map_err(|e| KronosAdapterError::PythonBridgeError(e.to_string()))
        } else {
            Python::with_gil(|py| -> PyResult<Py<PyAny>> {
                let model_id = variant.hf_model_id();
                let model_module = py.import("model")?;
                let kronos_class = model_module.getattr("Kronos")?;
                let model = kronos_class.call_method1("from_pretrained", (model_id,))?;
                
                // Save to cache
                let torch = py.import("torch")?;
                torch.call_method1("save", (model, cache_path.to_str().unwrap()))?;
                
                Ok(model.into())
            }).map_err(|e| KronosAdapterError::PythonBridgeError(e.to_string()))
        }
    }

    /// Create KronosPredictor instance
    pub fn create_predictor(
        &self,
        model: &Py<PyAny>,
        tokenizer: &Py<PyAny>,
        max_context: usize,
    ) -> Result<Py<PyAny>> {
        Python::with_gil(|py| -> PyResult<Py<PyAny>> {
            let model_module = py.import("model")?;
            let predictor_class = model_module.getattr("KronosPredictor")?;
            let predictor = predictor_class.call1((model, tokenizer, max_context))?;
            Ok(predictor.into())
        }).map_err(|e| KronosAdapterError::PythonBridgeError(e.to_string()))
    }

    /// Move model to device
    pub fn to_device(&self, model: &Py<PyAny>, device: &str) -> Result<()> {
        Python::with_gil(|py| -> PyResult<()> {
            let torch = py.import("torch")?;
            let torch_device = torch.getattr("device")?.call1((device,))?;
            model.call_method1(py, "to", (torch_device,))?;
            Ok(())
        }).map_err(|e| KronosAdapterError::PythonBridgeError(e.to_string()))
    }

    /// Set model to eval mode
    pub fn set_eval_mode(&self, model: &Py<PyAny>) -> Result<()> {
        Python::with_gil(|py| -> PyResult<()> {
            model.call_method0(py, "eval")?;
            Ok(())
        }).map_err(|e| KronosAdapterError::PythonBridgeError(e.to_string()))
    }
}