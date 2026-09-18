//! JAMES Identity & Integrity — the identity registry of JAMES.
//!
//! The Core keeps the identity registry and the JAR (James Auth Record)
//! repository. Every caller (module, agent, user, device) is represented by a
//! registered [`Identity`]. Claims are forwarded to the audit chain
//! ([`crate::forward_claim`]). Cryptographic verification of signed claims is
//! intentionally asymmetric: identity integrity (fingerprints, status) lives
//! here; the actual signature check happens against the registered public key
//! material and is run by the caller-supplied verifier.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use james_events::{Event, EventBus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::info;
use uuid::Uuid;

/// Errors raised by the identity registry.
#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("identity '{0}' is already registered")]
    AlreadyExists(String),
    #[error("identity '{0}' not found")]
    NotFound(String),
}

/// The kind of an identity (what the caller is).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum IdentityKind {
    Module,
    Agent,
    User,
    Device,
}

/// Lifecycle status of an identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum IdentityStatus {
    /// Registered and allowed to make claims.
    Active,
    /// Explicitly revoked; claims are rejected.
    Revoked,
    /// Blocked by a policy incident; claims are rejected until unblocked.
    Blocked,
}

/// A registered caller identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Identity {
    pub id: String,
    pub kind: IdentityKind,
    pub name: String,
    /// SHA-256 of the identity material (public key / signing key id).
    pub fingerprint: String,
    pub status: IdentityStatus,
    pub registered_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    /// Free-form attributes (e.g. capability allow-list hints, metadata).
    pub attributes: serde_json::Value,
}

impl Identity {
    pub fn new(
        id: impl Into<String>,
        kind: IdentityKind,
        name: impl Into<String>,
        public_key_material: &[u8],
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(public_key_material);
        let fingerprint = format!("{:x}", hasher.finalize());
        Self {
            id: id.into(),
            kind,
            name: name.into(),
            fingerprint,
            status: IdentityStatus::Active,
            registered_at: Utc::now(),
            expires_at: None,
            attributes: serde_json::json!({}),
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == IdentityStatus::Active
            && self.expires_at.map(|e| e > Utc::now()).unwrap_or(true)
    }
}

/// A claim a caller makes about a capability it wants to exercise.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Claim {
    pub subject: String,
    pub capability: String,
    pub action: String,
    pub resource: Option<String>,
    /// Placeholder: signature over the claim (external verifiers fill this).
    pub signature: Option<String>,
    pub at: DateTime<Utc>,
}

