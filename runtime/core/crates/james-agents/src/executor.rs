//! Plan executor — executes a [`Plan`] step-by-step through the
//! capability broker, honoring retry policies, dependencies and timeouts.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dashmap::DashMap;
use tracing::warn;

use crate::model::*;
use james_capability_broker::{CapabilityBroker, CapabilityExecutor, CapabilityRequestV2, RequestedEffect};
use crate::{AgentError, AgentState, CapabilityResolver, ExecutorCandidate, ProviderHealth, ResolutionContext, PlanExecutionResult, StepResult};

/// Simple exponential backoff delay for a retry attempt.
fn backoff_delay(policy: &RetryPolicy, attempt: u32) -> Duration {
    let exp = (policy.base_delay_ms as u64) * (policy.exponential_base.powf(attempt as f64) as u64);
    Duration::from_millis(exp.min(policy.max_delay_ms))
}

/// A standalone plan executor. Works with any set of registered executors.
pub struct PlanExecutor {
    broker: Arc<CapabilityBroker>,
    executors: Arc<DashMap<String, Arc<dyn CapabilityExecutor>>>,
    resolver: Arc<CapabilityResolver>,
}

impl PlanExecutor {
    pub fn new(broker: Arc<CapabilityBroker>) -> Self {
        Self {
            broker,
            executors: Arc::new(DashMap::new()),
            resolver: Arc::new(CapabilityResolver::new()),
        }
    }

    /// Register an executor for a capability id.
    pub fn register_executor(&self, capability_id: impl Into<String>, executor: Arc<dyn CapabilityExecutor>) {
        self.register_executor_with_provider(capability_id, "direct", executor);
    }

    /// Register an executor while preserving its real provider identity.
    /// Provider identity is used by health-aware resolution and failover.
    pub fn register_executor_with_provider(
        &self,
        capability_id: impl Into<String>,
        provider: impl Into<String>,
        executor: Arc<dyn CapabilityExecutor>,
    ) {
        let id = capability_id.into();
        let provider = provider.into();
        self.executors.insert(id.clone(), executor.clone());
        self.resolver.register_or_replace(ExecutorCandidate::new(id, provider, executor));
    }

    /// Merge a shared capability resolver's candidates into this executor.
    /// Local registrations are layered underneath (same provider tag wins by
    /// re-registration order on conflict).
    pub fn attach_resolver(&self, resolver: Arc<CapabilityResolver>) {
        for id in resolver.ids() {
            for candidate in resolver.candidates(&id) {
                self.resolver.register_or_replace(candidate);
            }
        }
    }

    /// The resolver used by this executor.
    pub fn resolver(&self) -> Arc<CapabilityResolver> {
        self.resolver.clone()
    }

    pub fn unregister_executor(&self, capability_id: &str) -> Option<Arc<dyn CapabilityExecutor>> {
        let removed = self.executors.remove(capability_id).map(|(_, e)| e);
        self.resolver.unregister(capability_id, "direct");
        self.resolver.unregister(capability_id, "python");
        removed
    }

    /// Remove every executor candidate owned by one provider.
    pub fn unregister_provider(&self, provider: &str) -> usize {
        let ids = self.resolver.ids();
        let mut removed = 0;
        for id in ids {
            if self.resolver.unregister(&id, provider) {
                self.executors.remove(&id);
                removed += 1;
            }
        }
        removed
    }

    pub fn provider_executor_ids(&self, provider: &str) -> Vec<String> {
        let mut ids = self.resolver.ids().into_iter()
            .filter(|id| self.resolver.candidates(id).iter().any(|c| c.provider == provider))
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn has_executor(&self, capability_id: &str) -> bool {
        self.executors.contains_key(capability_id)
            || self
                .resolver
                .resolve(capability_id, &ResolutionContext::default())
                .selected
                .is_some()
    }

    pub fn executor_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.executors.iter().map(|e| e.key().clone()).collect();
        ids.extend(self.resolver.ids());
        ids.sort();
        ids.dedup();
        ids
    }

