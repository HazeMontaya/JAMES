use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EventSeverity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: Uuid,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub source: String,
    pub target: Option<String>,
    pub payload: serde_json::Value,
    pub correlation_id: Option<Uuid>,
    pub causation_id: Option<Uuid>,
    pub severity: EventSeverity,
}

impl Event {
    pub fn new(event_type: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            event_type: event_type.into(),
            timestamp: Utc::now(),
            source: source.into(),
            target: None,
            payload: serde_json::Value::Null,
            correlation_id: None,
            causation_id: None,
            severity: EventSeverity::Info,
        }
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_causation_id(mut self, causation_id: Uuid) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    pub fn with_severity(mut self, severity: EventSeverity) -> Self {
        self.severity = severity;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event: Event,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl EventEnvelope {
    pub fn new(event: Event) -> Self {
        Self {
            event,
            metadata: HashMap::new(),
        }
    }
}

pub mod builtin_events {
    pub const SYSTEM_STARTED: &str = "system.started";
    pub const SYSTEM_STOPPING: &str = "system.stopping";
    pub const SYSTEM_STOPPED: &str = "system.stopped";

    pub const DEVICE_DISCOVERED: &str = "device.discovered";
    pub const DEVICE_CHANGED: &str = "device.changed";
    pub const DEVICE_REMOVED: &str = "device.removed";

    pub const CAPABILITY_REGISTERED: &str = "capability.registered";
    pub const CAPABILITY_REMOVED: &str = "capability.removed";
    pub const CAPABILITY_CHANGED: &str = "capability.changed";

    pub const PLUGIN_LOADED: &str = "plugin.loaded";
    pub const PLUGIN_UNLOADED: &str = "plugin.unloaded";
    pub const PLUGIN_ERROR: &str = "plugin.error";

    pub const TASK_CREATED: &str = "task.created";
    pub const TASK_STARTED: &str = "task.started";
    pub const TASK_COMPLETED: &str = "task.completed";
    pub const TASK_FAILED: &str = "task.failed";
    pub const TASK_CANCELLED: &str = "task.cancelled";

    pub const WORKFLOW_STARTED: &str = "workflow.started";
    pub const WORKFLOW_COMPLETED: &str = "workflow.completed";
    pub const WORKFLOW_FAILED: &str = "workflow.failed";

    pub const SERVICE_STARTED: &str = "service.started";
    pub const SERVICE_STOPPED: &str = "service.stopped";
    pub const SERVICE_FAILED: &str = "service.failed";
    pub const SERVICE_HEALTH_CHANGED: &str = "service.health_changed";

    pub const ERROR_OCCURRED: &str = "error.occurred";

    pub const DISCOVERY_SCAN_STARTED: &str = "discovery.scan_started";
    pub const DISCOVERY_SCAN_COMPLETED: &str = "discovery.scan_completed";
    pub const DISCOVERY_CHANGES_DETECTED: &str = "discovery.changes_detected";

    pub const CONFIG_CHANGED: &str = "config.changed";
    pub const REGISTRY_CHANGED: &str = "registry.changed";
}

pub fn create_system_event(event_type: &str, source: &str) -> Event {
    Event::new(event_type, source)
        .with_severity(EventSeverity::Info)
}

pub fn create_error_event(source: &str, error: &str, context: Option<serde_json::Value>) -> Event {
    let mut payload = serde_json::json!({ "error": error });
    if let Some(ctx) = context {
        payload["context"] = ctx;
    }
    Event::new(builtin_events::ERROR_OCCURRED, source)
        .with_payload(payload)
        .with_severity(EventSeverity::Error)
}

use tokio::sync::mpsc;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

pub struct EventBus {
    buffer_size: usize,
    subscribers: Arc<DashMap<String, Vec<mpsc::UnboundedSender<EventEnvelope>>>>,
    running: Arc<RwLock<bool>>,
}

impl EventBus {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer_size,
            subscribers: Arc::new(DashMap::new()),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn with_default_buffer() -> Self {
        Self::new(1000)
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        Ok(())
    }

    pub fn subscribe(&self, event_type: impl Into<String>) -> mpsc::UnboundedReceiver<EventEnvelope> {
        let (tx, rx) = mpsc::unbounded_channel();
        let event_type = event_type.into();
        
        self.subscribers
            .entry(event_type)
            .or_default()
            .push(tx);
        
        rx
    }

    pub fn subscribe_all(&self) -> mpsc::UnboundedReceiver<EventEnvelope> {
        let (tx, rx) = mpsc::unbounded_channel();
        
        self.subscribers
            .entry("*".to_string())
            .or_default()
            .push(tx);
        
        rx
    }

    pub async fn publish(&self, event: Event) -> Result<()> {
        if !*self.running.read().await {
            return Ok(());
        }

        let envelope = EventEnvelope::new(event.clone());
        let event_type = &event.event_type;

        if let Some(subscribers) = self.subscribers.get(event_type) {
            for tx in subscribers.iter() {
                let _ = tx.send(envelope.clone());
            }
        }

        if let Some(subscribers) = self.subscribers.get("*") {
            for tx in subscribers.iter() {
                let _ = tx.send(envelope.clone());
            }
        }

        Ok(())
    }

    pub async fn publish_envelope(&self, envelope: EventEnvelope) -> Result<()> {
        if !*self.running.read().await {
            return Ok(());
        }

        let event_type = &envelope.event.event_type;

        if let Some(subscribers) = self.subscribers.get(event_type) {
            for tx in subscribers.iter() {
                let _ = tx.send(envelope.clone());
            }
        }

        if let Some(subscribers) = self.subscribers.get("*") {
            for tx in subscribers.iter() {
                let _ = tx.send(envelope.clone());
            }
        }

        Ok(())
    }

    pub async fn publish_batch(&self, events: Vec<Event>) -> Result<()> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }
}