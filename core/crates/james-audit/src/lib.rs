//! JAMES Audit — tamper-evident audit log (B3 minimal).
//!
//! The audit log is the permanent record of the enforcement chain (§7):
//! every permission decision, policy decision, capability execution and
//! claim forward lands here. Entries are chained: each entry's hash covers
//! the previous entry's hash, making silent alteration detectable.
//!
//! `PersistentAuditLog` wraps `AuditLog` and adds append-only JSONL file
//! persistence so entries survive process restarts.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use james_events::{Event, EventBus, EventEnvelope};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::info;
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
    pub(crate) entries: RwLock<VecDeque<AuditEntry>>,
    pub(crate) by_event: DashMap<String, Vec<Uuid>>,
    event_bus: Option<Arc<EventBus>>,
    pub(crate) max_entries: usize,
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

// ---------------------------------------------------------------------------
// PersistentAuditLog — JSONL file-backed audit log
// ---------------------------------------------------------------------------

/// Configuration for the persistent audit log.
#[derive(Debug, Clone)]
pub struct PersistentAuditConfig {
    /// Path to the JSONL file where entries are appended.
    pub file_path: PathBuf,
    /// Maximum number of in-memory entries (ring buffer).
    pub max_entries: usize,
    /// Flush to disk on every append (true) or only on explicit save (false).
    pub flush_on_append: bool,
}

impl PersistentAuditConfig {
    pub fn new(file_path: impl Into<PathBuf>) -> Self {
        Self {
            file_path: file_path.into(),
            max_entries: 10_000,
            flush_on_append: true,
        }
    }

    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    pub fn with_flush_on_append(mut self, flush: bool) -> Self {
        self.flush_on_append = flush;
        self
    }
}

/// Audit log backed by an append-only JSONL file.
///
/// On creation, `load_from_disk` replays the file into the in-memory ring.
/// On every `append`, the new entry is appended to the file (and optionally
/// flushed). The chain hash covers the previous entry's hash so the file is
/// tamper-evident: flipping a line breaks the chain on next load.
pub struct PersistentAuditLog {
    inner: AuditLog,
    config: PersistentAuditConfig,
}

impl PersistentAuditLog {
    /// Create a new persistent log. Loads existing entries from disk first.
    pub async fn open(config: PersistentAuditConfig) -> Result<Self> {
        let inner = AuditLog::new(config.max_entries);
        let log = Self { inner, config };
        log.load_from_disk().await?;
        let count = log.inner.len().await;
        info!(
            "PersistentAuditLog opened at {} with {} entries",
            log.config.file_path.display(),
            count,
        );
        Ok(log)
    }

    /// Create with an event bus; entries are published as they arrive.
    pub async fn open_with_event_bus(
        config: PersistentAuditConfig,
        event_bus: Arc<EventBus>,
    ) -> Result<Self> {
        let inner = AuditLog::new(config.max_entries).with_event_bus(event_bus);
        let log = Self { inner, config };
        log.load_from_disk().await?;
        let count = log.inner.len().await;
        info!(
            "PersistentAuditLog opened at {} with {} entries",
            log.config.file_path.display(),
            count,
        );
        Ok(log)
    }

    /// Append an entry, also writing it to the JSONL file.
    pub async fn append(
        &self,
        event_type: impl Into<String>,
        source: impl Into<String>,
        subject: impl Into<String>,
        resource: impl Into<String>,
        outcome: AuditOutcome,
        detail: Option<String>,
    ) -> AuditEntry {
        let entry = self
            .inner
            .append(event_type, source, subject, resource, outcome, detail)
            .await;

        if self.config.flush_on_append {
            if let Err(e) = self.append_to_file(&entry).await {
                tracing::error!("failed to persist audit entry: {e}");
            }
        }

        entry
    }

    /// Record a claim-forward event (convenience, mirrors AuditLog).
    pub async fn record_claim_forward(&self, event: &Event) {
        self.inner.record_claim_forward(event).await;
        if self.config.flush_on_append {
            if let Some(entry) = self.inner.entries.read().await.front() {
                let _ = self.append_to_file(entry).await;
            }
        }
    }

