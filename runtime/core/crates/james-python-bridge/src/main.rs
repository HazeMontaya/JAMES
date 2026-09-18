//! JAMES Python Bridge - CLI binary

use clap::Parser;
use james_core::CoreConfig;
use james_core::JamesCore;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use tracing_subscriber::{EnvFilter, fmt};

use james_python_bridge::{BridgeConfig, PythonBridge};

#[derive(Parser, Debug)]
#[command(name = "james-python-bridge", version, about = "JAMES Python Bridge - connects Rust core to Python services")]
struct Args {
    /// NATS server URL
    #[arg(long, env = "JAMES_BRIDGE_NATS_URL")]
    nats_url: Option<String>,

    /// Bridge service name
    #[arg(long, env = "JAMES_BRIDGE_SERVICE_NAME")]
    service_name: Option<String>,

    /// Python service name
    #[arg(long, env = "JAMES_BRIDGE_PYTHON_SERVICE")]
    python_service: Option<String>,

    /// Subject prefix
    #[arg(long, env = "JAMES_BRIDGE_SUBJECT_PREFIX")]
    subject_prefix: Option<String>,

    /// Auto-register skills
    #[arg(long, env = "JAMES_BRIDGE_AUTO_REGISTER")]
    auto_register: Option<bool>,

    /// Skills path
    #[arg(long, env = "JAMES_BRIDGE_SKILLS_PATH")]
    skills_path: Option<String>,

    /// Health check interval (seconds)
    #[arg(long, env = "JAMES_BRIDGE_HEALTH_INTERVAL")]
    health_interval: Option<u64>,

    /// Request timeout (seconds)
    #[arg(long, env = "JAMES_BRIDGE_REQUEST_TIMEOUT")]
    request_timeout: Option<u64>,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Run as standalone (without JamesCore)
    #[arg(long)]
    standalone: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize tracing
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&args.log_level));
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("Starting JAMES Python Bridge");

    // Build configuration
    let mut config = BridgeConfig::load()?;

    if let Some(url) = args.nats_url {
        config.nats_url = url;
    }
    if let Some(name) = args.service_name {
        config.service_name = name;
    }
    if let Some(name) = args.python_service {
        config.python_service_name = name;
    }
    if let Some(prefix) = args.subject_prefix {
        config.subject_prefix = prefix;
    }
    if let Some(auto) = args.auto_register {
        config.auto_register_skills = auto;
    }
    if let Some(path) = args.skills_path {
        config.skills_path = std::path::PathBuf::from(path);
    }
    if let Some(interval) = args.health_interval {
        config.health_check_interval_secs = interval;
    }
    if let Some(timeout) = args.request_timeout {
        config.request_timeout_secs = timeout;
    }

    // Create and initialize bridge
    let mut bridge = if args.standalone {
        PythonBridge::new(config)
    } else {
        // Try to create with JamesCore
        let core_config = CoreConfig::default();
        let core = JamesCore::new(core_config).await?;
        PythonBridge::from_james_core(config, Arc::new(RwLock::new(core)))
    };

    // Initialize and start
    bridge.initialize().await?;
    bridge.start().await?;

    // Sync capabilities once at startup
    if let Err(e) = bridge.sync_capabilities().await {
        error!("Initial capability sync failed: {}", e);
    }

    info!("Python Bridge running. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    info!("Shutdown signal received");
    bridge.shutdown().await?;

    Ok(())
}