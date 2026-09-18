//! JAMES Agents — typed Agent/Plan/CapabilityRequest model and runtime.
//!
//! This crate implements the structured execution pipeline:
//! `UserIntent → Plan → PlanStep → CapabilityRequest → Broker → Executor → Result`

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use indexmap::IndexMap;
use james_capability_broker::{CapabilityBroker, CapabilityExecutor, CapabilityOutcome, CapabilityOutcomeV2, CapabilityRequestV2, RequestedEffect};
use james_capabilities::CapabilityRegistry;
use james_events::{Event, EventBus, builtin_events, create_system_event};
use james_identity::IdentityRegistry;
use james_memory::MemoryModule;
use james_tasks::TaskManager;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::debug;
use uuid::Uuid;

pub mod model;
pub mod planner;
pub mod executor;
pub mod registry;
pub mod resolver;

pub use model::*;
pub use planner::*;
pub use executor::*;
pub use registry::*;
pub use resolver::*;

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub role: AgentRole,
    pub goal: Option<String>,
    pub allowed_capabilities: Vec<String>,
    pub max_concurrent_tasks: usize,
    pub timeout_secs: u64,
    pub budget_usd: Option<f64>,
    pub workspace_id: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            name: "unnamed-agent".to_string(),
            role: AgentRole::Generic,
            goal: None,
            allowed_capabilities: vec!["*".to_string()],
            max_concurrent_tasks: 5,
            timeout_secs: 300,
            budget_usd: None,
            workspace_id: None,
        }
    }
}

/// Agent role classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Generic,
    Researcher,
    Coder,
    Browser,
    Analyst,
    Planner,
    Executor,
    Supervisor,
}

/// Agent state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Initializing,
    Running,
    Waiting,
    Blocked,
    Paused,
    Completed,
    Failed,
    Terminated,
}

/// An autonomous agent that can execute plans
pub struct Agent {
    config: AgentConfig,
    state: Arc<RwLock<AgentState>>,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    broker: Arc<CapabilityBroker>,
    identity_registry: Arc<IdentityRegistry>,
    memory: Arc<MemoryModule>,
    tasks: Arc<TaskManager>,
    executors: Arc<DashMap<String, Arc<dyn CapabilityExecutor>>>,
    resolver: Arc<CapabilityResolver>,
    /// Optional assembly-wide resolver. Kept separately so local agent
    /// registrations remain local while shared candidates can be refreshed.
    shared_resolver: Option<Arc<CapabilityResolver>>,
    current_plan: Arc<RwLock<Option<Plan>>>,
    step_results: Arc<RwLock<IndexMap<String, CapabilityOutcome>>>,
    metrics: Arc<RwLock<AgentMetrics>>,
}

/// Agent runtime metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub plans_executed: u64,
    pub steps_completed: u64,
    pub steps_failed: u64,
    pub total_execution_time_ms: u64,
    pub capabilities_used: HashMap<String, u64>,
    pub last_activity: Option<DateTime<Utc>>,
}