impl Claim {
    pub fn new(subject: impl Into<String>, capability: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            capability: capability.into(),
            action: action.into(),
            resource: None,
            signature: None,
            at: Utc::now(),
        }
    }

    /// Stable digest of the unsigned claim fields, to anchor a signature onto.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.subject.as_bytes());
        hasher.update(self.capability.as_bytes());
        hasher.update(self.action.as_bytes());
        if let Some(r) = &self.resource {
            hasher.update(r.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Result of verifying a claim against a registered identity.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClaimResult {
    pub verified: bool,
    pub reason: Option<String>,
    pub identity: Option<Identity>,
}

/// The identity registry (Core-owned). Thread-safe, in-memory.
pub struct IdentityRegistry {
    identities: DashMap<String, Identity>,
    event_bus: Option<Arc<EventBus>>,
}

impl Default for IdentityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityRegistry {
    pub fn new() -> Self {
        Self {
            identities: DashMap::new(),
            event_bus: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Register an identity. Fails if the id is already registered.
    pub fn register(&self, identity: Identity) -> std::result::Result<(), IdentityError> {
        if self.identities.contains_key(&identity.id) {
            return Err(IdentityError::AlreadyExists(identity.id));
        }
        info!("identity registered: {} ({})", identity.id, identity.kind_name());
        self.identities.insert(identity.id.clone(), identity);
        Ok(())
    }

    /// Look up an identity by id.
    pub fn get(&self, id: &str) -> Option<Identity> {
        self.identities.get(id).map(|e| e.clone())
    }

    /// Revoke (soft-delete) an identity. Claims are then rejected.
    pub fn revoke(&self, id: &str) -> bool {
        if let Some(mut entry) = self.identities.get_mut(id) {
            entry.status = IdentityStatus::Revoked;
            return true;
        }
        false
    }

    /// Block (policy incident) an identity.
    pub fn block(&self, id: &str) -> bool {
        if let Some(mut entry) = self.identities.get_mut(id) {
            entry.status = IdentityStatus::Blocked;
            return true;
        }
        false
    }

    /// Unblock an identity.
    pub fn unblock(&self, id: &str) -> bool {
        if let Some(mut entry) = self.identities.get_mut(id) {
            entry.status = IdentityStatus::Active;
            return true;
        }
        false
    }

    /// All registered identities.
    pub fn all(&self) -> Vec<Identity> {
        self.identities.iter().map(|e| e.clone()).collect()
    }

    // ---- Persistence ------------------------------------------------------

    /// Save all identities to a JSON file (atomic via temp file + rename).
    /// A successful save makes the registry restart-safe.
    pub async fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let identities = self.all();
        let json = serde_json::to_string_pretty(&identities)
            .context("failed to serialize identities")?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("failed to create identity directory")?;
        }

        let tmp = path.with_extension("json.tmp");
        tokio::fs::write(&tmp, json.as_bytes())
            .await
            .context("failed to write identity tmp file")?;
        tokio::fs::rename(&tmp, path)
            .await
            .context("failed to move identity file into place")?;

        info!("identity registry saved: {} identities to {}", identities.len(), path.display());
        Ok(())
    }

    /// Load identities from a JSON file, merging into the existing registry.
    /// Identities that already exist are skipped (first wins).
    pub async fn load(&self, path: impl AsRef<Path>) -> Result<usize> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(0);
        }
        let raw = tokio::fs::read_to_string(path)
            .await
            .context("failed to read identity file")?;
        let identities: Vec<Identity> = serde_json::from_str(&raw)
            .context("failed to parse identity file")?;

        let mut loaded = 0;
        for identity in identities {
            if self.identities.contains_key(&identity.id) {
                continue;
            }
            self.identities.insert(identity.id.clone(), identity);
            loaded += 1;
        }
        info!("identity registry loaded: {loaded} identities from {}", path.display());
        Ok(loaded)
    }

    /// Reset the in-memory registry (used before loading a snapshot).
    pub fn clear(&self) {
        self.identities.clear();
    }

    /// Number of registered identities.
    pub fn len(&self) -> usize {
        self.identities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }

    /// Verify a claim against the registry without a signature.
    ///
    /// Identity-level checks only: existence, active status, non-expiry and a
    /// claim/identity subject match. When a signature is present, the caller /
    /// broker is expected to have verified it and set the claim accordingly.
    pub fn verify_claim(&self, claim: &Claim) -> ClaimResult {
        let identity = match self.get(&claim.subject) {
            Some(id) => id,
            None => {
                return ClaimResult {
                    verified: false,
                    reason: Some(format!("unknown identity '{}'", claim.subject)),
                    identity: None,
                }
            }
        };

        if !identity.is_active() {
            return ClaimResult {
                verified: false,
                reason: Some(format!("identity '{}' is {:?}", identity.id, identity.status)),
                identity: Some(identity),
            };
        }

        ClaimResult {
            verified: true,
            reason: None,
            identity: Some(identity),
        }
    }
}

impl IdentityKind {
    fn name(self) -> &'static str {
        match self {
            IdentityKind::Module => "module",
            IdentityKind::Agent => "agent",
            IdentityKind::User => "user",
            IdentityKind::Device => "device",
        }
    }
}

impl Identity {
    pub(crate) fn kind_name(&self) -> &'static str {
        self.kind.name()
    }
}

