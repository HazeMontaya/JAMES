use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    pub instance_name: String,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub log_level: String,
    pub event_bus_buffer_size: usize,
    pub health_check_interval_secs: u64,
    pub scheduler_tick_interval_secs: u64,
    pub registry_cleanup_interval_secs: u64,
    pub task_timeout_secs: u64,
    pub max_concurrent_tasks: usize,
    pub enable_telemetry: bool,
}

impl Default for CoreConfig {
    fn default() -> Self {
        let base_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("james");

        Self {
            instance_name: "james-core".to_string(),
            data_dir: base_dir.join("data"),
            config_dir: base_dir.join("config"),
            log_level: "info".to_string(),
            event_bus_buffer_size: 1000,
            health_check_interval_secs: 30,
            scheduler_tick_interval_secs: 1,
            registry_cleanup_interval_secs: 300,
            task_timeout_secs: 300,
            max_concurrent_tasks: 100,
            enable_telemetry: false,
        }
    }
}

impl CoreConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::with_name("james").required(false))
            .add_source(config::Environment::with_prefix("JAMES"))
            .build()?;
        
        Ok(config.try_deserialize()?)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        let path = self.config_dir.join("james.toml");
        let toml = toml::to_string_pretty(self)?;
        std::fs::write(path, toml)?;
        Ok(())
    }
}