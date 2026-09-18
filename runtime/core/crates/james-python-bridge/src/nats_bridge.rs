//! NATS connection and message routing for the Python bridge

use async_nats::jetstream;
use async_nats::Client;
use futures::{SinkExt, StreamExt};
use james_events::{Event, EventBus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock, Mutex};
use tracing::{debug, error, info, warn};

use crate::config::BridgeConfig;

/// Subject patterns for bridge communication
pub mod subjects {
    use crate::config::BridgeConfig;

    pub fn capability_execute_prefix(config: &BridgeConfig) -> String {
        format!("{}.capability.execute", config.subject_prefix)
    }

    pub fn capability_execute_reply(config: &BridgeConfig) -> String {
        format!("{}.capability.execute.*.reply", config.subject_prefix)
    }

    pub fn capability_register(config: &BridgeConfig) -> String {
        format!("{}.capability.register", config.subject_prefix)
    }

    pub fn capability_list(config: &BridgeConfig) -> String {
        format!("{}.capability.list", config.subject_prefix)
    }

    pub fn health_check(config: &BridgeConfig) -> String {
        format!("{}.health", config.subject_prefix)
    }

    pub fn event_forward(config: &BridgeConfig) -> String {
        format!("{}.events.>", config.subject_prefix)
    }
}

/// Request to execute a capability from Python
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityExecuteRequest {
    pub request_id: String,
    pub capability_id: String,
    pub caller: String,
    pub input: serde_json::Value,
}

/// Response from Python capability execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityExecuteResponse {
    pub request_id: String,
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Python capability registration info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonCapabilityInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub version: String,
    pub description: String,
    pub required_permissions: Vec<String>,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

/// Health check payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub service: String,
    pub status: String,
    pub timestamp: String,
    pub capabilities: Vec<String>,
}

/// NATS Bridge - manages NATS connection and message routing
pub struct NatsBridge {
    config: BridgeConfig,
    client: Arc<Mutex<Option<Client>>>,
    jetstream: Option<jetstream::Context>,
    event_bus: Option<Arc<EventBus>>,
    pending_requests: Arc<RwLock<dashmap::DashMap<String, mpsc::Sender<CapabilityExecuteResponse>>>>,
    python_capabilities: Arc<RwLock<Vec<PythonCapabilityInfo>>>,
    shutdown_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
}

impl NatsBridge {
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            config,
            client: Arc::new(Mutex::new(None)),
            jetstream: None,
            event_bus: None,
            pending_requests: Arc::new(RwLock::new(dashmap::DashMap::new())),
            python_capabilities: Arc::new(RwLock::new(Vec::new())),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Connect to NATS and set up subscriptions
    pub async fn connect(&mut self) -> anyhow::Result<()> {
        info!("Connecting to NATS at {}", self.config.nats_url);

        let client = async_nats::connect(&self.config.nats_url).await?;
        let jetstream = jetstream::new(client.clone());

        {
            let mut client_guard = self.client.lock().await;
            *client_guard = Some(client.clone());
        }
        self.jetstream = Some(jetstream.clone());

        // Set up subscriptions
        self.subscribe_capability_execute().await?;
        self.subscribe_capability_register().await?;
        self.subscribe_capability_list().await?;
        self.subscribe_health_check().await?;
        self.subscribe_event_forward().await?;

        // Start health check publisher
        self.start_health_publisher().await?;

        info!("NATS bridge connected successfully");
        Ok(())
    }

    async fn subscribe_capability_execute(&self) -> anyhow::Result<()> {
        // Execution requests are sent with NATS request/reply by execute_capability().
        // This subscription is intentionally limited to the reply wildcard so the
        // bridge never consumes Python execution requests itself.
        let client = {
            let guard = self.client.lock().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("NATS client not connected"))?
                .clone()
        };
        let subject = subjects::capability_execute_reply(&self.config);
        let mut subscriber = client.subscribe(subject).await?;

        let pending = self.pending_requests.clone();

        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                let Ok(response) = serde_json::from_slice::<CapabilityExecuteResponse>(&msg.payload) else {
                    warn!("Ignoring malformed Python capability response");
                    continue;
                };

                let sender = {
                    let pending_guard = pending.read().await;
                    pending_guard.get(&response.request_id).map(|entry| entry.value().clone())
                };