impl Agent {
    /// Create a new agent with the given configuration and dependencies
    pub fn new(
        config: AgentConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        broker: Arc<CapabilityBroker>,
        identity_registry: Arc<IdentityRegistry>,
        memory: Arc<MemoryModule>,
        tasks: Arc<TaskManager>,
    ) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(AgentState::Idle)),
            event_bus,
            capability_registry,
            broker,
            identity_registry,
            memory,
            tasks,
            executors: Arc::new(DashMap::new()),
            resolver: Arc::new(CapabilityResolver::new()),
            shared_resolver: None,
            current_plan: Arc::new(RwLock::new(None)),
            step_results: Arc::new(RwLock::new(IndexMap::new())),
            metrics: Arc::new(RwLock::new(AgentMetrics::default())),
        }
    }

    /// Register a capability executor for this agent
    pub fn register_executor(&self, capability_id: &str, executor: Arc<dyn CapabilityExecutor>) {
        self.executors.insert(capability_id.to_string(), executor.clone());
        self.resolver.register_or_replace(ExecutorCandidate::new(
            capability_id,
            "direct",
            executor,
        ));
    }

    /// Attach a shared capability resolver. Its candidates are merged into the
    /// agent's resolver; the agent's own registrations are layered underneath.
    pub fn attach_resolver(&mut self, resolver: Arc<CapabilityResolver>) {
        self.shared_resolver = Some(resolver.clone());
        self.sync_shared_resolver(&resolver);
    }

    /// Refresh shared candidates immediately before execution. This keeps
    /// long-lived agents synchronized with runtime module/provider changes
    /// without sharing the mutable candidate store with agent-local overrides.
    fn sync_shared_resolver(&self, resolver: &CapabilityResolver) {
        for id in resolver.ids() {
            for candidate in resolver.candidates(&id) {
                self.resolver.register_or_replace(candidate);
            }
        }
    }

    /// The agent's resolver (candidates + selection logic).
    pub fn resolver(&self) -> Arc<CapabilityResolver> {
        self.resolver.clone()
    }

    /// Get the agent's current state
    pub async fn state(&self) -> AgentState {
        *self.state.read().await
    }

    /// Get the agent's configuration
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Execute a plan
    pub async fn execute_plan(&self, plan: Plan) -> Result<PlanExecutionResult, AgentError> {
        // Validate plan
        self.validate_plan(&plan).await?;

        // Set state to running
        *self.state.write().await = AgentState::Running;
        *self.current_plan.write().await = Some(plan.clone());
        self.step_results.write().await.clear();

        // Emit plan started event
        self.emit_event(create_system_event(builtin_events::TASK_STARTED, "james-agents")
            .with_payload(serde_json::json!({
                "agent_id": self.config.id,
                "plan_id": plan.id,
                "plan_name": plan.name,
            }))).await?;

        let start_time = std::time::Instant::now();
        let mut step_results = Vec::new();
        let mut all_success = true;

        // Execute each step in order
        for step in &plan.steps {
            // Check if agent should continue
            if *self.state.read().await != AgentState::Running {
                all_success = false;
                break;
            }

            *self.state.write().await = AgentState::Running;

            let step_start = std::time::Instant::now();
            let result = self.execute_step(&plan, step).await;
            let step_duration = step_start.elapsed().as_millis() as u64;

            match result {
                Ok(outcome) => {
                    self.step_results.write().await.insert(step.id.clone(), outcome.clone());
                    step_results.push(StepResult {
                        step_id: step.id.clone(),
                        success: true,
                        outcome: Some(outcome),
                        error: None,
                        duration_ms: step_duration,
                    });
                    
                    // Update metrics
                    let mut metrics = self.metrics.write().await;
                    metrics.steps_completed += 1;
                    metrics.total_execution_time_ms += step_duration;
                    *metrics.capabilities_used.entry(step.capability_id.clone()).or_insert(0) += 1;
                    metrics.last_activity = Some(Utc::now());
                }
                Err(e) => {
                    all_success = false;
                    step_results.push(StepResult {
                        step_id: step.id.clone(),
                        success: false,
                        outcome: None,
                        error: Some(e.to_string()),
                        duration_ms: step_duration,
                    });

                    let mut metrics = self.metrics.write().await;
                    metrics.steps_failed += 1;
                    metrics.last_activity = Some(Utc::now());

                    // Emit step failed event
                    self.emit_event(create_system_event("agent.step.failed", "james-agents")
                        .with_payload(serde_json::json!({
                            "agent_id": self.config.id,
                            "plan_id": plan.id,
                            "step_id": step.id,
                            "error": e.to_string(),
                        }))).await?;

                    // Check if plan should continue on failure
                    if !step.continue_on_failure {
                        break;
                    }
                }
            }
        }

        let total_duration = start_time.elapsed().as_millis() as u64;
        let final_state = if all_success { AgentState::Completed } else { AgentState::Failed };
        *self.state.write().await = final_state;

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.plans_executed += 1;
            metrics.total_execution_time_ms += total_duration;
        }

        // Emit plan completed event
        self.emit_event(create_system_event(if all_success { builtin_events::TASK_COMPLETED } else { builtin_events::TASK_FAILED }, "james-agents")
            .with_payload(serde_json::json!({
                "agent_id": self.config.id,
                "plan_id": plan.id,
                "success": all_success,
                "duration_ms": total_duration,
                "steps_completed": step_results.iter().filter(|r| r.success).count(),
                "steps_failed": step_results.iter().filter(|r| !r.success).count(),
            }))).await?;

        Ok(PlanExecutionResult {
            plan_id: plan.id,
            success: all_success,
            step_results,
            total_duration_ms: total_duration,
            final_state,
        })
    }

    /// Execute a single plan step
    async fn execute_step(&self, plan: &Plan, step: &PlanStep) -> Result<CapabilityOutcome, AgentError> {
        if let Some(shared) = &self.shared_resolver {
            self.sync_shared_resolver(shared);
        }

        debug!("Agent {} executing step {}: {}", self.config.id, step.id, step.capability_id);

        // Resolve executor via the capability resolver; fall back to the
        // legacy per-agent map (Block C resolution then broker enforcement).
        let executor = self
            .resolver
            .resolve_executor(&step.capability_id, &ResolutionContext::default())
            .or_else(|| self.executors.get(&step.capability_id).map(|e| e.clone()))
            .ok_or_else(|| AgentError::ExecutorNotFound(step.capability_id.clone()))?;

        // Prepare input with variable substitution
        let input = self.substitute_variables(&step.input, &plan.variables)?;

        // The agent's allowed-capabilities list is the agent-level authorization
        // boundary. Convert explicit entries into broker grants once per step;
        // wildcard agents intentionally do not receive implicit grants.
        let caller = format!("agent:{}", self.config.id);
        if self.config.allowed_capabilities.iter().any(|cap| cap == &step.capability_id) {
            self.broker.grant_capability_permissions(caller.clone(), &step.capability_id);
        }

        // All typed-agent execution now uses the structured v2 broker path.
        // This preserves request/correlation metadata, confirmation binding,
        // resource admission, semantic verification, deadlines/timeouts and
        // the v2 audit trail instead of falling back to the legacy path.
        let request = CapabilityRequestV2 {
            requested_effect: requested_effect_for(&step.capability_id),
            timeout_ms: Some(self.config.timeout_secs.saturating_mul(1000)),
            ..CapabilityRequestV2::new(caller, step.capability_id.clone(), input)
        };

        let outcome = self.broker.execute_v2(request, executor.as_ref()).await
            .map_err(|e| AgentError::BrokerError(e.to_string()))?;

        Ok(legacy_outcome(outcome))
    }

