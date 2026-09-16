use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Current config schema version. Bump on breaking changes and extend
/// `migrate` below.
pub const CONFIG_VERSION: u32 = 1;

fn default_config_version() -> u32 {
    CONFIG_VERSION
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    38241
}

fn default_ollama_url() -> String {
    "http://127.0.0.1:11434".to_string()
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_secret_backend() -> String {
    "env".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalhostConfig {
    /// Bind address. Default loopback only; 0.0.0.0 requires explicit opt-in.
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Whether the API requires the bearer token.
    #[serde(default = "default_true")]
    pub require_auth: bool,
}

fn default_true() -> bool {
    true
}

impl Default for LocalhostConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            require_auth: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// SQLite path. None = `<data_dir>/james.db` (resolved by storage layer).
    #[serde(default)]
    pub path: Option<PathBuf>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { path: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    #[serde(default = "default_provider")]
    pub default_provider: String,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            default_provider: default_provider(),
            ollama_url: default_ollama_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Secret backend: "env" (environment / .env file). "os" reserved.
    #[serde(default = "default_secret_backend")]
    pub backend: String,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            backend: default_secret_backend(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    #[serde(default = "default_config_version")]
    pub version: u32,
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
    #[serde(default)]
    pub localhost: LocalhostConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub models: ModelsConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,
}

impl Default for CoreConfig {
    fn default() -> Self {
        let base_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("james");

        Self {
            version: CONFIG_VERSION,
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
            localhost: LocalhostConfig::default(),
            storage: StorageConfig::default(),
            models: ModelsConfig::default(),
            secrets: SecretsConfig::default(),
        }
    }
}

impl CoreConfig {
    /// Load from `$JAMES_CONFIG`, else `./james.toml`, else the platform
    /// config dir — plus `JAMES_*` environment overrides on top.
    pub fn load() -> anyhow::Result<Self> {
        if let Some(path) = std::env::var_os("JAMES_CONFIG") {
            return Self::load_from(Path::new(&path));
        }
        if Path::new("james.toml").exists() {
            return Self::load_from(Path::new("james.toml"));
        }
        let defaults = Self::default();
        let platform = defaults.config_dir.join("james.toml");
        if platform.exists() {
            return Self::load_from(&platform);
        }
        // Nothing on disk: defaults + env (previous behavior).
        let config = config::Config::builder()
            .add_source(config::Environment::with_prefix("JAMES"))
            .build()?;
        let mut loaded: Self = config.try_deserialize().unwrap_or_else(|_| Self::default());
        loaded.version = CONFIG_VERSION;
        loaded.migrate();
        Ok(loaded)
    }

    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        // Merge over compiled defaults so partial/hand-written files
        // (and pre-v1 files without `version` or sections) keep working.
        let mut base = toml::Value::try_from(&Self::default())?;
        let overlay: toml::Value = toml::from_str(&raw)?;
        merge_toml(&mut base, &overlay);
        let mut loaded: Self = base.try_into()?;
        loaded.migrate()?;
        let env = config::Config::builder()
            .add_source(config::Environment::with_prefix("JAMES"))
            .build()?;
        apply_env_overlay(&mut loaded, env);
        Ok(loaded)
    }

    /// Migrate older schemas to CURRENT. Missing version counts as 0.
    /// Refuses files NEWER than this binary (no silent downgrade).
    fn migrate(&mut self) -> anyhow::Result<()> {
        if self.version > CONFIG_VERSION {
            anyhow::bail!(
                "config version {} is newer than supported v{}; refusing to downgrade",
                self.version,
                CONFIG_VERSION
            );
        }
        // v0 -> v1: sections did not exist; serde defaults filled them.
        self.version = CONFIG_VERSION;
        Ok(())
    }

    /// Save to the canonical location: `./james.toml` if it already exists
    /// (keeps repo-local setups round-tripping), else `<config_dir>/james.toml`.
    pub fn save(&self) -> anyhow::Result<()> {
        if Path::new("james.toml").exists() {
            return self.save_to(Path::new("james.toml"));
        }
        std::fs::create_dir_all(&self.config_dir)?;
        self.save_to(&self.config_dir.join("james.toml"))
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let toml = toml::to_string_pretty(self)?;
        std::fs::write(path, toml)?;
        Ok(())
    }

    /// Validate everything; collects ALL violations before returning.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.event_bus_buffer_size == 0 {
            errors.push("event_bus_buffer_size must be >= 1".to_string());
        }
        if self.max_concurrent_tasks == 0 {
            errors.push("max_concurrent_tasks must be >= 1".to_string());
        }
        if self.task_timeout_secs == 0 {
            errors.push("task_timeout_secs must be >= 1".to_string());
        }
        if self.health_check_interval_secs == 0 {
            errors.push("health_check_interval_secs must be >= 1".to_string());
        }
        if self.scheduler_tick_interval_secs == 0 {
            errors.push("scheduler_tick_interval_secs must be >= 1".to_string());
        }
        match self.log_level.to_ascii_lowercase().as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => {}
            other => errors.push(format!("log_level '{other}' is not one of trace/debug/info/warn/error")),
        }
        if self.localhost.port == 0 {
            errors.push("localhost.port must be 1..=65535".to_string());
        }
        if self.localhost.bind.is_empty() {
            errors.push("localhost.bind must not be empty".to_string());
        }
        if self.models.default_provider.trim().is_empty() {
            errors.push("models.default_provider must not be empty".to_string());
        }
        if self.models.ollama_url.trim().is_empty() {
            errors.push("models.ollama_url must not be empty".to_string());
        }
        match self.secrets.backend.as_str() {
            // "os" is reserved for the OS credential store (later phase).
            "env" => {}
            other => errors.push(format!(
                "secrets.backend '{other}' is unknown (supported: env)"
            )),
        }
        if self.instance_name.trim().is_empty() {
            errors.push("instance_name must not be empty".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Recursive table merge: `overlay` wins, missing keys keep `base`.
fn merge_toml(base: &mut toml::Value, overlay: &toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_map), toml::Value::Table(overlay_map)) => {
            for (k, v) in overlay_map {
                match base_map.get_mut(k) {
                    Some(existing) => merge_toml(existing, v),
                    None => {
                        base_map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value.clone();
        }
    }
}

/// Overlay flat JAMES_* env vars onto an already-loaded config.
/// Only keys this layer owns; unknown keys are ignored (not an error).
fn apply_env_overlay(config: &mut CoreConfig, overlay: config::Config) {
    if let Ok(v) = overlay.get_string("instance_name") {
        config.instance_name = v;
    }
    if let Ok(v) = overlay.get_string("log_level") {
        config.log_level = v;
    }
    if let Ok(v) = overlay.get::<u16>("localhost.port") {
        config.localhost.port = v;
    }
    if let Ok(v) = overlay.get_string("localhost.bind") {
        config.localhost.bind = v;
    }
    if let Ok(v) = overlay.get_string("models.default_provider") {
        config.models.default_provider = v;
    }
    if let Ok(v) = overlay.get_string("models.ollama_url") {
        config.models.ollama_url = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_roundtrip() {
        let dir = std::env::temp_dir().join("james-config-test-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("james.toml");

        let mut config = CoreConfig::default();
        config.instance_name = "roundtrip-test".to_string();
        config.localhost.port = 39999;
        config.models.default_provider = "test-provider".to_string();
        config.save_to(&path).unwrap();

        let loaded = CoreConfig::load_from(&path).unwrap();
        assert_eq!(loaded.instance_name, "roundtrip-test");
        assert_eq!(loaded.localhost.port, 39999);
        assert_eq!(loaded.models.default_provider, "test-provider");
        assert_eq!(loaded.version, CONFIG_VERSION);
        assert!(loaded.validate().is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_config_validation_collects_all_violations() {
        let mut config = CoreConfig::default();
        config.event_bus_buffer_size = 0;
        config.log_level = "verbose".to_string();
        config.localhost.port = 0;
        config.secrets.backend = "vault".to_string();

        let errors = config.validate().expect_err("must be invalid");
        assert!(errors.len() >= 4, "expected all violations, got: {errors:?}");
    }

    #[test]
    fn test_config_migrates_legacy_file_without_version() {
        let dir = std::env::temp_dir().join("james-config-test-legacy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("james.toml");
        // Pre-v1 file: no version, no sections.
        std::fs::write(&path, "instance_name = \"legacy\"\nlog_level = \"debug\"\n").unwrap();

        let loaded = CoreConfig::load_from(&path).unwrap();
        assert_eq!(loaded.version, CONFIG_VERSION);
        assert_eq!(loaded.instance_name, "legacy");
        assert_eq!(loaded.localhost.port, default_port());
        assert!(loaded.validate().is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_config_refuses_newer_version() {
        let dir = std::env::temp_dir().join("james-config-test-newer");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("james.toml");
        std::fs::write(
            &path,
            format!("version = {}\ninstance_name = \"future\"\n", CONFIG_VERSION + 1),
        )
        .unwrap();

        assert!(CoreConfig::load_from(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
