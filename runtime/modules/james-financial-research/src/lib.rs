//! JAMES Financial Research Orchestrator
//!
//! Orchestrates technical analysis, news sentiment, fundamentals, and Kronos forecasts
//! into synthesized research reports.

pub mod config;
pub mod error;
pub mod indicators;
pub mod orchestrator;
pub mod sentiment;
pub mod fundamentals;
pub mod synthesis;

pub use config::ResearchConfig;
pub use error::{ResearchError, Result};
pub use orchestrator::ResearchOrchestrator;
pub use indicators::TechnicalIndicators;
pub use sentiment::NewsSentimentEngine;
pub use fundamentals::FundamentalsEngine;
pub use synthesis::ResearchSynthesizer;