    /// Execute a single step (one attempt), applying the step timeout.
    pub async fn execute_step(
        &self,
        plan: &Plan,
        step: &PlanStep,
        caller: &str,
    ) -> Result<CapabilityExecutionResult, AgentError> {
        tracing::info!("PlanExecutor: execute_step for '{}', capability_id={}, caller={}", step.name, step.capability_id, caller);
        
        // Resolve via the shared resolver first, then the local map
        // (Block C: Requirement → Candidates → Selection).
        let resolution = self
            .resolver
            .resolve(&step.capability_id, &ResolutionContext::default());

        let (executor, provider) = match resolution.selected {
            Some(candidate) => (candidate.executor, candidate.provider),
            None => {
                // Compatibility fallback for legacy local registrations.
                match self.executors.get(&step.capability_id).map(|e| e.clone()) {
                    Some(executor) => (executor, "direct".to_string()),
                    None => {
                        tracing::error!("PlanExecutor: executor not found for capability_id={}", step.capability_id);
                        return Err(AgentError::ExecutorNotFound(step.capability_id.clone()));
                    }
                }
            }
        };

        tracing::debug!("PlanExecutor: found executor for capability_id={}", step.capability_id);
        
        let input = substitute_variables(&step.input, &plan.variables)?;
        tracing::debug!("PlanExecutor: substituted input for step '{}': {}", step.name, input);
        
        let started_at = Utc::now();
        let started = std::time::Instant::now();

        let requested_effect = match step.capability_id.split('.').next().unwrap_or_default() {
            "memory" | "web" | "browser" | "filesystem" | "device" => RequestedEffect::Read,
            "message" | "voice" | "communication" => RequestedEffect::Communicate,
            "code" | "task" | "process" | "system" => RequestedEffect::Execute,
            _ => RequestedEffect::Transform,
        };
        let outcome_fut = self.broker.execute_v2(
            CapabilityRequestV2 {
                requested_effect,
                timeout_ms: step.timeout_ms,
                reason: Some(if step.description.trim().is_empty() { step.name.clone() } else { step.description.clone() }),
                ..CapabilityRequestV2::new(caller.to_string(), step.capability_id.clone(), input)
            },
            executor.as_ref(),
        );
        let outcome = match step.timeout_ms {
            Some(ms) => {
                tracing::debug!("PlanExecutor: executing with timeout {}ms", ms);
                match tokio::time::timeout(Duration::from_millis(ms), outcome_fut).await {
                    Ok(Ok(o)) => o,
                    Ok(Err(e)) => {
                        tracing::error!("PlanExecutor: broker error for step '{}': {}", step.id, e);
                        if e.to_string().contains("executor error") {
                            self.resolver.set_provider_health(&provider, ProviderHealth::Unavailable);
                        }
                        return Err(AgentError::BrokerError(e.to_string()));
                    }
                    Err(_) => {
                        tracing::error!("PlanExecutor: step '{}' timed out after {}ms", step.id, ms);
                        self.resolver.set_provider_health(&provider, ProviderHealth::Unavailable);
                        return Err(AgentError::BrokerError(format!(
                            "step '{}' timed out after {}ms",
                            step.id, ms
                        )));
                    }
                }
            }
            None => outcome_fut
                .await
                .map_err(|e| {
                    tracing::error!("PlanExecutor: broker error for step '{}': {}", step.id, e);
                    if e.to_string().contains("executor error") {
                        self.resolver.set_provider_health(&provider, ProviderHealth::Unavailable);
                    }
                    AgentError::BrokerError(e.to_string())
                })?,
        };

        tracing::info!("PlanExecutor: step '{}' completed, provider={}, allowed={}, executed={}, reason={:?}", step.id, provider, outcome.allowed, outcome.executed, outcome.reason);

        // Feed executor health back into the resolver. Authorization denials are
        // not provider failures; only an allowed-but-unexecuted request marks a
        // provider unavailable. A successful execution restores health.
        if outcome.allowed {
            let health = if outcome.executed {
                ProviderHealth::Available
            } else {
                ProviderHealth::Unavailable
            };
            self.resolver.set_provider_health(&provider, health);
        }

        Ok(CapabilityExecutionResult {
            step_id: step.id.clone(),
            capability_id: step.capability_id.clone(),
            success: outcome.allowed && outcome.executed,
            output: outcome.output.clone(),
            error: outcome.reason.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            started_at,
            completed_at: Utc::now(),
        })
    }

