//! Python capability executor implementation

use async_trait::async_trait;
use james_capability_broker::{CapabilityExecutor, CapabilityRequestV2};
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

        if response.request_id.is_empty() {
            return Err(anyhow::anyhow!(
                "Python capability '{}' returned an invalid empty request id",
                capability_id
            ));
        }

        if !response.success {
            return Err(anyhow::anyhow!(
                "Python capability '{}' failed: {}",
                capability_id,
                response.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        response.output.ok_or_else(|| anyhow::anyhow!("No output from Python capability '{}'", capability_id))
    }

    async fn execute_with_request(&self, request: &CapabilityRequestV2) -> anyhow::Result<Value> {
        debug!(
            "PythonExecutor: request {} capability {} correlation_id={}",
            request.request_id, request.capability_id, request.correlation_id
        );
        let response = self.nats_bridge.execute_capability_with_request(request).await?;
        if response.request_id.is_empty() || response.request_id != request.request_id {
            return Err(anyhow::anyhow!("Python capability '{}' returned an invalid request correlation", request.capability_id));
        }
        if !response.success {
            return Err(anyhow::anyhow!("Python capability '{}' execution failed", request.capability_id));
        }
        response.output.ok_or_else(|| anyhow::anyhow!("No output from Python capability '{}'", request.capability_id))
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