    /// Flush all current in-memory entries to disk (full rewrite).
    pub async fn flush(&self) -> Result<()> {
        let entries = self.inner.entries.read().await;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.config.file_path)
            .await?;
        for entry in entries.iter() {
            let line = serde_json::to_string(entry)?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }
        file.flush().await?;
        Ok(())
    }

    /// The number of entries currently held in memory.
    pub async fn len(&self) -> usize {
        self.inner.len().await
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.is_empty().await
    }

    /// All entries (newest first).
    pub async fn entries(&self) -> Vec<AuditEntry> {
        self.inner.entries().await
    }

    /// Entries by event type.
    pub async fn by_event_type(&self, event_type: &str) -> Vec<AuditEntry> {
        self.inner.by_event_type(event_type).await
    }

    /// Verify chain integrity.
    pub async fn verify_chain(&self) -> bool {
        self.inner.verify_chain().await
    }

    // -- internal helpers --------------------------------------------------

    /// Append a single entry as a JSON line.
    async fn append_to_file(&self, entry: &AuditEntry) -> Result<()> {
        if let Some(parent) = self.config.file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.file_path)
            .await?;
        let line = serde_json::to_string(entry)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }

    /// Replay entries from the JSONL file into the in-memory ring.
    async fn load_from_disk(&self) -> Result<()> {
        let path = &self.config.file_path;
        if !path.exists() {
            return Ok(());
        }
        let file = tokio::fs::File::open(path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut loaded = 0u64;
        let mut entries = self.inner.entries.write().await;

        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<AuditEntry>(&line) {
                Ok(entry) => {
                    // Verify chain link for this entry.
                    if let Some(last) = entries.back() {
                        if entry.prev_hash != last.hash {
                            tracing::warn!(
                                "audit chain break at entry {} — prev_hash mismatch, ignoring tail",
                                entry.id,
                            );
                            break;
                        }
                    }
                    if AuditLog::field_digest(&entry) != entry.hash {
                        tracing::warn!(
                            "audit entry {} has invalid hash — tamper detected, ignoring",
                            entry.id,
                        );
                        break;
                    }
                    entries.push_back(entry);
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!("failed to parse audit line: {e}");
                }
            }
        }

        // Enforce max_entries after load.
        while entries.len() > self.inner.max_entries {
            entries.pop_front();
        }

        // Rebuild by_event index.
        drop(entries);
        let entries_guard = self.inner.entries.read().await;
        for entry in entries_guard.iter() {
            self.inner
                .by_event
                .entry(entry.event_type.clone())
                .or_default()
                .push(entry.id);
        }

        info!("loaded {loaded} audit entries from {}", path.display());
        Ok(())
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

    // ---- PersistentAuditLog tests ----

    #[tokio::test]
    async fn test_persistent_audit_append_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        // Create, append, drop.
        {
            let config = PersistentAuditConfig::new(path.clone());
            let log = PersistentAuditLog::open(config).await.unwrap();
            log.append("audit.test", "src", "subject1", "res1", AuditOutcome::Allowed, None)
                .await;
            log.append("audit.test", "src", "subject2", "res2", AuditOutcome::Denied, Some("reason".into()))
                .await;
            log.flush().await.unwrap();
            assert_eq!(log.len().await, 2);
        }

        // Re-open: entries should be loaded from disk.
        {
            let config = PersistentAuditConfig::new(path.clone());
            let log = PersistentAuditLog::open(config).await.unwrap();
            assert_eq!(log.len().await, 2);
            let entries = log.entries().await;
            assert_eq!(entries[0].subject, "subject2"); // newest first
            assert_eq!(entries[1].subject, "subject1");
            assert!(log.verify_chain().await);
        }
    }

    #[tokio::test]
    async fn test_persistent_audit_chain_integrity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        {
            let config = PersistentAuditConfig::new(path.clone());
            let log = PersistentAuditLog::open(config).await.unwrap();
            for i in 0..5 {
                log.append(
                    format!("audit.{i}"),
                    "s",
                    format!("sub{i}"),
                    format!("res{i}"),
                    AuditOutcome::Allowed,
                    None,
                )
                .await;
            }
            log.flush().await.unwrap();
        }

        // Re-open and verify chain integrity holds.
        {
            let config = PersistentAuditConfig::new(path.clone());
            let log = PersistentAuditLog::open(config).await.unwrap();
            assert!(log.verify_chain().await);
            assert_eq!(log.len().await, 5);
        }
    }

    #[tokio::test]
    async fn test_persistent_audit_tamper_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        // Write 3 entries.
        {
            let config = PersistentAuditConfig::new(path.clone());
            let log = PersistentAuditLog::open(config).await.unwrap();
            for i in 0..3 {
                log.append(
                    format!("audit.{i}"),
                    "s",
                    format!("sub{i}"),
                    "res",
                    AuditOutcome::Allowed,
                    None,
                )
                .await;
            }
            log.flush().await.unwrap();
        }

        // Tamper with the file: change a line.
        {
            let mut content = tokio::fs::read_to_string(&path).await.unwrap();
            content = content.replace("\"Allowed\"", "\"Denied\"");
            tokio::fs::write(&path, content).await.unwrap();
        }

        // Re-open: chain should be broken, partial load.
        {
            let config = PersistentAuditConfig::new(path.clone());
            let log = PersistentAuditLog::open(config).await.unwrap();
            // It loads entries until the chain break or hash mismatch.
            assert!(log.len().await < 3 || !log.verify_chain().await);
        }
    }

    #[tokio::test]
    async fn test_persistent_audit_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        let config = PersistentAuditConfig::new(path.clone());
        let log = PersistentAuditLog::open(config).await.unwrap();
        assert_eq!(log.len().await, 0);
        assert!(log.verify_chain().await);
    }
}