    /// Execute a step honoring the step retry policy.
    pub async fn execute_step_with_retry(
        &self,
        plan: &Plan,
        step: &PlanStep,
        caller: &str,
    ) -> Result<CapabilityExecutionResult, AgentError> {
        let mut attempt = 0u32;
        let max = step.retry_policy.max_retries;
        loop {
            match self.execute_step(plan, step, caller).await {
                Ok(result) if result.success => return Ok(result),
                Ok(result) => {
                    // Non-transient failure, don't retry.
                    if !retryable(&step.retry_policy) {
                        return Err(AgentError::BrokerError(
                            result.error.unwrap_or_else(|| "unknown failure".to_string()),
                        ));
                    }
                    if attempt >= max {
                        return Err(AgentError::BrokerError(
                            result.error.unwrap_or_else(|| "unknown failure".to_string()),
                        ));
                    }
                    attempt += 1;
                    warn!(
                        "step {} failed (attempt {}); retrying in {:?}",
                        step.id,
                        attempt,
                        backoff_delay(&step.retry_policy, attempt)
                    );
                    tokio::time::sleep(backoff_delay(&step.retry_policy, attempt)).await;
                }
                Err(e) => {
                    // Interactive confirmation is a control-flow boundary, not
                    // a transient execution failure. Retrying would spam the
                    // broker and create duplicate approval requests.
                    if e.to_string().contains("confirmation") {
                        return Err(e);
                    }
                    if !retryable(&step.retry_policy) || attempt >= max {
                        return Err(e);
                    }
                    attempt += 1;
                    warn!("step {} error: {e}; retrying", step.id);
                    tokio::time::sleep(backoff_delay(&step.retry_policy, attempt)).await;
                }
            }
        }
    }

    /// Execute a full plan sequentially, honoring step dependencies in order.
    pub async fn execute(
        &self,
        plan: &Plan,
        caller: &str,
    ) -> Result<PlanExecutionResult, AgentError> {
        tracing::info!("PlanExecutor: executing plan '{}' with {} steps, caller={}", plan.name, plan.steps.len(), caller);
        let plan_started = std::time::Instant::now();
        let mut step_results: Vec<StepResult> = Vec::with_capacity(plan.steps.len());
        let mut previous_outputs: HashMap<String, serde_json::Value> = HashMap::new();
        let mut completed_steps: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut all_success = true;

        for step in &plan.steps {
            for dependency in &step.depends_on {
                if !completed_steps.contains(dependency) {
                    return Err(AgentError::BrokerError(format!(
                        "step '{}' dependency '{}' has not completed",
                        step.id, dependency
                    )));
                }
            }
            tracing::info!("PlanExecutor: executing step '{}' (capability={})", step.name, step.capability_id);
            // Resolve variable references against previous step outputs.
            let mut enriched = plan.clone();
            enriched.variables.insert(
                format!("step_outputs"),
                serde_json::json!(previous_outputs),
            );

            let start = std::time::Instant::now();
            match self.execute_step_with_retry(&enriched, step, caller).await {
                Ok(result) => {
                    previous_outputs.insert(step.id.clone(), result.output.clone().unwrap_or(serde_json::json!(null)));
                    step_results.push(StepResult {
                        step_id: step.id.clone(),
                        success: true,
                        outcome: None,
                        error: None,
                        duration_ms: result.duration_ms,
                    });
                    completed_steps.insert(step.id.clone());
                    tracing::info!("PlanExecutor: step '{}' ok in {}ms, output={}", step.id, result.duration_ms, result.output.clone().unwrap_or(serde_json::json!(null)));
                }
                Err(e) => {
                    all_success = false;
                    tracing::error!("PlanExecutor: step '{}' failed: {}", step.id, e);
                    step_results.push(StepResult {
                        step_id: step.id.clone(),
                        success: false,
                        outcome: None,
                        error: Some(e.to_string()),
                        duration_ms: start.elapsed().as_millis() as u64,
                    });
                    if !step.continue_on_failure {
                        break;
                    }
                }
            }
        }

        Ok(PlanExecutionResult {
            plan_id: plan.id.clone(),
            success: all_success,
            step_results,
            total_duration_ms: plan_started.elapsed().as_millis() as u64,
            final_state: if all_success { AgentState::Completed } else { AgentState::Failed },
        })
    }
}

fn retryable(policy: &RetryPolicy) -> bool {
    policy.max_retries > 0 && !policy.retry_on.is_empty()
}

/// Substitute `${var}` / `${step_outputs.x}` references in a JSON value.
pub fn substitute_variables(
    value: &serde_json::Value,
    variables: &HashMap<String, serde_json::Value>,
) -> Result<serde_json::Value, AgentError> {
    let mut result = value.clone();
    substitute_recursive(&mut result, variables);
    Ok(result)
}

