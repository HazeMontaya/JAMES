//! JAMES Capability Broker — the enforcement spine of JAMES.
//!
//! Every action passes through the broker. The broker does NOT implement
//! capabilities; it decides whether a requested capability may execute, and
//! delegates the actual work to a [`CapabilityExecutor`] (a module or tool).
//!
//! Enforced chain (JAMES Master Concept §6/§7):
//! ```text
//! Capability-Request
//!   → Capability exists? (registry)
//!   → Input schema validation
//!   → Permission check (caller grants)
//!   → Policy decision      (ALLOW / DENY / ASK / CONDITIONAL)
//!   → Execution boundary   (executor)
//!   → Output verification  (output schema)
//!   → Usage metrics + Audit event
//! ```

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use james_capabilities::{CapabilityRegistry, CapabilityStatus, RiskLevel};
use james_events::{Event, EventBus};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

/// Outcome of the policy evaluation step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Capability may execute.
    Allow,
    /// Capability may not execute. The string is the human reason.
    Deny(String),
    /// Execution requires an interactive confirmation step.
    Ask,
    /// Execution allowed only under the described condition.
    Conditional(String),
}

/// A static policy rule bound to a capability id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub capability_id: String,
    pub decision: PolicyDecision,
}

impl PolicyRule {
    pub fn deny(capability_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            capability_id: capability_id.into(),
            decision: PolicyDecision::Deny(reason.into()),
        }
    }

    pub fn allow(capability_id: impl Into<String>) -> Self {
        Self {
            capability_id: capability_id.into(),
            decision: PolicyDecision::Allow,
        }
    }
}

/// A module or tool that actually performs a capability.
#[async_trait]
pub trait CapabilityExecutor: Send + Sync {
    /// Execute a capability. Must return a JSON value matching the capability's
    /// output schema (if one is registered).
    async fn execute(
        &self,
        capability_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value>;
}

/// Classification of the data carried by a capability request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataClassification { Public, Internal, Confidential, Restricted }

/// Requested side effect of an execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestedEffect { Read, Transform, Write, Execute, Communicate }

/// Structured confirmation binding to caller/capability/target/scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationContext {
    pub required: bool,
    pub confirmation_id: Option<String>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub caller_identity: Option<String>,
    pub capability_id: Option<String>,
    pub target: Option<String>,
    pub scope: Option<String>,
}

/// A pending human-approval request bound to the original capability request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationRequest {
    pub confirmation_id: String,
    pub request_id: String,
    pub caller_identity: String,
    pub capability_id: String,
    pub target: Option<String>,
    pub scope: Option<String>,
    pub reason: String,
    pub approved: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub expires_at: chrono::DateTime<Utc>,
}

/// Result returned when a pending confirmation is approved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationResult {
    pub confirmation_id: String,
    pub approved: bool,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyContext {
    pub values: serde_json::Map<String, serde_json::Value>,
}

/// Final structured capability request for the execution spine.
/// The legacy CapabilityRequest remains as a compatibility boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRequestV2 {
    pub request_id: String,
    pub caller_identity: String,
    pub capability_id: String,
    pub capability_version: Option<String>,
    pub target: Option<String>,
    pub scope: Option<String>,
    pub input: serde_json::Value,
    pub data_classification: DataClassification,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub requested_effect: RequestedEffect,
    pub risk_class: String,
    pub deadline_at: Option<chrono::DateTime<Utc>>,
    pub timeout_ms: Option<u64>,
    pub policy_context: PolicyContext,
    pub confirmation_context: ConfirmationContext,
}

impl CapabilityRequestV2 {
    pub fn new(caller_identity: impl Into<String>, capability_id: impl Into<String>, input: serde_json::Value) -> Self {
        let request_id = uuid::Uuid::new_v4().to_string();
        Self {
            request_id: request_id.clone(),
            caller_identity: caller_identity.into(),
            capability_id: capability_id.into(),
            capability_version: None,
            target: None,
            scope: None,
            input,
            data_classification: DataClassification::Internal,
            correlation_id: request_id,
            causation_id: None,
            requested_effect: RequestedEffect::Read,
            risk_class: "unspecified".to_string(),
            deadline_at: None,
            timeout_ms: None,
            policy_context: PolicyContext::default(),
            confirmation_context: ConfirmationContext {
                required: false,
                confirmation_id: None,
                expires_at: None,
                caller_identity: None,
                capability_id: None,
                target: None,
                scope: None,
            },
        }
    }

    fn legacy(&self) -> CapabilityRequest {
        CapabilityRequest { caller: self.caller_identity.clone(), capability_id: self.capability_id.clone(), input: self.input.clone() }
    }

