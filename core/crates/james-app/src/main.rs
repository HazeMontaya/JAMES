use james_core::{JamesCore, CoreConfig, LogFields, init_tracing};
use anyhow::Result;
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let log = LogFields::new("james-app");
    info!("{} Starting JAMES App v{}", log.prefix(), env!("CARGO_PKG_VERSION"));

    let mut config = CoreConfig::load().unwrap_or_else(|e| {
        error!("{} Failed to load config: {}, using defaults", log.prefix(), e);
        CoreConfig::default()
    });

    if let Err(errors) = config.validate() {
        error!(
            "{} Invalid configuration ({} problem(s)), using defaults: {}",
            log.prefix(),
            errors.len(),
            errors.join("; ")
        );
        config = CoreConfig::default();
    }

    if let Err(e) = config.save() {
        error!("{} Failed to save config: {}", log.prefix(), e);
    }

    // Single construction path: JamesCore::new wires all components.
    // (No manual field assignment, no duplicate initialization.)
    let mut core = JamesCore::new(config).await?;
    core.start().await?;

    info!("{} JAMES Core running. Press Ctrl+C to stop.", log.prefix());
    signal::ctrl_c().await?;

    info!("{} Shutdown signal received", log.prefix());
    core.stop().await?;

    info!("{} JAMES Core stopped", log.prefix());
    Ok(())
}