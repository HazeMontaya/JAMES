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

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use james_capabilities::CapabilityRegistry;
use james_events::{Event, EventBus};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

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

/// The executor trait — a module or tool that actually performs a capability.
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
        let definition = self
            .registry
            .get_definition(&request.capability_id)
            .ok_or_else(|| BrokerError::CapabilityUnavailable(request.capability_id.clone()))?;

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
            PolicyDecision::Conditional(_) | PolicyDecision::Allow => {}
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

    /// Check-only decision (no execution) — used by UIs to preview permission.
    pub async fn decide(&self, request: &CapabilityRequest) -> Result<PolicyDecision> {
        let definition = self
            .registry
            .get_definition(&request.capability_id)
            .ok_or_else(|| BrokerError::CapabilityUnavailable(request.capability_id.clone()))?;
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
}