fn requested_effect_for(capability_id: &str) -> RequestedEffect {
    if capability_id.starts_with("memory.read")
        || capability_id.starts_with("memory.")
        || capability_id.ends_with(".read")
        || capability_id.ends_with(".fetch")
        || capability_id.ends_with(".extract")
        || capability_id.ends_with(".search")
    {
        return RequestedEffect::Read;
    }
    if capability_id.ends_with(".write")
        || capability_id.ends_with(".create")
        || capability_id.ends_with(".delete")
        || capability_id.starts_with("filesystem.")
    {
        return RequestedEffect::Write;
    }
    if capability_id.ends_with(".execute") || capability_id == "code.execute" {
        return RequestedEffect::Execute;
    }
    if capability_id.starts_with("browser.")
        || capability_id.starts_with("voice.")
        || capability_id.starts_with("tts.")
        || capability_id.starts_with("stt.")
        || capability_id == "void.chat"
    {
        return RequestedEffect::Communicate;
    }
    RequestedEffect::Transform
}

fn legacy_outcome(outcome: CapabilityOutcomeV2) -> CapabilityOutcome {
    CapabilityOutcome {
        caller: outcome.caller_identity,
        capability_id: outcome.capability_id,
        allowed: outcome.allowed,
        decision: outcome.decision,
        executed: outcome.executed,
        output: outcome.output,
        input_verified: outcome.input_verified,
        output_verified: outcome.output_verified,
        duration_ms: outcome.duration_ms,
        reason: outcome.reason,
        audited: outcome.audited,
    }
}

    /// Validate a plan before execution
    async fn validate_plan(&self, plan: &Plan) -> Result<(), AgentError> {
        // Check all capabilities are allowed for this agent
        for step in &plan.steps {
            if !self.is_capability_allowed(&step.capability_id) {
                return Err(AgentError::CapabilityNotAllowed(step.capability_id.clone()));
            }

            // Check if an executor is resolvable
            let has_executor = self
                .resolver
                .resolve_executor(&step.capability_id, &ResolutionContext::default())
                .is_some()
                || self.executors.contains_key(&step.capability_id);
            if !has_executor {
                return Err(AgentError::ExecutorNotFound(step.capability_id.clone()));
            }
        }
        Ok(())
    }

    /// Check if a capability is allowed for this agent
    fn is_capability_allowed(&self, capability_id: &str) -> bool {
        self.config.allowed_capabilities.iter().any(|c| c == "*" || c == capability_id)
    }

    /// Substitute variables in input JSON
    fn substitute_variables(&self, input: &serde_json::Value, variables: &HashMap<String, serde_json::Value>) -> Result<serde_json::Value, AgentError> {
        let mut result = input.clone();
        Self::substitute_recursive(&mut result, variables);
        Ok(result)
    }

    fn substitute_recursive(value: &mut serde_json::Value, variables: &HashMap<String, serde_json::Value>) {
        match value {
            serde_json::Value::String(s) => {
                if let Some(var_name) = s.strip_prefix("${").and_then(|s| s.strip_suffix("}")) {
                    if let Some(var_value) = variables.get(var_name) {
                        *value = var_value.clone();
                    }
                }
            }
            serde_json::Value::Object(map) => {
                for v in map.values_mut() {
                    Self::substitute_recursive(v, variables);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr.iter_mut() {
                    Self::substitute_recursive(v, variables);
                }
            }
            _ => {}
        }
    }

    /// Emit an event to the bus
    async fn emit_event(&self, event: Event) -> anyhow::Result<()> {
        self.event_bus.publish(event).await
    }

    /// Get agent metrics
    pub async fn metrics(&self) -> AgentMetrics {
        self.metrics.read().await.clone()
    }

    /// Pause agent execution
    pub async fn pause(&self) {
        *self.state.write().await = AgentState::Paused;
    }

    /// Resume agent execution
    pub async fn resume(&self) {
        if *self.state.read().await == AgentState::Paused {
            *self.state.write().await = AgentState::Running;
        }
    }

    /// Stop agent execution
    pub async fn stop(&self) {
        *self.state.write().await = AgentState::Terminated;
    }
}

