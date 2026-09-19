//! Bridge configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// NATS server URL
    #[serde(default = "default_nats_url")]
    pub nats_url: String,

    /// Name of this bridge service
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// Python service name to connect to
    #[serde(default = "default_python_service_name")]
    pub python_service_name: String,

    /// Subject prefix for capability execution
    #[serde(default = "default_subject_prefix")]
    pub subject_prefix: String,

    /// Shared secret required for capability execution over NATS.
    /// Empty disables remote execution until configured explicitly.
    #[serde(default)]
    pub bridge_token: String,

    /// Whether to auto-register Python skills as capabilities
    #[serde(default = "default_true")]
    pub auto_register_skills: bool,

    /// Path to Python skills directory (for discovery)
    #[serde(default = "default_skills_path")]
    pub skills_path: PathBuf,

    /// Health check interval seconds
    #[serde(default = "default_health_interval")]
    pub health_check_interval_secs: u64,

    /// Request timeout seconds
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,
}

fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_service_name() -> String {
    "james-python-bridge".to_string()
}

fn default_python_service_name() -> String {
    "james-python".to_string()
}

fn default_subject_prefix() -> String {
    "james.bridge".to_string()
}

fn default_true() -> bool {
    true
}

fn default_skills_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".james")
        .join("skills")
}

fn default_health_interval() -> u64 {
    30
}

fn default_request_timeout() -> u64 {
    60
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            nats_url: default_nats_url(),
            service_name: default_service_name(),
            python_service_name: default_python_service_name(),
            subject_prefix: default_subject_prefix(),
            bridge_token: String::new(),
            auto_register_skills: default_true(),
            skills_path: default_skills_path(),
            health_check_interval_secs: default_health_interval(),
            request_timeout_secs: default_request_timeout(),
        }
    }
}

impl BridgeConfig {
    /// Load config from file, environment, or defaults
    pub fn load() -> anyhow::Result<Self> {
        let mut config = Self::default();

        // Try to load from config file
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("james").join("bridge.toml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let file_config: Self = toml::from_str(&content)?;
                config = file_config;
            }
        }

        // Override with environment variables (prefixed with JAMES_BRIDGE_)
        if let Ok(url) = std::env::var("JAMES_BRIDGE_NATS_URL") {
            config.nats_url = url;
        }
        if let Ok(name) = std::env::var("JAMES_BRIDGE_SERVICE_NAME") {
            config.service_name = name;
        }
        if let Ok(name) = std::env::var("JAMES_BRIDGE_PYTHON_SERVICE") {
            config.python_service_name = name;
        }
        if let Ok(prefix) = std::env::var("JAMES_BRIDGE_SUBJECT_PREFIX") {
            config.subject_prefix = prefix;
        }
        if let Ok(token) = std::env::var("JAMES_BRIDGE_TOKEN") {
            config.bridge_token = token;
        }
        if let Ok(auto) = std::env::var("JAMES_BRIDGE_AUTO_REGISTER") {
            config.auto_register_skills = auto.parse().unwrap_or(true);
        }
        if let Ok(path) = std::env::var("JAMES_BRIDGE_SKILLS_PATH") {
            config.skills_path = PathBuf::from(path);
        }

        Ok(config)
    }
}