/// Forward a claim decision into the audit chain (B3: claims FORWARD).
///
/// Publishes an `audit.claim.<outcome>` event; if an event bus is attached to
/// the [`IdentityRegistry`], the record is forwarded there. Returns the event
/// id so callers can correlate.
pub async fn forward_claim(
    bus: Option<&Arc<EventBus>>,
    registry: Option<&IdentityRegistry>,
    claim: &Claim,
    verified: bool,
    reason: Option<&str>,
) -> Uuid {
    let event_type = if verified {
        "audit.claim.verified"
    } else {
        "audit.claim.rejected"
    };
    let event = Event::new(event_type, "james-identity").with_payload(serde_json::json!({
        "subject": claim.subject,
        "capability": claim.capability,
        "action": claim.action,
        "resource": claim.resource,
        "verified": verified,
        "reason": reason,
        "claim_digest": claim.digest(),
        "at": claim.at.to_rfc3339(),
    }));
    let event_id = event.event_id;

    if let Some(bus) = bus {
        let _ = bus
            .publish_envelope(james_events::EventEnvelope::new(event.clone()))
            .await;
    }
    // Ensure registry usage is observable when provided.
    if let Some(reg) = registry {
        if let Some(identity) = reg.get(&claim.subject) {
            info!(
                "claim {}/{} for {} -> verified={} reason={:?} (fp={})",
                claim.subject,
                claim.capability,
                identity.kind.name(),
                verified,
                reason,
                identity.fingerprint
            );
        }
    }
    event_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_verify_claim() {
        let reg = IdentityRegistry::new();
        let id = Identity::new("com.example.browser", IdentityKind::Module, "james-browser", b"pubkey-1");
        reg.register(id).unwrap();

        let claim = Claim::new("com.example.browser", "browser.navigate", "execute");
        let result = reg.verify_claim(&claim);
        assert!(result.verified);
        assert_eq!(result.identity.unwrap().name, "james-browser");
    }

    #[test]
    fn test_unknown_identity_rejected() {
        let reg = IdentityRegistry::new();
        let claim = Claim::new("unknown", "browser.navigate", "execute");
        let result = reg.verify_claim(&claim);
        assert!(!result.verified);
        assert_eq!(result.identity, None);
    }

    #[test]
    fn test_revoked_identity_rejected() {
        let reg = IdentityRegistry::new();
        reg.register(Identity::new("mod1", IdentityKind::Module, "m1", b"k")).unwrap();
        assert!(reg.revoke("mod1"));
        let claim = Claim::new("mod1", "memory.read", "execute");
        let result = reg.verify_claim(&claim);
        assert!(!result.verified);
    }

    #[test]
    fn test_fingerprint_stable() {
        let a = Identity::new("a", IdentityKind::Device, "dev-a", b"material");
        let b = Identity::new("b", IdentityKind::Device, "dev-b", b"material");
        let c = Identity::new("c", IdentityKind::Device, "dev-c", b"other");
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_ne!(a.fingerprint, c.fingerprint);
    }

    #[test]
    fn test_duplicate_rejected() {
        let reg = IdentityRegistry::new();
        reg.register(Identity::new("dup", IdentityKind::User, "u", b"k")).unwrap();
        let err = reg.register(Identity::new("dup", IdentityKind::User, "u2", b"k2"));
        assert!(err.is_err());
        assert!(matches!(err, Err(IdentityError::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_forward_claim_event() {
        let bus = Arc::new(EventBus::new(16));
        bus.start().await.unwrap();
        let mut rx = bus.subscribe("audit.claim.verified");
        let claim = Claim::new("mod1", "system.files.read", "execute");
        let _id = forward_claim(Some(&bus), None, &claim, true, None).await;
        let envelope = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("audit.claim.verified event was forwarded")
            .unwrap();
        assert_eq!(envelope.event.event_type, "audit.claim.verified");
        assert_eq!(envelope.event.payload["subject"], "mod1");
    }

    #[tokio::test]
    async fn test_save_and_load_roundtrip() {
        let reg = IdentityRegistry::new();
        reg.register(Identity::new("mod1", IdentityKind::Module, "m1", b"key1")).unwrap();
        reg.register(Identity::new("user1", IdentityKind::User, "u1", b"key2")).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identities.json");

        reg.save(&path).await.unwrap();

        // Fresh registry loads the two identities back.
        let reg2 = IdentityRegistry::new();
        let loaded = reg2.load(&path).await.unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(reg2.len(), 2);
        let id = reg2.get("mod1").unwrap();
        assert_eq!(id.kind, IdentityKind::Module);
        assert_eq!(id.name, "m1");
        assert!(id.is_active());
    }

    #[tokio::test]
    async fn test_load_skips_duplicates() {
        let reg = IdentityRegistry::new();
        reg.register(Identity::new("mod1", IdentityKind::Module, "m1", b"key1")).unwrap();
        reg.register(Identity::new("mod2", IdentityKind::Module, "m2", b"key3")).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identities.json");
        reg.save(&path).await.unwrap();

        // Loading into the same registry skips the existing 'mod1'.
        let loaded = reg.load(&path).await.unwrap();
        assert_eq!(loaded, 0); // both already present
        assert_eq!(reg.len(), 2);

        // Loading into a registry with one existing identity adds the other.
        let reg3 = IdentityRegistry::new();
        reg3.register(Identity::new("mod1", IdentityKind::Module, "m1", b"key1")).unwrap();
        let loaded2 = reg3.load(&path).await.unwrap();
        assert_eq!(loaded2, 1);
        assert_eq!(reg3.len(), 2);
    }

    #[tokio::test]
    async fn test_load_missing_file_returns_zero() {
        let reg = IdentityRegistry::new();
        let dir = tempfile::tempdir().unwrap();
        let n = reg.load(dir.path().join("nope.json")).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_load_after_revocation_restores_status() {
        let reg = IdentityRegistry::new();
        reg.register(Identity::new("mod1", IdentityKind::Module, "m1", b"key1")).unwrap();
        assert!(reg.revoke("mod1"));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identities.json");
        reg.save(&path).await.unwrap();

        // Re-load: the revoked status round-trips.
        let reg2 = IdentityRegistry::new();
        reg2.load(&path).await.unwrap();
        let id = reg2.get("mod1").unwrap();
        assert_eq!(id.status, IdentityStatus::Revoked);
        assert!(!id.is_active());
    }
}