fn substitute_recursive(value: &mut serde_json::Value, variables: &HashMap<String, serde_json::Value>) {
    match value {
        serde_json::Value::String(s) => {
            if let Some(var_name) = s.strip_prefix("${").and_then(|s| s.strip_suffix("}")) {
                let normalized = var_name
                    .split('.')
                    .next()
                    .unwrap_or(var_name);
                let base = variables.get(normalized).or_else(|| variables.get(var_name));
                if let Some(v) = base {
                    *value = v.clone();
                }
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                substitute_recursive(v, variables);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                substitute_recursive(v, variables);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_capabilities::{
        CapabilityCategory, CapabilityDefinition, ExecutionTarget, RiskLevel,
    };
    use james_events::EventBus;
    use std::collections::HashMap;

    async fn register_test_capability(registry: &james_capabilities::CapabilityRegistry, id: &str) {
        registry
            .register(
                CapabilityDefinition {
                    id: id.to_string(),
                    name: id.to_string(),
                    category: CapabilityCategory::Custom("test".to_string()),
                    version: "1.0.0".to_string(),
                    provider: "test".to_string(),
                    description: "test".to_string(),
                    risk_level: RiskLevel::Low,
                    required_permissions: vec![],
                    dependencies: vec![],
                    input_schema: None,
                    output_schema: None,
                    execution_target: ExecutionTarget::Local,
                    tags: vec![],
                    deprecated: false,
                    experimental: false,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
    }

    fn test_plan(step_capability: &str) -> Plan {
        Plan {
            id: "plan-1".to_string(),
            name: "test plan".to_string(),
            description: "test".to_string(),
            intent_id: "intent-1".to_string(),
            steps: vec![PlanStep {
                id: "step-1".to_string(),
                name: "step 1".to_string(),
                description: "test step".to_string(),
                capability_id: step_capability.to_string(),
                input: serde_json::json!({"query": "hello"}),
                depends_on: vec![],
                estimated_duration_ms: None,
                timeout_ms: None,
                retry_policy: RetryPolicy::default(),
                continue_on_failure: false,
                metadata: StepMetadata::default(),
            }],
            variables: HashMap::new(),
            metadata: PlanMetadata::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_plan_executor_resolves_through_shared_resolver() {
        let event_bus = Arc::new(EventBus::with_default_buffer());
        let registry = Arc::new(james_capabilities::CapabilityRegistry::new());
        register_test_capability(&registry, "echo.capability").await;

        let broker = Arc::new(
            CapabilityBroker::new(registry.clone())
                .with_event_bus(event_bus.clone()),
        );

        // Shared resolver provides the executor; PlanExecutor has none locally.
        let shared = Arc::new(CapabilityResolver::new());
        shared.register_or_replace(ExecutorCandidate::new(
            "echo.capability",
            "shared-provider",
            Arc::new(james_capability_broker::NoopExecutor),
        ));

        let executor = PlanExecutor::new(broker.clone());
        executor.attach_resolver(shared.clone());
        assert!(!executor.executors.contains_key("echo.capability"));
        assert!(executor.has_executor("echo.capability"));
        assert!(executor.executor_ids().contains(&"echo.capability".to_string()));

        let plan = test_plan("echo.capability");
        let result = executor.execute(&plan, "agent:test").await.unwrap();
        assert!(result.success);
        assert_eq!(result.step_results.len(), 1);
        assert!(result.step_results[0].success);

        // Local registration also feeds the resolver.
        executor.register_executor("other.capability", Arc::new(james_capability_broker::NoopExecutor));
        assert!(executor.executor_ids().contains(&"other.capability".to_string()));
    }

    #[tokio::test]
    async fn test_plan_executor_broker_denies_then_allows_python_style_executor() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct CountingExecutor {
            calls: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl CapabilityExecutor for CountingExecutor {
            async fn execute(
                &self,
                _capability_id: &str,
                _input: serde_json::Value,
            ) -> anyhow::Result<serde_json::Value> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::json!({"source": "python-bridge-contract", "ok": true}))
            }
        }

        let event_bus = Arc::new(EventBus::with_default_buffer());
        let registry = Arc::new(james_capabilities::CapabilityRegistry::new());
        registry
            .register(
                CapabilityDefinition {
                    id: "web.search".to_string(),
                    name: "Web Search".to_string(),
                    category: CapabilityCategory::Network,
                    version: "1.0.0".to_string(),
                    provider: "python".to_string(),
                    description: "test Python bridge capability".to_string(),
                    risk_level: RiskLevel::Medium,
                    required_permissions: vec!["network.public_web".to_string()],
                    dependencies: vec![],
                    input_schema: None,
                    output_schema: None,
                    execution_target: ExecutionTarget::Local,
                    tags: vec!["python".to_string(), "bridge".to_string()],
                    deprecated: false,
                    experimental: true,
                },
                "python-bridge",
            )
            .await
            .unwrap();

        let broker = Arc::new(CapabilityBroker::new(registry.clone()).with_event_bus(event_bus));
        let calls = Arc::new(AtomicUsize::new(0));
        let bridge_executor: Arc<dyn CapabilityExecutor> =
            Arc::new(CountingExecutor { calls: calls.clone() });

        let executor = PlanExecutor::new(broker.clone());
        executor.register_executor("web.search", bridge_executor);

        let plan = test_plan("web.search");

        let denied = executor.execute(&plan, "user").await.unwrap();
        assert!(!denied.success);
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        broker.grant_capability_permissions("user", "web.search");

        let allowed = executor.execute(&plan, "user").await.unwrap();
        assert!(allowed.success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            allowed.step_results[0].success,
            true,
            "broker approval must reach the executor"
        );
    }

    #[tokio::test]
    async fn test_plan_executor_marks_failed_provider_unavailable_and_fails_over() {
        struct FailingExecutor;

        #[async_trait::async_trait]
        impl CapabilityExecutor for FailingExecutor {
            async fn execute(
                &self,
                capability_id: &str,
                _input: serde_json::Value,
            ) -> anyhow::Result<serde_json::Value> {
                Err(anyhow::anyhow!("simulated {} provider outage", capability_id))
            }
        }

        let registry = Arc::new(james_capabilities::CapabilityRegistry::new());
        register_test_capability(&registry, "failover.capability").await;
        let broker = Arc::new(CapabilityBroker::new(registry).without_audit());

        let shared = Arc::new(CapabilityResolver::new());
        shared.register(ExecutorCandidate::new(
            "failover.capability",
            "python",
            Arc::new(FailingExecutor),
        ));
        shared.register(ExecutorCandidate::new(
            "failover.capability",
            "backup",
            Arc::new(james_capability_broker::NoopExecutor),
        ));

        let executor = PlanExecutor::new(broker);
        executor.attach_resolver(shared);
        let plan = test_plan("failover.capability");

        let first = executor.execute(&plan, "agent:test").await.unwrap();
        assert!(!first.success);
        assert_eq!(
            executor.resolver().provider_health("python"),
            ProviderHealth::Unavailable
        );

        let second = executor.execute(&plan, "agent:test").await.unwrap();
        assert!(second.success);
        assert_eq!(
            executor.resolver().resolve("failover.capability", &ResolutionContext::default())
                .selected.unwrap().provider,
            "backup"
        );
    }

    #[tokio::test]
    async fn test_plan_executor_fails_cleanly_without_resolver_entry() {
        let registry = Arc::new(james_capabilities::CapabilityRegistry::new());
        register_test_capability(&registry, "missing.executor").await;
        let broker = Arc::new(CapabilityBroker::new(registry.clone()).without_audit());
        let executor = PlanExecutor::new(broker);

        let mut plan = test_plan("missing.executor");
        plan.steps[0].retry_policy = RetryPolicy {
            max_retries: 0,
            ..RetryPolicy::default()
        };
        let result = executor.execute(&plan, "agent:test").await.unwrap();
        assert!(!result.success);
        let error = result.step_results[0]
            .error
            .as_deref()
            .expect("step should carry an error");
        assert!(
            error.contains("Executor not found for capability: missing.executor"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_executor_registry_seeds_resolver() {
        let reg = crate::ExecutorRegistry::with_provider("platform");
        let ex: Arc<dyn CapabilityExecutor> = Arc::new(james_capability_broker::NoopExecutor);
        reg.register("platform.example", ex);
        let resolver = Arc::new(CapabilityResolver::new());
        reg.seed(&resolver);
        assert!(resolver.resolve_executor("platform.example", &ResolutionContext::default()).is_some());
        assert_eq!(
            resolver.candidates("platform.example")[0].provider,
            "platform"
        );
    }
}