    fn effective_timeout(&self) -> Option<std::time::Duration> {
        let by_timeout = self.timeout_ms.map(std::time::Duration::from_millis);
        let by_deadline = self.deadline_at.and_then(|deadline| {
            let remaining = deadline.signed_duration_since(Utc::now());
            if remaining.num_milliseconds() <= 0 { Some(std::time::Duration::ZERO) }
            else { Some(std::time::Duration::from_millis(remaining.num_milliseconds() as u64)) }
        });
        match (by_timeout, by_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityOutcomeV2 {
    pub request_id: String,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub caller_identity: String,
    pub capability_id: String,
    pub allowed: bool,
    pub decision: PolicyDecision,
    pub executed: bool,
    pub output: Option<serde_json::Value>,
    pub input_verified: bool,
    pub output_verified: bool,
    pub duration_ms: u64,
    pub reason: Option<String>,
    pub audited: bool,
}

/// A capability execution request.
#[derive(Debug, Clone)]
pub struct CapabilityRequest {
    /// The caller identity (module id, agent id, or "user").
    pub caller: String,
    /// The capability id, e.g. `memory.write` or `ai.inference`.
    pub capability_id: String,
    /// Input payload, validated against the capability input schema.
    pub input: serde_json::Value,
}

/// The structured outcome of a broker decision/execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityOutcome {
    pub caller: String,
    pub capability_id: String,
    pub allowed: bool,
    pub decision: PolicyDecision,
    pub executed: bool,
    pub output: Option<serde_json::Value>,
    pub input_verified: bool,
    pub output_verified: bool,
    pub duration_ms: u64,
    pub reason: Option<String>,
    pub audited: bool,
}

/// Errors raised by the broker. Deliberately structured so Callers can react
/// without string matching.
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("capability '{0}' is not registered or not available")]
    CapabilityUnavailable(String),
    #[error("input validation failed for '{capability}': {detail}")]
    InputValidationFailed { capability: String, detail: String },
    #[error("output validation failed for '{capability}': {detail}")]
    OutputValidationFailed { capability: String, detail: String },
    #[error("permission denied: caller '{caller}' lacks permission '{permission}' for capability '{capability}'")]
    PermissionDenied {
        caller: String,
        permission: String,
        capability: String,
    },
    #[error("policy denied execution of '{0}': {1}")]
    PolicyDenied(String, String),
    #[error("execution requires interactive confirmation ('{0}')")]
    ConfirmationRequired(String),
    #[error("executor error for '{capability}': {detail}")]
    ExecutionFailed { capability: String, detail: String },
    #[error("confirmation binding failed for '{capability}': {detail}")]
    ConfirmationBindingFailed { capability: String, detail: String },
    #[error("resource admission denied for '{capability}': {detail}")]
    ResourceAdmissionDenied { capability: String, detail: String },
}

#[async_trait]
pub trait ResourceAdmission: Send + Sync {
    async fn admit(&self, request: &CapabilityRequestV2) -> Result<()>;
}

/// Optional semantic verification performed after schema validation and before
/// a capability result is exposed to the rest of the runtime.
#[async_trait]
pub trait SemanticVerifier: Send + Sync {
    async fn verify(
        &self,
        request: &CapabilityRequestV2,
        output: &serde_json::Value,
    ) -> Result<()>;
}

/// The Capability Broker — the single gate every action must pass.
pub struct CapabilityBroker {
    registry: Arc<CapabilityRegistry>,
    event_bus: Option<Arc<EventBus>>,
    /// caller id -> set of granted permissions
    grants: DashMap<String, Vec<String>>,
    /// capability id -> policy rule
    policies: DashMap<String, PolicyRule>,
    /// audit toggle (always on in production; off-able for tests)
    audit_enabled: bool,
    /// outputs that passed verification but the caller may still read
    verified_cache: DashMap<String, serde_json::Value>,
    /// Pending interactive approvals, keyed by opaque confirmation id.
    confirmations: DashMap<String, ConfirmationRequest>,
    /// Serializes confirmation state transitions so approval/consumption is single-use under concurrency.
    confirmation_lock: Arc<Mutex<()>>,
    resource_admission: Option<Arc<dyn ResourceAdmission>>,
    semantic_verifier: Option<Arc<dyn SemanticVerifier>>,
}

