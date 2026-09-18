//! Python capability executor implementation

use async_trait::async_trait;
use james_capability_broker::CapabilityExecutor;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use crate::nats_bridge::NatsBridge;

/// Executor that forwards capability calls to Python via NATS
pub struct PythonExecutor {
    nats_bridge: Arc<NatsBridge>,
    default_caller: String,
}

impl PythonExecutor {
    pub fn new(nats_bridge: Arc<NatsBridge>) -> Self {
        Self {
            nats_bridge,
            default_caller: "python-bridge".to_string(),
        }
    }

    pub fn with_caller(mut self, caller: String) -> Self {
        self.default_caller = caller;
        self
    }
}

#[async_trait]
impl CapabilityExecutor for PythonExecutor {
    async fn execute(&self, capability_id: &str, input: Value) -> anyhow::Result<Value> {
        debug!("PythonExecutor: executing {} with caller {}", capability_id, self.default_caller);

        let response = self.nats_bridge
            .execute_capability(capability_id, &self.default_caller, input)
            .await?;

        if !response.success {
            return Err(anyhow::anyhow!(
                "Python capability '{}' failed: {}",
                capability_id,
                response.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        response.output.ok_or_else(|| anyhow::anyhow!("No output from Python capability '{}'", capability_id))
    }
}

/// Local executor for capabilities that run in Rust (for testing/fallback)
pub struct LocalExecutor;

#[async_trait]
impl CapabilityExecutor for LocalExecutor {
    async fn execute(&self, capability_id: &str, input: Value) -> anyhow::Result<Value> {
        debug!("LocalExecutor: executing {}", capability_id);

        // Echo back for testing
        Ok(serde_json::json!({
            "executed_by": "local",
            "capability": capability_id,
            "input": input
        }))
    }
}