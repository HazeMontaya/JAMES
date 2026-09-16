//! JAMES Audit — tamper-evident audit log (B3 minimal).
//!
//! The audit log is the permanent record of the enforcement chain (§7):
//! every permission decision, policy decision, capability execution and
//! claim forward lands here. Entries are chained: each entry's hash covers
//! the previous entry's hash, making silent alteration detectable.

use std::collections::VecDeque;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use james_events::{Event, EventBus, EventEnvelope};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Outcome of an audited action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum AuditOutcome {
    Allowed,
    Denied,
    Failed,
    Blocked,
    Ask,
    Unknown,
}

/// One audit entry in the chain.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditEntry {
    pub id: Uuid,
    /// Event type, e.g. `audit.capability.executed` or `audit.claim.rejected`.
    pub event_type: String,
    /// Source component that recorded the entry.
    pub source: String,
    /// Caller identity / module / agent id.
    pub subject: String,
    /// Capability or resource id involved.
    pub resource: String,
    pub outcome: AuditOutcome,
    /// Optional reason / human detail.
    pub detail: Option<String>,
    pub at: DateTime<Utc>,
    /// SHA-256 of this entry's fields (excluding `hash`).
    pub hash: String,
    /// SHA-256 of the previous entry's hash; the first entry uses a zero hash.
    pub prev_hash: String,
}