/// Result of a plan step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub success: bool,
    pub outcome: Option<CapabilityOutcome>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Result of a full plan execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanExecutionResult {
    pub plan_id: String,
    pub success: bool,
    pub step_results: Vec<StepResult>,
    pub total_duration_ms: u64,
    pub final_state: AgentState,
}

/// Agent errors
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Capability not allowed for this agent: {0}")]
    CapabilityNotAllowed(String),
    #[error("Executor not found for capability: {0}")]
    ExecutorNotFound(String),
    #[error("Broker error: {0}")]
    BrokerError(String),
    #[error("Plan validation failed: {0}")]
    PlanValidationFailed(String),
    #[error("Variable substitution failed: {0}")]
    VariableSubstitutionFailed(String),
    #[error("Agent is not in a valid state for this operation")]
    InvalidState,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Agent factory for creating configured agents
pub struct AgentFactory {
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    broker: Arc<CapabilityBroker>,
    identity_registry: Arc<IdentityRegistry>,
    memory: Arc<MemoryModule>,
    tasks: Arc<TaskManager>,
    executor_registry: Arc<ExecutorRegistry>,
    shared_resolver: Option<Arc<CapabilityResolver>>,
}

impl AgentFactory {
    pub fn new(
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        broker: Arc<CapabilityBroker>,
        identity_registry: Arc<IdentityRegistry>,
        memory: Arc<MemoryModule>,
        tasks: Arc<TaskManager>,
        executor_registry: Arc<ExecutorRegistry>,
    ) -> Self {
        Self {
            event_bus,
            capability_registry,
            broker,
            identity_registry,
            memory,
            tasks,
            executor_registry,
            shared_resolver: None,
        }
    }

