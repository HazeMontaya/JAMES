use james_core::{JamesCore, CoreConfig, LogFields, init_tracing};
use anyhow::Result;
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let log = LogFields::new("james-core");
    info!("{} Starting JAMES Core v{}", log.prefix(), env!("CARGO_PKG_VERSION"));

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

    info!("{} JAMES Core stopped", log.prefix());
    Ok(())
}