const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// In-memory tamper-evident audit log.
pub struct AuditLog {
    entries: RwLock<VecDeque<AuditEntry>>,
    by_event: DashMap<String, Vec<Uuid>>,
    event_bus: Option<Arc<EventBus>>,
    max_entries: usize,
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl AuditLog {
    /// Create a log bounded to `max_entries` in-memory entries.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RwLock::new(VecDeque::new()),
            by_event: DashMap::new(),
            event_bus: None,
            max_entries: max_entries.max(1),
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    fn field_digest(entry: &AuditEntry) -> String {
        let mut hasher = Sha256::new();
        hasher.update(entry.id.to_string().as_bytes());
        hasher.update(entry.event_type.as_bytes());
        hasher.update(entry.source.as_bytes());
        hasher.update(entry.subject.as_bytes());
        hasher.update(entry.resource.as_bytes());
        hasher.update(format!("{:?}", entry.outcome).as_bytes());
        if let Some(d) = &entry.detail {
            hasher.update(d.as_bytes());
        }
        hasher.update(entry.at.to_rfc3339().as_bytes());
        hasher.update(entry.prev_hash.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Append an entry to the chain. Returns the stored entry.
    pub async fn append(
        &self,
        event_type: impl Into<String>,
        source: impl Into<String>,
        subject: impl Into<String>,
        resource: impl Into<String>,
        outcome: AuditOutcome,
        detail: Option<String>,
    ) -> AuditEntry {
        let mut entries = self.entries.write().await;
        let prev_hash = entries.back().map(|e| e.hash.clone()).unwrap_or_else(|| ZERO_HASH.to_string());

        let mut entry = AuditEntry {
            id: Uuid::now_v7(),
            event_type: event_type.into(),
            source: source.into(),
            subject: subject.into(),
            resource: resource.into(),
            outcome,
            detail,
            at: Utc::now(),
            hash: String::new(),
            prev_hash,
        };
        entry.hash = Self::field_digest(&entry);
        let stored = entry.clone();
        entries.push_back(entry.clone());
        while entries.len() > self.max_entries {
            entries.pop_front();
        }
        drop(entries);

        self.by_event
            .entry(entry.event_type.clone())
            .or_default()
            .push(entry.id);

        if let Some(bus) = &self.event_bus {
            let event = Event::new("audit.entry", &entry.source).with_payload(serde_json::json!({
                "audit_id": entry.id,
                "event_type": entry.event_type,
                "subject": entry.subject,
                "resource": entry.resource,
                "outcome": format!("{:?}", entry.outcome),
                "hash": entry.hash,
                "prev_hash": entry.prev_hash,
                "at": entry.at.to_rfc3339(),
            }));
            let _ = bus.publish_envelope(EventEnvelope::new(event)).await;
        }

        entry
    }

    /// Convenience: log a raw event received from the identity module.
    pub async fn record_claim_forward(&self, event: &Event) {
        let subject = event
            .payload
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let resource = event
            .payload
            .get("capability")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let outcome = if event.payload.get("verified").and_then(|v| v.as_bool()).unwrap_or(false) {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        };
        let detail = event.payload.get("reason").and_then(|v| v.as_str()).map(str::to_string);
        self.append(
            event.event_type.clone(),
            event.source.clone(),
            subject,
            resource,
            outcome,
            detail,
        )
        .await;
    }

    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }

    /// All entries, newest first.
    pub async fn entries(&self) -> Vec<AuditEntry> {
        let entries = self.entries.read().await;
        entries.iter().rev().cloned().collect()
    }

    /// Entries of a given event type, newest first.
    pub async fn by_event_type(&self, event_type: &str) -> Vec<AuditEntry> {
        let ids: Vec<Uuid> = self.by_event.get(event_type).map(|v| v.clone()).unwrap_or_default();
        let entries = self.entries.read().await;
        entries
            .iter()
            .filter(|e| ids.contains(&e.id))
            .rev()
            .cloned()
            .collect()
    }

    /// Verify chain integrity from the oldest kept entry forward.
    pub async fn verify_chain(&self) -> bool {
        let entries = self.entries.read().await;
        let mut prev = ZERO_HASH.to_string();
        for e in entries.iter() {
            if e.prev_hash != prev {
                return false;
            }
            if Self::field_digest(e) != e.hash {
                return false;
            }
            prev = e.hash.clone();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_append_and_entries() {
        let log = AuditLog::new(100);
        log.append("audit.capability.executed", "broker", "void", "memory.read", AuditOutcome::Allowed, None).await;
        log.append("audit.claim.rejected", "identity", "unknown", "browser.navigate", AuditOutcome::Denied, Some("unknown identity".into())).await;

        assert_eq!(log.len().await, 2);
        let entries = log.entries().await;
        assert_eq!(entries[0].event_type, "audit.claim.rejected"); // newest first
        assert_eq!(entries[1].outcome, AuditOutcome::Allowed);

        let denied = log.by_event_type("audit.claim.rejected").await;
        assert_eq!(denied.len(), 1);
        assert_eq!(denied[0].detail.as_deref(), Some("unknown identity"));
    }

    #[tokio::test]
    async fn test_chain_integrity() {
        let log = AuditLog::new(100);
        log.append("a", "s", "sub1", "res1", AuditOutcome::Allowed, None).await;
        log.append("b", "s", "sub2", "res2", AuditOutcome::Denied, None).await;
        log.append("c", "s", "sub3", "res3", AuditOutcome::Failed, None).await;
        assert!(log.verify_chain().await);

        // Tamper: flip an outcome; chain verification must fail.
        {
            let mut entries = log.entries.write().await;
            let mut last = entries.back_mut().unwrap();
            last.outcome = AuditOutcome::Allowed;
        }
        assert!(!log.verify_chain().await);
    }

    #[tokio::test]
    async fn test_bounded_capacity() {
        let log = AuditLog::new(3);
        for i in 0..5 {
            log.append(format!("e{i}"), "s", "sub", "res", AuditOutcome::Unknown, None).await;
        }
        assert_eq!(log.len().await, 3);
    }

    #[tokio::test]
    async fn test_record_claim_forward() {
        let log = AuditLog::new(100);
        let event = Event::new("audit.claim.rejected", "james-identity").with_payload(serde_json::json!({
            "subject": "mod1",
            "capability": "browser.navigate",
            "verified": false,
            "reason": "denied by policy",
        }));
        log.record_claim_forward(&event).await;
        let entries = log.by_event_type("audit.claim.rejected").await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].subject, "mod1");
        assert_eq!(entries[0].resource, "browser.navigate");
        assert_eq!(entries[0].outcome, AuditOutcome::Denied);
    }
}