    /// Attach the assembly/shared resolver so factory-created typed agents
    /// use the same live executor candidates as direct plan execution.
    pub fn attach_resolver(&mut self, resolver: Arc<CapabilityResolver>) {
        self.shared_resolver = Some(resolver);
    }

    fn finish_agent(&self, mut agent: Agent) -> Agent {
        if let Some(resolver) = &self.shared_resolver {
            agent.attach_resolver(resolver.clone());
        }
        agent
    }

    /// Create a researcher agent
    pub fn create_researcher(&self, name: &str) -> Agent {
        let config = AgentConfig {
            id: Uuid::now_v7().to_string(),
            name: name.to_string(),
            role: AgentRole::Researcher,
            goal: Some("Research and gather information".to_string()),
            allowed_capabilities: vec![
                "web.search".to_string(),
                "web.fetch".to_string(),
                "memory.read".to_string(),
                "memory.write".to_string(),
            ],
            max_concurrent_tasks: 3,
            timeout_secs: 600,
            budget_usd: Some(10.0),
            workspace_id: None,
        };
        let agent = Agent::new(config, self.event_bus.clone(), self.capability_registry.clone(),
            self.broker.clone(), self.identity_registry.clone(), self.memory.clone(), self.tasks.clone());
        
        // Register executors
        for cap in &agent.config.allowed_capabilities {
            if let Some(executor) = self.executor_registry.get(cap) {
                agent.register_executor(cap, executor);
            }
        }
        self.finish_agent(agent)
    }

    /// Create a coder agent
    pub fn create_coder(&self, name: &str) -> Agent {
        let config = AgentConfig {
            id: Uuid::now_v7().to_string(),
            name: name.to_string(),
            role: AgentRole::Coder,
            goal: Some("Write, analyze, and refactor code".to_string()),
            allowed_capabilities: vec![
                "code.read".to_string(),
                "code.write".to_string(),
                "code.execute".to_string(),
                "filesystem.read".to_string(),
                "filesystem.write".to_string(),
                "memory.read".to_string(),
                "memory.write".to_string(),
            ],
            max_concurrent_tasks: 2,
            timeout_secs: 600,
            budget_usd: Some(5.0),
            workspace_id: None,
        };
        let agent = Agent::new(config, self.event_bus.clone(), self.capability_registry.clone(),
            self.broker.clone(), self.identity_registry.clone(), self.memory.clone(), self.tasks.clone());
        
        for cap in &agent.config.allowed_capabilities {
            if let Some(executor) = self.executor_registry.get(cap) {
                agent.register_executor(cap, executor);
            }
        }
        self.finish_agent(agent)
    }

    /// Create a browser agent
    pub fn create_browser(&self, name: &str) -> Agent {
        let config = AgentConfig {
            id: Uuid::now_v7().to_string(),
            name: name.to_string(),
            role: AgentRole::Browser,
            goal: Some("Automate browser interactions".to_string()),
            allowed_capabilities: vec![
                "browser.navigate".to_string(),
                "browser.click".to_string(),
                "browser.type".to_string(),
                "browser.screenshot".to_string(),
                "browser.extract".to_string(),
                "memory.read".to_string(),
                "memory.write".to_string(),
            ],
            max_concurrent_tasks: 1,
            timeout_secs: 300,
            budget_usd: Some(2.0),
            workspace_id: None,
        };
        let agent = Agent::new(config, self.event_bus.clone(), self.capability_registry.clone(),
            self.broker.clone(), self.identity_registry.clone(), self.memory.clone(), self.tasks.clone());
        
        for cap in &agent.config.allowed_capabilities {
            if let Some(executor) = self.executor_registry.get(cap) {
                agent.register_executor(cap, executor);
            }
        }
        self.finish_agent(agent)
    }

    /// Create a generic agent with custom config
    pub fn create_custom(&self, config: AgentConfig) -> Agent {
        let agent = Agent::new(config, self.event_bus.clone(), self.capability_registry.clone(),
            self.broker.clone(), self.identity_registry.clone(), self.memory.clone(), self.tasks.clone());
        
        for cap in &agent.config.allowed_capabilities {
            if let Some(executor) = self.executor_registry.get(cap) {
                agent.register_executor(cap, executor);
            }
        }
        self.finish_agent(agent)
    }
}