impl CapabilityBroker {
    pub fn new(registry: Arc<CapabilityRegistry>) -> Self {
        Self {
            registry,
            event_bus: None,
            grants: DashMap::new(),
            policies: DashMap::new(),
            audit_enabled: true,
            verified_cache: DashMap::new(),
            confirmations: DashMap::new(),
            confirmation_lock: Arc::new(Mutex::new(())),
            resource_admission: None,
            semantic_verifier: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn without_audit(mut self) -> Self {
        self.audit_enabled = false;
        self
    }

    pub fn with_resource_admission(mut self, admission: Arc<dyn ResourceAdmission>) -> Self {
        self.resource_admission = Some(admission);
        self
    }

    pub fn with_semantic_verifier(mut self, verifier: Arc<dyn SemanticVerifier>) -> Self {
        self.semantic_verifier = Some(verifier);
        self
    }

    // ---- Permission administration -------------------------------------

    pub fn grant_permission(&self, caller: impl Into<String>, permission: impl Into<String>) {
        let caller = caller.into();
        let permission = permission.into();
        let mut entry = self.grants.entry(caller.clone()).or_default();
        if !entry.contains(&permission) {
            entry.push(permission);
        }
    }

    pub fn revoke_permission(&self, caller: impl Into<String>, permission: impl Into<String>) {
        let caller = caller.into();
        let permission = permission.into();
        if let Some(mut entry) = self.grants.get_mut(&caller) {
            entry.retain(|p| p != &permission);
        }
    }

    /// Grant every permission required by a capability to a caller.
    pub fn grant_capability_permissions(&self, caller: impl Into<String>, capability_id: &str) {
        let caller = caller.into();
        if let Some(def) = self.registry.get_definition(capability_id) {
            for perm in def.required_permissions {
                self.grant_permission(caller.clone(), perm);
            }
        }
    }

    pub fn caller_permissions(&self, caller: &str) -> Vec<String> {
        self.grants.get(caller).map(|e| e.clone()).unwrap_or_default()
    }

    // ---- Policy administration -------------------------------------------

    pub fn add_policy(&self, rule: PolicyRule) {
        self.policies.insert(rule.capability_id.clone(), rule);
    }

    pub fn remove_policy(&self, capability_id: &str) {
        self.policies.remove(capability_id);
    }

    fn evaluate_policy(&self, capability_id: &str) -> PolicyDecision {
        self.policies
            .get(capability_id)
            .map(|r| r.decision.clone())
            .unwrap_or(PolicyDecision::Allow)
    }

    // ---- Core flow ---------------------------------------------------------

    /// Execute a capability through the full enforced chain.
    ///
    /// On `Ask`/`Deny` the broker refuses to call the executor and returns the
    /// outcome with `executed == false`.
    pub async fn execute(
        &self,
        request: CapabilityRequest,
        executor: &dyn CapabilityExecutor,
    ) -> Result<CapabilityOutcome> {
        let start = Instant::now();

        // 1. Capability must exist and be available.
        let registered = self
            .registry
            .get(&request.capability_id)
            .ok_or_else(|| BrokerError::CapabilityUnavailable(request.capability_id.clone()))?;
        if registered.status != CapabilityStatus::Available {
            return Err(BrokerError::CapabilityUnavailable(request.capability_id.clone()).into());
        }
        let definition = registered.definition;

        // 2. Permission check — every required permission must be granted.
        for perm in &definition.required_permissions {
            let granted = self
                .grants
                .get(&request.caller)
                .map(|e| e.contains(perm))
                .unwrap_or(false);
            if !granted {
                self.audit("audit.capability.denied", &request, None).await;
                return Err(BrokerError::PermissionDenied {
                    caller: request.caller.clone(),
                    permission: perm.clone(),
                    capability: request.capability_id.clone(),
                }
                .into());
            }
        }

        // 3. Input schema validation.
        if let Err(e) = self
            .registry
            .validate_input(&request.capability_id, &request.input)
        {
            self.audit("audit.capability.invalid_input", &request, None).await;
            return Err(BrokerError::InputValidationFailed {
                capability: request.capability_id.clone(),
                detail: e.to_string(),
            }
            .into());
        }
        let input_verified = true;

        // 4. Policy decision.
        let decision = self.evaluate_policy(&request.capability_id);
        match &decision {
            PolicyDecision::Deny(reason) => {
                self.audit("audit.capability.policy_denied", &request, None).await;
                return Err(BrokerError::PolicyDenied(
                    request.capability_id.clone(),
                    reason.clone(),
                )
                .into());
            }
            PolicyDecision::Ask => {
                self.audit("audit.capability.ask", &request, None).await;
                return Err(
                    BrokerError::ConfirmationRequired(request.capability_id.clone()).into()
                );
            }
            PolicyDecision::Conditional(condition) => {
                self.audit("audit.capability.conditional", &request, None).await;
                return Err(BrokerError::PolicyDenied(
                    request.capability_id.clone(),
                    format!("condition not satisfied: {condition}"),
                )
                .into());
            }
            PolicyDecision::Allow => {}
        }

        // 5. Execution boundary — delegate to the executor.
        let output = match executor
            .execute(&request.capability_id, request.input.clone())
            .await
        {
            Ok(out) => out,
            Err(e) => {
                self.audit("audit.capability.failed", &request, None).await;
                return Err(BrokerError::ExecutionFailed {
                    capability: request.capability_id.clone(),
                    detail: e.to_string(),
                }
                .into());
            }
        };

        // 6. Output verification.
        if let Err(e) = self
            .registry
            .validate_output(&request.capability_id, &output)
        {
            self.audit("audit.capability.invalid_output", &request, Some(&output))
                .await;
            return Err(BrokerError::OutputValidationFailed {
                capability: request.capability_id.clone(),
                detail: e.to_string(),
            }
            .into());
        }
        let output_verified = true;

        // 7. Usage + audit.
        self.registry.record_usage(&request.capability_id).ok();
        self.verified_cache
            .insert(request.capability_id.clone(), output.clone());
        self.audit("audit.capability.executed", &request, Some(&output))
            .await;

        Ok(CapabilityOutcome {
            caller: request.caller,
            capability_id: request.capability_id,
            allowed: true,
            decision,
            executed: true,
            output: Some(output),
            input_verified,
            output_verified,
            duration_ms: start.elapsed().as_millis() as u64,
            reason: None,
            audited: self.audit_enabled,
        })
    }

    /// Preferred structured execution entry point for new runtime code.
    ///
    /// This is the canonical execution path. It deliberately does not call
    /// the legacy broker so every v2 request traverses one explicit sequence:
    /// availability → permission → input → policy → confirmation →
    /// resources → executor → output → semantic verification → usage/audit.
    pub async fn execute_v2(
        &self,
        request: CapabilityRequestV2,
        executor: &dyn CapabilityExecutor,
    ) -> Result<CapabilityOutcomeV2> {
        let started = Instant::now();

        if request.effective_timeout().is_some_and(|timeout| timeout.is_zero()) {
            self.audit_v2("audit.capability.deadline_exceeded", &request, None).await;
            return Err(BrokerError::ExecutionFailed {
                capability: request.capability_id.clone(),
                detail: "request deadline/timeout already expired".to_string(),
            }.into());
        }

        let registered = self.registry.get(&request.capability_id).ok_or_else(|| {
            BrokerError::CapabilityUnavailable(request.capability_id.clone())
        })?;
        if registered.status != CapabilityStatus::Available {
            return Err(BrokerError::CapabilityUnavailable(request.capability_id.clone()).into());
        }
        let definition = registered.definition;

        for perm in &definition.required_permissions {
            let granted = self.grants.get(&request.caller_identity)
                .map(|e| e.contains(perm))
                .unwrap_or(false);
            if !granted {
                self.audit_v2("audit.capability.permission_denied", &request, None).await;
                return Err(BrokerError::PermissionDenied {
                    caller: request.caller_identity.clone(),
                    permission: perm.clone(),
                    capability: request.capability_id.clone(),
                }.into());
            }
        }

        if let Err(e) = self.registry.validate_input(&request.capability_id, &request.input) {
            self.audit_v2("audit.capability.invalid_input", &request, None).await;
            return Err(BrokerError::InputValidationFailed {
                capability: request.capability_id.clone(),
                detail: e.to_string(),
            }.into());
        }

        // Only require/consume a confirmation when the request explicitly carries one.
        // This lets low-risk capabilities execute without manufacturing confirmation state.
        if request.confirmation_context.required {
            self.validate_confirmation(&request).await?;
        }

        let decision = self.decide_v2(&request).await?;
        match &decision {
            PolicyDecision::Allow => {}
            PolicyDecision::Deny(reason) => {
                self.audit_v2("audit.capability.policy_denied", &request, None).await;
                return Err(BrokerError::PolicyDenied(
                    request.capability_id.clone(), reason.clone()
                ).into());
            }
            PolicyDecision::Ask => {
                self.audit_v2("audit.capability.confirmation_required", &request, None).await;
                return Err(BrokerError::ConfirmationRequired(
                    request.capability_id.clone()
                ).into());
            }
            PolicyDecision::Conditional(condition) => {
                self.audit_v2("audit.capability.conditional", &request, None).await;
                return Err(BrokerError::PolicyDenied(
                    request.capability_id.clone(),
                    format!("condition not satisfied: {condition}"),
                ).into());
            }
        }

        if let Some(admission) = &self.resource_admission {
            if let Err(e) = admission.admit(&request).await {
                self.audit_v2("audit.capability.resource_denied", &request, None).await;
                return Err(BrokerError::ResourceAdmissionDenied {
                    capability: request.capability_id.clone(),
                    detail: e.to_string(),
                }.into());
            }
        }

        let execution = executor.execute(&request.capability_id, request.input.clone());
        let output = match request.effective_timeout() {
            Some(timeout) => match tokio::time::timeout(timeout, execution).await {
                Ok(Ok(output)) => output,
                Ok(Err(error)) => {
                    self.audit_v2(
                        "audit.capability.failed",
                        &request,
                        None,
                    ).await;
                    return Err(BrokerError::ExecutionFailed {
                        capability: request.capability_id.clone(),
                        detail: error.to_string(),
                    }.into());
                }
                Err(_) => {
                    self.audit_v2(
                        "audit.capability.timeout",
                        &request,
                        None,
                    ).await;
                    return Err(BrokerError::ExecutionFailed {
                        capability: request.capability_id.clone(),
                        detail: "request timeout exceeded".to_string(),
                    }.into());
                }
            },
            None => match execution.await {
                Ok(output) => output,
                Err(error) => {
                    self.audit_v2(
                        "audit.capability.failed",
                        &request,
                        None,
                    ).await;
                    return Err(BrokerError::ExecutionFailed {
                        capability: request.capability_id.clone(),
                        detail: error.to_string(),
                    }.into());
                }
            },
        };

        if let Err(e) = self.registry.validate_output(&request.capability_id, &output) {
            self.audit_v2("audit.capability.invalid_output", &request, Some(&output)).await;
            return Err(BrokerError::OutputValidationFailed {
                capability: request.capability_id.clone(),
                detail: e.to_string(),
            }.into());
        }

        if let Some(verifier) = &self.semantic_verifier {
            if let Err(e) = verifier.verify(&request, &output).await {
                self.audit_v2("audit.capability.semantic_verification_failed", &request, Some(&output)).await;
                return Err(BrokerError::OutputValidationFailed {
                    capability: request.capability_id.clone(),
                    detail: format!("semantic verification failed: {e}"),
                }.into());
            }
        }

        self.registry.record_usage(&request.capability_id).ok();
        self.verified_cache.insert(request.request_id.clone(), output.clone());
        self.audit_v2("audit.capability.executed.v2", &request, Some(&output)).await;

        Ok(CapabilityOutcomeV2 {
            request_id: request.request_id,
            correlation_id: request.correlation_id,
            causation_id: request.causation_id,
            caller_identity: request.caller_identity,
            capability_id: request.capability_id,
            allowed: true,
            decision,
            executed: true,
            output: Some(output),
            input_verified: true,
            output_verified: true,
            duration_ms: started.elapsed().as_millis() as u64,
            reason: None,
            audited: self.audit_enabled,
        })
    }

    /// Create a short-lived approval request for a capability requiring confirmation.
    pub async fn request_confirmation(
        &self,
        request: &CapabilityRequestV2,
    ) -> Result<ConfirmationRequest> {
        let registered = self.registry.get(&request.capability_id).ok_or_else(|| {
            BrokerError::CapabilityUnavailable(request.capability_id.clone())
        })?;
        if registered.status != CapabilityStatus::Available {
            return Err(BrokerError::CapabilityUnavailable(request.capability_id.clone()).into());
        }
        let now = Utc::now();
        let expires_at = now + chrono::Duration::minutes(5);
        let confirmation_id = Uuid::new_v4().to_string();
        let pending = ConfirmationRequest {
            confirmation_id: confirmation_id.clone(),
            request_id: request.request_id.clone(),
            caller_identity: request.caller_identity.clone(),
            capability_id: request.capability_id.clone(),
            target: request.target.clone(),
            scope: request.scope.clone(),
            reason: request.reason.clone().filter(|value| !value.trim().is_empty()).unwrap_or_else(|| format!("interactive approval required for {}", request.capability_id)),
            approved: false,
            created_at: now,
            expires_at,
        };
        { let _guard = self.confirmation_lock.lock().await;
            self.confirmations.insert(confirmation_id, pending.clone());
        }
        self.audit_v2("audit.capability.confirmation_requested", request, None).await;
        Ok(pending)
    }

    /// Approve a pending request. Approval is single-use and expires automatically.
    pub async fn approve_confirmation(
        &self,
        confirmation_id: &str,
        approver_identity: &str,
    ) -> Result<ConfirmationResult> {
        let pending = {
            let _guard = self.confirmation_lock.lock().await;
            let mut entry = self.confirmations.get_mut(confirmation_id).ok_or_else(|| {
                BrokerError::ConfirmationBindingFailed {
                    capability: "unknown".to_string(),
                    detail: "confirmation not found or already consumed".into(),
                }
            })?;
            if approver_identity.trim().is_empty() {
                return Err(BrokerError::ConfirmationBindingFailed {
                    capability: entry.capability_id.clone(),
                    detail: "approver identity missing".into(),
                }.into());
            }
            if entry.expires_at <= Utc::now() {
                let expired = entry.clone();
                drop(entry);
                self.confirmations.remove(confirmation_id);
                self.audit_confirmation("audit.capability.confirmation_expired", &expired, approver_identity).await;
                return Err(BrokerError::ConfirmationBindingFailed {
                    capability: "expired".to_string(),
                    detail: "confirmation expired".into(),
                }.into());
            }
            if entry.approved {
                return Err(BrokerError::ConfirmationBindingFailed {
                    capability: entry.capability_id.clone(),
                    detail: "confirmation already approved".into(),
                }.into());
            }
            entry.approved = true;
            entry.clone()
        };
        let expires_at = pending.expires_at;
        self.audit_confirmation("audit.capability.confirmation_approved", &pending, approver_identity).await;
        Ok(ConfirmationResult {
            confirmation_id: confirmation_id.to_string(),
            approved: true,
            expires_at,
        })
    }

    /// Reject and remove a pending confirmation.
    pub async fn reject_confirmation(
        &self,
        confirmation_id: &str,
        approver_identity: &str,
    ) -> Result<()> {
        let pending = {
            let _guard = self.confirmation_lock.lock().await;
            let pending = self.confirmations.get(confirmation_id).ok_or_else(|| {
                BrokerError::ConfirmationBindingFailed {
                    capability: "unknown".to_string(),
                    detail: "confirmation not found or already consumed".into(),
                }
            })?;
            if approver_identity.trim().is_empty() {
                return Err(BrokerError::ConfirmationBindingFailed {
                    capability: pending.capability_id.clone(),
                    detail: "approver identity missing".into(),
                }.into());
            }
            let pending = pending.clone();
            drop(pending);
            let removed = self.confirmations.remove(confirmation_id).map(|(_, value)| value);
            removed.ok_or_else(|| anyhow::anyhow!("confirmation was consumed concurrently"))?
        };
        self.audit_confirmation("audit.capability.confirmation_rejected", &pending, approver_identity).await;
        Ok(())
    }

    /// Remove expired confirmations and emit an explicit expiration event.
    pub async fn cleanup_expired_confirmations(&self) -> usize {
        let now = Utc::now();
        let _guard = self.confirmation_lock.lock().await;
        let expired: Vec<(String, ConfirmationRequest)> = self
            .confirmations
            .iter()
            .filter(|entry| entry.expires_at <= now)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        let mut removed = 0usize;
        for (id, pending) in expired {
            if self.confirmations.remove(&id).is_some() {
                self.audit_confirmation(
                    "audit.capability.confirmation_expired",
                    &pending,
                    "system:expiry",
                ).await;
                removed += 1;
            }
        }
        removed
    }

    /// Validate and consume an approved confirmation.
    async fn validate_confirmation(&self, request: &CapabilityRequestV2) -> Result<()> {
        let c = &request.confirmation_context;
        if !c.required {
            return Ok(());
        }
        let confirmation_id = c.confirmation_id.as_deref().filter(|v| !v.trim().is_empty()).ok_or_else(|| {
            BrokerError::ConfirmationBindingFailed {
                capability: request.capability_id.clone(),
                detail: "confirmation id missing".into(),
            }
        })?;

        if c.expires_at.map(|e| e <= Utc::now()).unwrap_or(true) {
            return Err(BrokerError::ConfirmationBindingFailed {
                capability: request.capability_id.clone(),
                detail: "confirmation expired or missing expiry".into(),
            }.into());
        }
        if c.caller_identity.as_ref().is_some_and(|v| v != &request.caller_identity)
            || c.capability_id.as_ref().is_some_and(|v| v != &request.capability_id)
            || (c.target.is_some() && c.target != request.target)
            || (c.scope.is_some() && c.scope != request.scope)
        {
            return Err(BrokerError::ConfirmationBindingFailed {
                capability: request.capability_id.clone(),
                detail: "confirmation context binding mismatch".into(),
            }.into());
        }

        let pending = {
            let _guard = self.confirmation_lock.lock().await;
            let pending = self.confirmations.get(confirmation_id).ok_or_else(|| {
                BrokerError::ConfirmationBindingFailed {
                    capability: request.capability_id.clone(),
                    detail: "confirmation not found, rejected, or already consumed".into(),
                }
            })?;

            if pending.request_id != request.request_id
                || pending.caller_identity != request.caller_identity
                || pending.capability_id != request.capability_id
                || pending.target != request.target
                || pending.scope != request.scope
                || !pending.approved
            {
                return Err(BrokerError::ConfirmationBindingFailed {
                    capability: request.capability_id.clone(),
                    detail: "confirmation binding mismatch or approval missing".into(),
                }.into());
            }
            if pending.expires_at <= Utc::now() {
                let expired = pending.clone();
                drop(pending);
                self.confirmations.remove(confirmation_id);
                self.audit_confirmation(
                    "audit.capability.confirmation_expired",
                    &expired,
                    &request.caller_identity,
                ).await;
                return Err(BrokerError::ConfirmationBindingFailed {
                    capability: request.capability_id.clone(),
                    detail: "confirmation expired".into(),
                }.into());
            }

            let consumed = pending.clone();
            drop(pending);
            self.confirmations.remove(confirmation_id);
            consumed
        };

        self.audit_v2("audit.capability.confirmation_consumed", request, None).await;
        self.audit_confirmation(
            "audit.capability.confirmation_consumed",
            &pending,
            &request.caller_identity,
        ).await;
        Ok(())
    }

    async fn audit_confirmation(
        &self,
        event_type: &str,
        confirmation: &ConfirmationRequest,
        actor: &str,
    ) {
        if !self.audit_enabled {
            return;
        }
        if let Some(bus) = &self.event_bus {
            let payload = serde_json::json!({
                "confirmation_id": confirmation.confirmation_id,
                "request_id": confirmation.request_id,
                "caller_identity": confirmation.caller_identity,
                "capability": confirmation.capability_id,
                "target": confirmation.target,
                "scope": confirmation.scope,
                "approved": confirmation.approved,
                "actor": actor,
                "created_at": confirmation.created_at.to_rfc3339(),
                "expires_at": confirmation.expires_at.to_rfc3339(),
                "at": Utc::now().to_rfc3339(),
            });
            if bus.publish(Event::new(event_type, "james-capability-broker").with_payload(payload)).await.is_err() {
                warn!("broker confirmation audit: failed to publish {}", event_type);
            }
        }
        info!(
            "broker {} confirmation={} actor={} capability={}",
            event_type, confirmation.confirmation_id, actor, confirmation.capability_id
        );
    }

    pub async fn decide_v2(&self, request: &CapabilityRequestV2) -> Result<PolicyDecision> {
        if request.deadline_at.map(|d| d <= Utc::now()).unwrap_or(false) {
            return Ok(PolicyDecision::Deny("request deadline expired".to_string()));
        }

        // Risk is an enforcement boundary, not descriptive metadata. Explicit
        // policies remain authoritative; otherwise dangerous capabilities
        // cannot become executable merely because an agent is broadly allowed.
        if let Some(rule) = self.policies.get(&request.capability_id) {
            return Ok(rule.decision.clone());
        }

        let registered = self.registry.get(&request.capability_id).ok_or_else(|| {
            BrokerError::CapabilityUnavailable(request.capability_id.clone())
        })?;
        match registered.definition.risk_level {
            RiskLevel::Critical => Ok(PolicyDecision::Deny(
                "critical-risk capability requires an explicit policy and cannot execute by default".to_string(),
            )),
            RiskLevel::High => {
                if request.confirmation_context.required {
                    Ok(PolicyDecision::Allow)
                } else {
                    Ok(PolicyDecision::Ask)
                }
            }
            RiskLevel::Medium | RiskLevel::Low => self.decide(&request.legacy()).await,
        }
    }

    /// Check-only decision (no execution) — used by UIs to preview permission.
    pub async fn decide(&self, request: &CapabilityRequest) -> Result<PolicyDecision> {
        let registered = self
            .registry
            .get(&request.capability_id)
            .ok_or_else(|| BrokerError::CapabilityUnavailable(request.capability_id.clone()))?;
        if registered.status != CapabilityStatus::Available {
            return Err(BrokerError::CapabilityUnavailable(request.capability_id.clone()).into());
        }
        let definition = registered.definition;
        for perm in &definition.required_permissions {
            let granted = self
                .grants
                .get(&request.caller)
                .map(|e| e.contains(perm))
                .unwrap_or(false);
            if !granted {
                return Ok(PolicyDecision::Deny(format!("missing permission '{perm}'")));
            }
        }
        Ok(self.evaluate_policy(&request.capability_id))
    }

    pub fn list_grants(&self) -> Vec<(String, Vec<String>)> {
        self.grants
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    async fn audit_v2(&self, event_type: &str, request: &CapabilityRequestV2, output: Option<&serde_json::Value>) {
        if !self.audit_enabled { return; }
        if let Some(bus) = &self.event_bus {
            let mut payload = serde_json::json!({
                "request_id": request.request_id,
                "caller_identity": request.caller_identity,
                "capability": request.capability_id,
                "capability_version": request.capability_version,
                "target": request.target,
                "scope": request.scope,
                "data_classification": request.data_classification,
                "correlation_id": request.correlation_id,
                "causation_id": request.causation_id,
                "requested_effect": request.requested_effect,
                "risk_class": request.risk_class,
                "deadline_at": request.deadline_at.map(|v| v.to_rfc3339()),
                "at": Utc::now().to_rfc3339()
            });
            if let Some(out) = output { payload["output"] = out.clone(); }
            if bus.publish(Event::new(event_type, "james-capability-broker").with_payload(payload)).await.is_err() {
                warn!("broker audit: failed to publish {}", event_type);
            }
        }
        info!("broker {} request={} correlation={} caller={} capability={}", event_type, request.request_id, request.correlation_id, request.caller_identity, request.capability_id);
    }

    async fn audit(
        &self,
        event_type: &str,
        request: &CapabilityRequest,
        output: Option<&serde_json::Value>,
    ) {
        if !self.audit_enabled {
            return;
        }
        if let Some(bus) = &self.event_bus {
            let mut payload = serde_json::json!({
                "caller": request.caller,
                "capability": request.capability_id,
                "input": request.input,
                "at": Utc::now().to_rfc3339(),
            });
            if let Some(out) = output {
                payload["output"] = out.clone();
            }
            if bus
                .publish(Event::new(event_type, "james-capability-broker").with_payload(payload))
                .await
                .is_err()
            {
                warn!("broker audit: failed to publish {}", event_type);
            }
        }
        info!(
            "broker {} caller={} capability={}",
            event_type, request.caller, request.capability_id
        );
    }
}

/// A registry-based executor for tests — executes trivial capabilities.
#[derive(Default)]
pub struct NoopExecutor;

#[async_trait]
impl CapabilityExecutor for NoopExecutor {
    async fn execute(
        &self,
        capability_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "echo": capability_id, "data": input }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_capabilities::{CapabilityDefinition, CapabilityCategory, ExecutionTarget, RiskLevel};

    fn test_definition(id: &str, perms: Vec<&str>) -> CapabilityDefinition {
        CapabilityDefinition {
            id: id.to_string(),
            name: id.to_string(),
            category: CapabilityCategory::Custom("test".to_string()),
            version: "1.0.0".to_string(),
            provider: "test".to_string(),
            description: "test".to_string(),
            risk_level: RiskLevel::Low,
            required_permissions: perms.into_iter().map(|s| s.to_string()).collect(),
            dependencies: vec![],
            input_schema: None,
            output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec![],
            deprecated: false,
            experimental: false,
        }
    }

    async fn registry_with(ids: &[(&str, Vec<&str>)]) -> Arc<CapabilityRegistry> {
        let reg = Arc::new(CapabilityRegistry::new());
        for (id, perms) in ids {
            let def = test_definition(id, perms.clone());
            reg.register(def, "test").await.unwrap();
        }
        reg
    }

    #[tokio::test]
    async fn test_missing_permission_denied() {
        let reg = registry_with(&[("memory.read", vec!["memory.read"])]).await;
        let broker = CapabilityBroker::new(reg).without_audit();
        let err = broker
            .execute(
                CapabilityRequest {
                    caller: "void".to_string(),
                    capability_id: "memory.read".to_string(),
                    input: serde_json::json!({}),
                },
                &NoopExecutor,
            )
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("permission denied"), "{msg}");
    }

    #[tokio::test]
    async fn test_grant_then_execute_ok() {
        let reg = registry_with(&[("memory.read", vec!["memory.read"])]).await;
        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("void", "memory.read");
        let outcome = broker
            .execute(
                CapabilityRequest {
                    caller: "void".to_string(),
                    capability_id: "memory.read".to_string(),
                    input: serde_json::json!({"limit": 10}),
                },
                &NoopExecutor,
            )
            .await
            .unwrap();
        assert!(outcome.executed);
        assert!(outcome.allowed);
        assert!(outcome.output_verified);
    }

    #[tokio::test]
    async fn test_policy_deny_blocks_execution() {
        let reg = registry_with(&[("ai.inference", vec!["ai.inference"])]).await;
        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("ai", "ai.inference");
        broker.add_policy(PolicyRule::deny("ai.inference", "policy test"));
        let err = broker
            .execute(
                CapabilityRequest {
                    caller: "ai".to_string(),
                    capability_id: "ai.inference".to_string(),
                    input: serde_json::json!({}),
                },
                &NoopExecutor,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("policy denied"), "{err:?}");
    }

    #[tokio::test]
    async fn test_unknown_capability_unavailable() {
        let reg = registry_with(&[]).await;
        let broker = CapabilityBroker::new(reg).without_audit();
        let err = broker
            .execute(
                CapabilityRequest {
                    caller: "x".to_string(),
                    capability_id: "nope".to_string(),
                    input: serde_json::json!({}),
                },
                &NoopExecutor,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not registered"), "{err:?}");
    }

    #[tokio::test]
    async fn test_decide_preview() {
        let reg = registry_with(&[("voice.output", vec!["voice.output"])]).await;
        let broker = CapabilityBroker::new(reg).without_audit();
        // No grant yet -> Deny
        let d = broker
            .decide(&CapabilityRequest {
                caller: "tts".to_string(),
                capability_id: "voice.output".to_string(),
                input: serde_json::json!({}),
            })
            .await
            .unwrap();
        assert!(matches!(d, PolicyDecision::Deny(_)));
        broker.grant_capability_permissions("tts", "voice.output");
        let d = broker
            .decide(&CapabilityRequest {
                caller: "tts".to_string(),
                capability_id: "voice.output".to_string(),
                input: serde_json::json!({}),
            })
            .await
            .unwrap();
        assert_eq!(d, PolicyDecision::Allow);
    }

    #[tokio::test]
    async fn test_v2_critical_risk_denied_by_default() {
        let mut def = test_definition("python_exec", vec!["process.python"]);
        def.risk_level = RiskLevel::Critical;
        let reg = registry_with(&[]).await;
        reg.register(def, "test").await.unwrap();
        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("agent:test", "python_exec");

        let decision = broker.decide_v2(&CapabilityRequestV2::new(
            "agent:test",
            "python_exec",
            serde_json::json!({}),
        )).await.unwrap();

        assert!(matches!(decision, PolicyDecision::Deny(reason) if reason.contains("critical-risk")));
    }

    #[tokio::test]
    async fn test_v2_high_risk_requires_confirmation_by_default() {
        let mut def = test_definition("file.write", vec!["file.write"]);
        def.risk_level = RiskLevel::High;
        let reg = registry_with(&[]).await;
        reg.register(def, "test").await.unwrap();
        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("agent:test", "file.write");

        let decision = broker.decide_v2(&CapabilityRequestV2::new(
            "agent:test",
            "file.write",
            serde_json::json!({}),
        )).await.unwrap();

        assert_eq!(decision, PolicyDecision::Ask);
    }

    #[tokio::test]
    async fn test_ask_requires_confirmation() {
        let reg = registry_with(&[("process.run", vec!["process.execute"])]).await;
        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("x", "process.run");
        broker.add_policy(PolicyRule {
            capability_id: "process.run".to_string(),
            decision: PolicyDecision::Ask,
        });
        let err = broker
            .execute(
                CapabilityRequest {
                    caller: "x".to_string(),
                    capability_id: "process.run".to_string(),
                    input: serde_json::json!({}),
                },
                &NoopExecutor,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("confirmation"), "{err:?}");
    }

    #[tokio::test]
    async fn test_disabled_capability_cannot_execute_or_decide() {
        let reg = registry_with(&[("memory.read", vec!["memory.read"])]).await;
        reg.update_status("memory.read", CapabilityStatus::Disabled).unwrap();
        let broker = CapabilityBroker::new(reg.clone()).without_audit();
        broker.grant_capability_permissions("x", "memory.read");

        let err = broker
            .execute(
                CapabilityRequest {
                    caller: "x".to_string(),
                    capability_id: "memory.read".to_string(),
                    input: serde_json::json!({}),
                },
                &NoopExecutor,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not registered or not available"), "{err:?}");

        let err = broker
            .decide(&CapabilityRequest {
                caller: "x".to_string(),
                capability_id: "memory.read".to_string(),
                input: serde_json::json!({}),
            })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not registered or not available"), "{err:?}");
    }

    #[tokio::test]
    async fn test_conditional_policy_does_not_execute_without_evaluation() {
        let reg = registry_with(&[("process.run", vec!["process.execute"])]).await;
        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("x", "process.run");
        broker.add_policy(PolicyRule {
            capability_id: "process.run".to_string(),
            decision: PolicyDecision::Conditional("only during maintenance window".to_string()),
        });

        let err = broker
            .execute(
                CapabilityRequest {
                    caller: "x".to_string(),
                    capability_id: "process.run".to_string(),
                    input: serde_json::json!({}),
                },
                &NoopExecutor,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("condition not satisfied"), "{err:?}");
    }
    #[tokio::test]
    async fn test_high_risk_confirmation_round_trip_is_bound_and_single_use() {
        let mut def = test_definition("file.write", vec!["file.write"]);
        def.risk_level = RiskLevel::High;
        let reg = registry_with(&[]).await;
        reg.register(def, "test").await.unwrap();

        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("agent:test", "file.write");

        let mut request = CapabilityRequestV2::new(
            "agent:test",
            "file.write",
            serde_json::json!({"path": "workspace/a.txt", "content": "hello"}),
        );
        request.target = Some("workspace/a.txt".to_string());
        request.scope = Some("workspace".to_string());

        let confirmation = broker.request_confirmation(&request).await.unwrap();
        assert_eq!(confirmation.request_id, request.request_id);
        assert!(!confirmation.approved);

        let before = broker.execute_v2(request.clone(), &NoopExecutor).await.unwrap_err();
        assert!(before.to_string().contains("confirmation"));

        broker.approve_confirmation(&confirmation.confirmation_id, "agent:test").await.unwrap();
        request.confirmation_context = ConfirmationContext {
            required: true,
            confirmation_id: Some(confirmation.confirmation_id.clone()),
            expires_at: Some(confirmation.expires_at),
            caller_identity: Some("agent:test".to_string()),
            capability_id: Some("file.write".to_string()),
            target: Some("workspace/a.txt".to_string()),
            scope: Some("workspace".to_string()),
        };

        let outcome = broker.execute_v2(request.clone(), &NoopExecutor).await.unwrap();
        assert!(outcome.executed);

        let replay = broker.execute_v2(request, &NoopExecutor).await.unwrap_err();
        assert!(replay.to_string().contains("confirmation"), "{replay:?}");
    }

    #[tokio::test]
    async fn test_confirmation_cannot_be_approved_twice() {
        let reg = Arc::new(CapabilityRegistry::new());
        let mut def = test_definition("file.write", vec!["file.write"]);
        def.risk_level = RiskLevel::High;
        reg.register(def, "test").await.unwrap();
        let broker = Arc::new(CapabilityBroker::new(reg).without_audit());
        broker.grant_capability_permissions("agent:test", "file.write");

        let request = CapabilityRequestV2::new(
            "agent:test",
            "file.write",
            serde_json::json!({"path":"workspace/a.txt","content":"x"}),
        )
        .with_target("workspace/a.txt")
        .with_scope("workspace")
        .with_confirmation_context(ConfirmationContext {
            required: false,
            confirmation_id: None,
            expires_at: None,
            caller_identity: None,
            capability_id: None,
            target: None,
            scope: None,
        });

        let confirmation = broker.request_confirmation(&request).await.unwrap();
        broker.approve_confirmation(&confirmation.confirmation_id, "human:1").await.unwrap();
        let second = broker.approve_confirmation(&confirmation.confirmation_id, "human:2").await;
        assert!(second.is_err());
    }

    #[tokio::test]
    async fn test_confirmation_cannot_be_reused_for_another_target() {
        let mut def = test_definition("file.write", vec!["file.write"]);
        def.risk_level = RiskLevel::High;
        let reg = registry_with(&[]).await;
        reg.register(def, "test").await.unwrap();

        let broker = CapabilityBroker::new(reg).without_audit();
        broker.grant_capability_permissions("agent:test", "file.write");

        let request = CapabilityRequestV2::new(
            "agent:test",
            "file.write",
            serde_json::json!({"path": "workspace/a.txt", "content": "hello"}),
        );
        let confirmation = broker.request_confirmation(&request).await.unwrap();
        broker.approve_confirmation(&confirmation.confirmation_id, "agent:test").await.unwrap();

        let mut altered = request.clone();
        altered.target = Some("workspace/other.txt".to_string());
        altered.confirmation_context = ConfirmationContext {
            required: true,
            confirmation_id: Some(confirmation.confirmation_id),
            expires_at: Some(confirmation.expires_at),
            caller_identity: Some("agent:test".to_string()),
            capability_id: Some("file.write".to_string()),
            target: Some("workspace/other.txt".to_string()),
            scope: Some("workspace".to_string()),
        };

        let err = broker.execute_v2(altered, &NoopExecutor).await.unwrap_err();
        assert!(err.to_string().contains("binding"), "{err:?}");
    }
    }

}