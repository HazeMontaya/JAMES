use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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

fn default_event_version() -> u32 {
    1
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
    /// Schema version of this event type. Old payloads without the field
    /// deserialize as version 1.
    #[serde(default = "default_event_version")]
    pub version: u32,
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
            version: 1,
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

    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
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

    // Legacy plugin.* names: kept as deprecated aliases, do not emit.
    #[deprecated(note = "emit module.* instead")]
    pub const PLUGIN_LOADED: &str = "plugin.loaded";
    #[deprecated(note = "emit module.* instead")]
    pub const PLUGIN_UNLOADED: &str = "plugin.unloaded";
    #[deprecated(note = "emit module.* instead")]
    pub const PLUGIN_ERROR: &str = "plugin.error";

    pub const MODULE_DISCOVERED: &str = "module.discovered";
    pub const MODULE_LOADED: &str = "module.loaded";
    pub const MODULE_UNLOADED: &str = "module.unloaded";
    pub const MODULE_FAILED: &str = "module.failed";

    pub const TASK_CREATED: &str = "task.created";
    pub const TASK_STARTED: &str = "task.started";
    pub const TASK_COMPLETED: &str = "task.completed";
    pub const TASK_FAILED: &str = "task.failed";
    pub const TASK_CANCELLED: &str = "task.cancelled";
    pub const TASK_TIMEOUT: &str = "task.timeout";

    pub const TOOL_EXECUTION_STARTED: &str = "tool.execution_started";
    pub const TOOL_EXECUTION_COMPLETED: &str = "tool.execution_completed";
    pub const TOOL_EXECUTION_FAILED: &str = "tool.execution_failed";

    pub const APPROVAL_REQUESTED: &str = "approval.requested";
    pub const APPROVAL_DECIDED: &str = "approval.decided";

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

    pub const MEMORY_UPDATED: &str = "memory.updated";

    pub const AUDIT_RECORDED: &str = "audit.recorded";

    pub const UI_CONNECTED: &str = "ui.connected";
    pub const UI_DISCONNECTED: &str = "ui.disconnected";

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

/// Payload keys whose values must never leave the process in clear text.
/// Matching is case-insensitive substring on the key.
const SENSITIVE_KEY_PARTS: &[&str] = &[
    "password", "passwd", "secret", "token", "apikey", "api_key", "credential",
    "bearer", "authorization", "private_key", "client_secret", "session_key",
];

const REDACTED: &str = "[REDACTED]";

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEY_PARTS.iter().any(|part| lower.contains(part))
}

fn is_sensitive_value(value: &str) -> bool {
    value.starts_with("sk-") || value.to_ascii_lowercase().contains("bearer ")
}

/// Recursively redact secrets from an event payload in place.
pub fn scrub_payload(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if is_sensitive_key(k) {
                    *v = serde_json::Value::String(REDACTED.to_string());
                } else {
                    scrub_payload(v);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                scrub_payload(item);
            }
        }
        serde_json::Value::String(s) if is_sensitive_value(s) => {
            *s = REDACTED.to_string();
        }
        _ => {}
    }
}

use tokio::sync::mpsc;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

#[derive(Debug)]
struct SubscriptionEntry {
    id: u64,
    tx: mpsc::Sender<EventEnvelope>,
}

pub struct EventBus {
    buffer_size: usize,
    next_id: AtomicU64,
    subscribers: Arc<DashMap<String, Vec<SubscriptionEntry>>>,
    dropped: Arc<AtomicUsize>,
    running: Arc<RwLock<bool>>,
}

impl EventBus {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            // tokio bounded channels panic on capacity 0; clamp honestly.
            buffer_size: buffer_size.max(1),
            next_id: AtomicU64::new(1),
            subscribers: Arc::new(DashMap::new()),
            dropped: Arc::new(AtomicUsize::new(0)),
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

    pub fn is_running(&self) -> bool {
        // Best-effort read for diagnostics; lifecycle decisions use start/stop.
        self.running.try_read().map(|g| *g).unwrap_or(false)
    }

    /// Subscribe; the returned receiver is bounded (`buffer_size`).
    /// Slow consumers drop newest events (see `dropped_count`).
    pub fn subscribe(&self, event_type: impl Into<String>) -> mpsc::Receiver<EventEnvelope> {
        self.subscribe_with_id(event_type).1
    }

    /// Subscribe and keep the id for a later `unsubscribe`.
    pub fn subscribe_with_id(
        &self,
        event_type: impl Into<String>,
    ) -> (u64, mpsc::Receiver<EventEnvelope>) {
        let (tx, rx) = mpsc::channel(self.buffer_size);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.subscribers
            .entry(event_type.into())
            .or_default()
            .push(SubscriptionEntry { id, tx });
        (id, rx)
    }

    pub fn subscribe_all(&self) -> mpsc::Receiver<EventEnvelope> {
        self.subscribe("*")
    }

    /// Remove one subscription. Returns true if it existed.
    /// Dead receivers are also pruned opportunistically on publish.
    pub fn unsubscribe(&self, event_type: &str, id: u64) -> bool {
        let mut removed = false;
        if let Some(mut subs) = self.subscribers.get_mut(event_type) {
            let before = subs.len();
            subs.retain(|s| s.id != id);
            removed = subs.len() != before;
        }
        removed
    }

