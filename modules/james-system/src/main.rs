//! James-System - Runnable JAMES system (core + all first-party modules).
//!
//! This is a convenience launcher for the fully assembled system. The
//! zero-module path is `james-app` (core only); this binary proves the
//! assembly wiring works end-to-end.

use anyhow::Result;
use james_assembly::{AssemblyOptions, JamesAssembly};
use james_core::{CoreConfig, init_tracing, LogFields};
use tokio::signal;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let log = LogFields::new("james-system");
    info!(
        "{} Starting JAMES System v{} (core + all first-party modules)",
        log.prefix(),
        env!("CARGO_PKG_VERSION")
    );

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

    let core = james_core::JamesCore::new(config).await?;
    let options = AssemblyOptions {
        enable_web: true,
        enable_voice: true,
        enable_dashboard: true,
        ..AssemblyOptions::default()
    };

    let mut assembly = JamesAssembly::new(core, options).await?;
    assembly.start().await?;

    info!("{} JAMES System running. Press Ctrl+C to stop.", log.prefix());
    signal::ctrl_c().await?;

    info!("{} Shutdown signal received", log.prefix());
    assembly.stop().await?;

    info!("{} JAMES System stopped", log.prefix());
    Ok(())
}