//! Finetuning pipeline wrapper for Kronos

use std::path::{Path, PathBuf};
use std::process::Command;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::*;
use crate::error::{KronosAdapterError, Result};

/// Finetuning result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinetuneResult {
    pub tokenizer_path: PathBuf,
    pub predictor_path: PathBuf,
    pub train_loss: f64,
    pub val_loss: f64,
    pub epochs_completed: usize,
    pub training_time_secs: u64,
}

/// Finetuning pipeline
pub struct FinetunePipeline {
    kronos_repo_path: PathBuf,
    python_executable: String,
}

impl FinetunePipeline {
    pub fn new(kronos_repo_path: PathBuf, python_executable: Option<String>) -> Self {
        Self {
            kronos_repo_path,
            python_executable: python_executable.unwrap_or_else(|| "python".to_string()),
        }
    }

    /// Prepare dataset using Qlib
    pub fn prepare_dataset(&self, config: &FinetuneConfig) -> Result<()> {
        let script = self.kronos_repo_path.join("finetune/qlib_data_preprocess.py");
        
        if !script.exists() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Dataset prep script not found: {}", script.display())
            ));
        }

        // Set environment variables
        let mut cmd = Command::new(&self.python_executable);
        cmd.arg(&script)
            .env("PYTHONPATH", &self.kronos_repo_path)
            .env("QLIB_DATA_PATH", &config.output_dir)
            .current_dir(&self.kronos_repo_path);

        let output = cmd.output()
            .map_err(|e| crate::error::KronosAdapterError::Internal(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Dataset preparation failed: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }

        Ok(())
    }

    /// Finetune tokenizer
    pub fn finetune_tokenizer(&self, config: &FinetuneConfig, num_gpus: usize) -> Result<PathBuf> {
        let script = self.kronos_repo_path.join("finetune/train_tokenizer.py");
        
        if !script.exists() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Tokenizer training script not found: {}", script.display())
            ));
        }

        let mut cmd = Command::new(&self.python_executable);
        cmd.arg("-m")
            .arg("torch.distributed.run")
            .arg("--standalone")
            .arg("--nproc_per_node")
            .arg(num_gpus.to_string())
            .arg(&script)
            .env("PYTHONPATH", &self.kronos_repo_path)
            .current_dir(&self.kronos_repo_path);

        let output = cmd.output()
            .map_err(|e| crate::error::KronosAdapterError::Internal(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Tokenizer finetuning failed: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }

        // Return path to saved tokenizer
        let tokenizer_path = config.output_dir.join("tokenizer_best.pt");
        Ok(tokenizer_path)
    }

    /// Finetune predictor
    pub fn finetune_predictor(&self, config: &FinetuneConfig, num_gpus: usize) -> Result<PathBuf> {
        let script = self.kronos_repo_path.join("finetune/train_predictor.py");
        
        if !script.exists() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Predictor training script not found: {}", script.display())
            ));
        }

        let mut cmd = Command::new(&self.python_executable);
        cmd.arg("-m")
            .arg("torch.distributed.run")
            .arg("--standalone")
            .arg("--nproc_per_node")
            .arg(num_gpus.to_string())
            .arg(&script)
            .env("PYTHONPATH", &self.kronos_repo_path)
            .current_dir(&self.kronos_repo_path);

        let output = cmd.output()
            .map_err(|e| crate::error::KronosAdapterError::Internal(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Predictor finetuning failed: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }

        // Return path to saved predictor
        let predictor_path = config.output_dir.join("predictor_best.pt");
        Ok(predictor_path)
    }

    /// Run backtesting
    pub fn backtest(&self, config: &FinetuneConfig, device: &str) -> Result<BacktestResult> {
        let script = self.kronos_repo_path.join("finetune/qlib_test.py");
        
        if !script.exists() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Backtest script not found: {}", script.display())
            ));
        }

        let mut cmd = Command::new(&self.python_executable);
        cmd.arg(&script)
            .arg("--device")
            .arg(device)
            .env("PYTHONPATH", &self.kronos_repo_path)
            .current_dir(&self.kronos_repo_path);

        let output = cmd.output()
            .map_err(|e| crate::error::KronosAdapterError::Internal(e.to_string()))?;

        if !output.status.success() {
            return Err(crate::error::KronosAdapterError::Internal(
                format!("Backtest failed: {}", String::from_utf8_lossy(&output.stderr))
            ));
        }

        // Parse backtest results from stdout
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_backtest_output(&stdout)
    }
}

/// Backtest result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub total_return: f64,
    pub annualized_return: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: f64,
    pub win_rate: f64,
    pub num_trades: usize,
}

/// Parse backtest output from Kronos qlib_test.py
fn parse_backtest_output(output: &str) -> Result<BacktestResult> {
    // Parse the output for key metrics
    // This is a simplified parser - real implementation would be more robust
    let mut result = BacktestResult {
        total_return: 0.0,
        annualized_return: 0.0,
        sharpe_ratio: 0.0,
        max_drawdown: 0.0,
        win_rate: 0.0,
        num_trades: 0,
    };

    for line in output.lines() {
        if line.contains("Total Return:") {
            if let Some(val) = line.split(':').nth(1) {
                result.total_return = val.trim().parse().unwrap_or(0.0);
            }
        } else if line.contains("Annualized Return:") {
            if let Some(val) = line.split(':').nth(1) {
                result.annualized_return = val.trim().parse().unwrap_or(0.0);
            }
        } else if line.contains("Sharpe Ratio:") {
            if let Some(val) = line.split(':').nth(1) {
                result.sharpe_ratio = val.trim().parse().unwrap_or(0.0);
            }
        } else if line.contains("Max Drawdown:") {
            if let Some(val) = line.split(':').nth(1) {
                result.max_drawdown = val.trim().parse().unwrap_or(0.0);
            }
        } else if line.contains("Win Rate:") {
            if let Some(val) = line.split(':').nth(1) {
                result.win_rate = val.trim().parse().unwrap_or(0.0);
            }
        } else if line.contains("Number of Trades:") {
            if let Some(val) = line.split(':').nth(1) {
                result.num_trades = val.trim().parse().unwrap_or(0);
            }
        }
    }

    Ok(result)
}