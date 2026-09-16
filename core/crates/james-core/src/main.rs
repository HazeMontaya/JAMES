use james_core::{JamesCore, CoreConfig};
use tracing_subscriber::EnvFilter;
use anyhow::Result;
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    info!("Starting JAMES Core v{}", env!("CARGO_PKG_VERSION"));

    let mut config = CoreConfig::load().unwrap_or_else(|e| {
        error!("Failed to load config: {}, using defaults", e);
        CoreConfig::default()
    });

    if let Err(errors) = config.validate() {
        error!(
            "Invalid configuration ({} problem(s)), using defaults: {}",
            errors.len(),
            errors.join("; ")
        );
        config = CoreConfig::default();
    }

    if let Err(e) = config.save() {
        error!("Failed to save config: {}", e);
    }

    // Single construction path: JamesCore::new wires all components.
    // (No manual field assignment, no duplicate initialization.)
    let mut core = JamesCore::new(config).await?;
    core.start().await?;

    info!("JAMES Core running. Press Ctrl+C to stop.");
    signal::ctrl_c().await?;

    info!("Shutdown signal received");
    core.stop().await?;

    info!("JAMES Core stopped");
    Ok(())
}

fn init_tracing() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,james_core=debug,james_events=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    Ok(())
}
