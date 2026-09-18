//! Error types for Kronos Adapter

use thiserror::Error;

#[derive(Error, Debug)]
pub enum KronosAdapterError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Model loading failed: {0}")]
    ModelLoadFailed(String),

    #[error("Tokenizer loading failed: {0}")]
    TokenizerLoadFailed(String),

    #[error("Python bridge error: {0}")]
    PythonBridgeError(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Data fetch failed: {0}")]
    DataFetchFailed(String),

    #[error("Data normalization failed: {0}")]
    NormalizationFailed(String),

    #[error("Finetuning failed: {0}")]
    FinetuningFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Device error: {0}")]
    DeviceError(String),

    #[error("HuggingFace Hub error: {0}")]
    HubError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Python exception: {0}")]
    PyError(#[from] pyo3::PyErr),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("TOML deserialization error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, KronosAdapterError>;