                if let Some(sender) = sender {
                    let _ = sender.send(response).await;
                } else {
                    debug!("No pending request for capability response {}", response.request_id);
                }
            }
        });

        Ok(())
    }

    async fn subscribe_capability_register(&self) -> anyhow::Result<()> {
        let client = {
            let guard = self.client.lock().await;
            guard.as_ref()
                .ok_or_else(|| anyhow::anyhow!("NATS client not connected"))?
                .clone()
        };
        let subject = subjects::capability_register(&self.config);
        let mut subscriber = client.subscribe(subject).await?;

        let python_caps = self.python_capabilities.clone();

        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                if let Ok(cap_info) = serde_json::from_slice::<PythonCapabilityInfo>(&msg.payload) {
                    info!("Registering Python capability: {}", cap_info.id);
                    let mut caps = python_caps.write().await;
                    caps.retain(|c| c.id != cap_info.id);
                    caps.push(cap_info);
                }
            }
        });

        Ok(())
    }

    async fn subscribe_capability_list(&self) -> anyhow::Result<()> {
        let client = {
            let guard = self.client.lock().await;
            guard.as_ref()
                .ok_or_else(|| anyhow::anyhow!("NATS client not connected"))?
                .clone()
        };
        let subject = subjects::capability_list(&self.config);
        let mut subscriber = client.subscribe(subject).await?;

        let python_caps = self.python_capabilities.clone();
        let client = client.clone();

        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                let caps = python_caps.read().await;
                let list: Vec<_> = caps.iter().map(|c| c.id.clone()).collect();
                let response = serde_json::json!({ "capabilities": list });
                if let Some(reply) = msg.reply {
                    if let Ok(bytes) = serde_json::to_vec(&response) {
                        let _ = client.publish(reply, bytes.into()).await;
                    }
                }
            }
        });

        Ok(())
    }

    async fn subscribe_health_check(&self) -> anyhow::Result<()> {
        let client = {
            let guard = self.client.lock().await;
            guard.as_ref()
                .ok_or_else(|| anyhow::anyhow!("NATS client not connected"))?
                .clone()
        };
        let subject = subjects::health_check(&self.config);
        let mut subscriber = client.subscribe(subject).await?;

        let python_caps = self.python_capabilities.clone();
        let config = self.config.clone();
        let client = client.clone();

        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                let caps = python_caps.read().await;
                let health = HealthCheck {
                    service: config.service_name.clone(),
                    status: "healthy".to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    capabilities: caps.iter().map(|c| c.id.clone()).collect(),
                };
                if let Some(reply) = msg.reply {
                    if let Ok(bytes) = serde_json::to_vec(&health) {
                        let _ = client.publish(reply, bytes.into()).await;
                    }
                }
            }
        });

        Ok(())
    }

    async fn subscribe_event_forward(&self) -> anyhow::Result<()> {
        let client = {
            let guard = self.client.lock().await;
            guard.as_ref()
                .ok_or_else(|| anyhow::anyhow!("NATS client not connected"))?
                .clone()
        };
        let subject = subjects::event_forward(&self.config);
        let mut subscriber = client.subscribe(subject).await?;

        let event_bus = self.event_bus.clone();

        tokio::spawn(async move {
            while let Some(msg) = subscriber.next().await {
                if let Ok(event) = serde_json::from_slice::<Event>(&msg.payload) {
                    if let Some(bus) = &event_bus {
                        let _ = bus.publish(event).await;
                    }
                }
            }
        });

        Ok(())
    }

async fn start_health_publisher(&self) -> anyhow::Result<()> {
        let client = {
            let guard = self.client.lock().await;
            guard.as_ref()
                .ok_or_else(|| anyhow::anyhow!("NATS client not connected"))?
                .clone()
        };
        let config = self.config.clone();
        let python_caps = self.python_capabilities.clone();
        let subject = subjects::health_check(&config);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config.health_check_interval_secs));
            loop {
                interval.tick().await;
                let caps = python_caps.read().await;
                let health = HealthCheck {
                    service: config.service_name.clone(),
                    status: "healthy".to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    capabilities: caps.iter().map(|c| c.id.clone()).collect(),
                };
                if let Ok(bytes) = serde_json::to_vec(&health) {
                    let _ = client.publish(subject.clone(), bytes.into()).await;
                }
            }
        });

        Ok(())
    }

    /// Execute a capability via Python (request/reply)
    pub async fn execute_capability(
        &self,
        capability_id: &str,
        caller: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<CapabilityExecuteResponse> {
        let client = {
            let guard = self.client.lock().await;
            guard.as_ref()
                .ok_or_else(|| anyhow::anyhow!("NATS client not connected"))?
                .clone()
        };

        let request_id = uuid::Uuid::now_v7().to_string();
        let request = CapabilityExecuteRequest {
            request_id: request_id.clone(),
            capability_id: capability_id.to_string(),
            caller: caller.to_string(),
            input,
        };

        let subject = format!("{}.{}", subjects::capability_execute_prefix(&self.config), capability_id);

        let payload = serde_json::to_vec(&request)?;
        let response_message = tokio::time::timeout(
            Duration::from_secs(self.config.request_timeout_secs),
            client.request(subject, payload.into()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Request timeout"))??;

        let response: CapabilityExecuteResponse = serde_json::from_slice(&response_message.payload)
            .map_err(|e| anyhow::anyhow!("Invalid Python capability response: {}", e))?;

        Ok(response)

    /// Get list of registered Python capabilities
    pub async fn list_python_capabilities(&self) -> Vec<PythonCapabilityInfo> {
        self.python_capabilities.read().await.clone()
    }

    /// Shutdown the bridge
    pub async fn shutdown(&self) {
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(()).await;
        }
        if let Some(mut client) = self.client.lock().await.take() {
            let _ = client.close().await;
        }
    }
}