    /// Events dropped because a subscriber lagged behind (bounded buffer full).
    pub fn dropped_count(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn subscriber_count(&self, event_type: &str) -> usize {
        self.subscribers.get(event_type).map(|s| s.len()).unwrap_or(0)
    }

    fn ensure_running(&self) -> Result<()> {
        // Blocking check: publish paths must not silently swallow events.
        let running = self.running.try_read().map(|g| *g).unwrap_or(false);
        if running {
            Ok(())
        } else {
            anyhow::bail!("event bus is stopped; event would be lost")
        }
    }

    fn fanout(&self, envelope: &EventEnvelope, event_type: &str) {
        // Exact-type subscribers, then wildcard subscribers
        // (unless the event itself targets "*").
        let mut keys = vec![event_type];
        if event_type != "*" {
            keys.push("*");
        }
        for key in keys {
            if let Some(mut subs) = self.subscribers.get_mut(key) {
                Self::send_all(&mut subs, envelope, &self.dropped);
            }
        }
    }

    fn send_all(
        subs: &mut Vec<SubscriptionEntry>,
        envelope: &EventEnvelope,
        dropped: &AtomicUsize,
    ) {
        let mut closed = false;
        for sub in subs.iter() {
            match sub.tx.try_send(envelope.clone()) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    dropped.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!("event bus: subscriber {} lagging, event dropped", sub.id);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    closed = true;
                }
            }
        }
        if closed {
            subs.retain(|s| !s.tx.is_closed());
        }
    }

    pub async fn publish(&self, mut event: Event) -> Result<()> {
        self.ensure_running()?;
        scrub_payload(&mut event.payload);
        let event_type = event.event_type.clone();
        let envelope = EventEnvelope::new(event);
        self.fanout(&envelope, &event_type);
        Ok(())
    }

    pub async fn publish_envelope(&self, mut envelope: EventEnvelope) -> Result<()> {
        self.ensure_running()?;
        scrub_payload(&mut envelope.event.payload);
        let event_type = envelope.event.event_type.clone();
        self.fanout(&envelope, &event_type);
        Ok(())
    }

    pub async fn publish_batch(&self, events: Vec<Event>) -> Result<()> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_version_defaults_to_one() {
        let event = Event::new("test.event", "test");
        assert_eq!(event.version, 1);

        // Old payloads without the field deserialize as version 1.
        let legacy = serde_json::json!({
            "event_id": Uuid::now_v7(),
            "event_type": "test.event",
            "timestamp": Utc::now(),
            "source": "test",
            "target": null,
            "payload": null,
            "correlation_id": null,
            "causation_id": null,
            "severity": "Info",
        });
        let parsed: Event = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.version, 1);
    }

    #[tokio::test]
    async fn test_publish_fails_when_stopped() {
        let bus = EventBus::new(100);
        // Never started: publish must Err, never silently Ok.
        assert!(bus.publish(Event::new("test.event", "test")).await.is_err());
        assert!(bus
            .publish_envelope(EventEnvelope::new(Event::new("test.event", "test")))
            .await
            .is_err());

        bus.start().await.unwrap();
        assert!(bus.publish(Event::new("test.event", "test")).await.is_ok());
        bus.stop().await.unwrap();
        assert!(bus.publish(Event::new("test.event", "test")).await.is_err());
    }

    #[tokio::test]
    async fn test_unsubscribe_removes_subscription() {
        let bus = EventBus::new(100);
        bus.start().await.unwrap();

        let (id, mut rx) = bus.subscribe_with_id("test.event");
        assert_eq!(bus.subscriber_count("test.event"), 1);
        assert!(bus.unsubscribe("test.event", id));
        assert_eq!(bus.subscriber_count("test.event"), 0);
        assert!(!bus.unsubscribe("test.event", id));

        bus.publish(Event::new("test.event", "test")).await.unwrap();
        // Closed receiver after unsubscribe: recv returns None.
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn test_bounded_backpressure_counts_drops() {
        let bus = EventBus::new(1);
        bus.start().await.unwrap();
        let mut rx = bus.subscribe("test.event");

        bus.publish(Event::new("test.event", "test")).await.unwrap();
        bus.publish(Event::new("test.event", "test")).await.unwrap();
        bus.publish(Event::new("test.event", "test")).await.unwrap();

        // Buffer holds 1; the other two overflows are counted, not stored.
        assert_eq!(bus.dropped_count(), 2);
        assert!(rx.recv().await.is_some());
    }

    #[tokio::test]
    async fn test_scrub_redacts_secrets() {
        let bus = EventBus::new(100);
        bus.start().await.unwrap();
        let mut rx = bus.subscribe("test.event");

        let event = Event::new("test.event", "test").with_payload(serde_json::json!({
            "username": "alice",
            "password": "hunter2",
            "apiKey": "sk-xt-secret",
            "nested": { "token": "abc", "ok": 1 },
            "list": [{ "bearer": "x" }, { "fine": true }],
        }));
        bus.publish(event).await.unwrap();

        let received = rx.recv().await.unwrap();
        let p = &received.event.payload;
        assert_eq!(p["username"], "alice");
        assert_eq!(p["password"], "[REDACTED]");
        assert_eq!(p["apiKey"], "[REDACTED]");
        assert_eq!(p["nested"]["token"], "[REDACTED]");
        assert_eq!(p["nested"]["ok"], 1);
        assert_eq!(p["list"][0]["bearer"], "[REDACTED]");
        assert!(p["list"][1]["fine"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_dead_receivers_are_pruned() {
        let bus = EventBus::new(100);
        bus.start().await.unwrap();
        {
            let _rx = bus.subscribe("test.event");
            assert_eq!(bus.subscriber_count("test.event"), 1);
        }
        // Receiver dropped: next publish prunes the dead sender.
        bus.publish(Event::new("test.event", "test")).await.unwrap();
        assert_eq!(bus.subscriber_count("test.event"), 0);
    }
}