/// Global agent registry
pub struct AgentRegistry {
    agents: DashMap<String, Arc<Agent>>,
    factory: Arc<AgentFactory>,
}

impl AgentRegistry {
    pub fn new(factory: Arc<AgentFactory>) -> Self {
        Self {
            agents: DashMap::new(),
            factory,
        }
    }

    /// Register an agent
    pub fn register(&self, agent: Arc<Agent>) -> String {
        let id = agent.config().id.clone();
        self.agents.insert(id.clone(), agent);
        id
    }

    /// Get an agent by ID
    pub fn get(&self, id: &str) -> Option<Arc<Agent>> {
        self.agents.get(id).map(|a| a.clone())
    }

    /// List all agents
    pub fn list(&self) -> Vec<Arc<Agent>> {
        self.agents.iter().map(|a| a.clone()).collect()
    }

    /// Remove an agent
    pub fn remove(&self, id: &str) -> Option<Arc<Agent>> {
        self.agents.remove(id).map(|(_, a)| a)
    }

    /// Create and register a researcher agent
    pub fn create_researcher(&self, name: &str) -> Arc<Agent> {
        let agent = Arc::new(self.factory.create_researcher(name));
        self.register(agent.clone());
        agent
    }

    /// Create and register a coder agent
    pub fn create_coder(&self, name: &str) -> Arc<Agent> {
        let agent = Arc::new(self.factory.create_coder(name));
        self.register(agent.clone());
        agent
    }

    /// Create and register a browser agent
    pub fn create_browser(&self, name: &str) -> Arc<Agent> {
        let agent = Arc::new(self.factory.create_browser(name));
        self.register(agent.clone());
        agent
    }

    /// Create and register a custom agent from a config
    pub fn create_custom(&self, config: AgentConfig) -> Arc<Agent> {
        let agent = Arc::new(self.factory.create_custom(config));
        self.register(agent.clone());
        agent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_factory() -> AgentFactory {
        let event_bus = Arc::new(EventBus::with_default_buffer());
        let capability_registry = Arc::new(CapabilityRegistry::new());
        let broker = Arc::new(CapabilityBroker::new(capability_registry.clone()).without_audit());
        let identity_registry = Arc::new(IdentityRegistry::new());
        let memory = Arc::new(MemoryModule::new(
            james_memory::MemoryConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        ));
        let tasks = Arc::new(TaskManager::new(Some(event_bus.clone())));
        let executor_registry = Arc::new(ExecutorRegistry::new());
        
        AgentFactory::new(event_bus, capability_registry, broker, identity_registry, memory, tasks, executor_registry)
    }

    #[tokio::test]
    async fn test_agent_creation() {
        let factory = create_test_factory();
        let agent = factory.create_researcher("test-researcher");
        assert_eq!(agent.config().role, AgentRole::Researcher);
        assert_eq!(agent.state().await, AgentState::Idle);
    }

    #[tokio::test]
    async fn test_agent_factory_creates_different_types() {
        let factory = create_test_factory();
        
        let researcher = factory.create_researcher("r1");
        assert_eq!(researcher.config().role, AgentRole::Researcher);
        
        let coder = factory.create_coder("c1");
        assert_eq!(coder.config().role, AgentRole::Coder);
        
        let browser = factory.create_browser("b1");
        assert_eq!(browser.config().role, AgentRole::Browser);
    }

    #[tokio::test]
    async fn test_agent_registry() {
        let factory = Arc::new(create_test_factory());
        let registry = AgentRegistry::new(factory);
        
        let agent1 = registry.create_researcher("r1");
        let agent2 = registry.create_coder("c1");
        
        let agents = registry.list();
        assert_eq!(agents.len(), 2);
        
        let found = registry.get(&agent1.config().id);
        assert!(found.is_some());
        
        let removed = registry.remove(&agent1.config().id);
        assert!(removed.is_some());
        assert_eq!(registry.list().len(), 1);
    }
}