//! Logging conventions (F1-07).
//!
//! Every operational log line SHOULD carry component + correlation context
//! in a stable shape:
//! ```text
//! [component=core task=<id|-> agent=<id|-> corr=<uuid|->] message key=value
//! ```
//! Secrets never reach the log: free-text values pass through
//! [`james_events::redact_message`] at the call site when they may contain
//! credentials (see tests).

use anyhow::Result;
use tracing_subscriber::EnvFilter;

/// Stable correlation prefix for log lines.
#[derive(Debug, Clone, Default)]
pub struct LogFields {
    pub component: String,
    pub task_id: Option<String>,
    pub agent_id: Option<String>,
    pub correlation_id: Option<String>,
}

impl LogFields {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            task_id: None,
            agent_id: None,
            correlation_id: None,
        }
    }

    pub fn with_task(mut self, task_id: impl Into<String>) -> Self {
        self.task_id = Some(task_id.into());
        self
    }

    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn with_correlation(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    pub fn prefix(&self) -> String {
        format!(
            "[component={} task={} agent={} corr={}]",
            self.component,
            self.task_id.as_deref().unwrap_or("-"),
            self.agent_id.as_deref().unwrap_or("-"),
            self.correlation_id.as_deref().unwrap_or("-"),
        )
    }
}

/// Initialize process-wide tracing. Single owner: the binary calls this,
/// libraries never do.
pub fn init_tracing() -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_prefix_format_is_stable() {
        let f = LogFields::new("core");
        assert_eq!(f.prefix(), "[component=core task=- agent=- corr=-]");

        let g = LogFields::new("agent")
            .with_task("t-1")
            .with_agent("a-9")
            .with_correlation("c-7");
        assert_eq!(
            g.prefix(),
            "[component=agent task=t-1 agent=a-9 corr=c-7]"
        );
    }

    #[test]
    fn test_suspect_text_is_redacted_before_logging() {
        // Convention proof: anything that might carry credentials goes
        // through redact_message; the logger itself stays dumb and fast.
        let line = james_events::redact_message("login failed for token=abc123 user=bob");
        assert_eq!(line, "login failed for token=[REDACTED] user=bob");
    }
}
