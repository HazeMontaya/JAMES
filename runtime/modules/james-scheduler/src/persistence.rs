//! Legacy scheduler manifest persistence adapter.
//!
//! This is deliberately isolated from scheduling authority. The canonical
//! scheduler state lives in james-scheduler-core::Scheduler; this adapter only
//! serializes the module compatibility manifest while migration callers exist.

use std::path::Path;
use anyhow::Result;
use tokio::fs;
use super::{ScheduledJob, SchedulerConfig};

pub(crate) async fn load(config: &SchedulerConfig) -> Result<Vec<ScheduledJob>> {
    let path = Path::new(&config.database_path);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).await?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&raw)?)
}

pub(crate) async fn save(config: &SchedulerConfig, jobs: &[ScheduledJob]) -> Result<()> {
    let path = Path::new(&config.database_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await?;
        }
    }
    fs::write(path, serde_json::to_string_pretty(jobs)?).await?;
    Ok(())
}
