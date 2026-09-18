//! Agent model definitions — UserIntent, Plan, PlanStep, CapabilityRequest

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A user's high-level intent
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserIntent {
    pub id: String,
    pub user_id: String,
    pub raw_input: String,
    pub parsed_intent: ParsedIntent,
    pub context: IntentContext,
    pub created_at: DateTime<Utc>,
    pub priority: IntentPriority,
}

impl UserIntent {
    pub fn new(user_id: String, raw_input: String, parsed_intent: ParsedIntent) -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            user_id,
            raw_input,
            parsed_intent,
            context: IntentContext::default(),
            created_at: Utc::now(),
            priority: IntentPriority::Normal,
        }
    }
}

/// Parsed intent from user input
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParsedIntent {
    pub action: String,
    pub target: Option<String>,
    pub parameters: HashMap<String, serde_json::Value>,
    pub constraints: Vec<IntentConstraint>,
    pub expected_output: Option<String>,
}

/// Intent constraints
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentConstraint {
    pub constraint_type: ConstraintType,
    pub value: serde_json::Value,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    TimeBudget,
    CostBudget,
    QualityLevel,
    PrivacyLevel,
    ResourceLimit,
}

/// Intent context
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct IntentContext {
    pub conversation_id: Option<String>,
    pub session_id: Option<String>,
    pub active_agents: Vec<String>,
    pub available_capabilities: Vec<String>,
    pub user_preferences: HashMap<String, serde_json::Value>,
}

/// Intent priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntentPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// A plan consisting of ordered steps
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    pub id: String,
    pub name: String,
    pub description: String,
    pub intent_id: String,
    pub steps: Vec<PlanStep>,
    pub variables: HashMap<String, serde_json::Value>,
    pub metadata: PlanMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Plan {
    pub fn new(name: String, description: String, intent_id: String, steps: Vec<PlanStep>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7().to_string(),
            name,
            description,
            intent_id,
            steps,
            variables: HashMap::new(),
            metadata: PlanMetadata::default(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add_variable(&mut self, key: String, value: serde_json::Value) {
        self.variables.insert(key, value);
        self.updated_at = Utc::now();
    }

    pub fn total_estimated_duration_ms(&self) -> u64 {
        self.steps.iter().map(|s| s.estimated_duration_ms.unwrap_or(0)).sum()
    }
}

/// Plan metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PlanMetadata {
    pub tags: Vec<String>,
    pub estimated_cost_usd: Option<f64>,
    pub estimated_duration_ms: Option<u64>,
    pub required_capabilities: Vec<String>,
    pub rollback_plan_id: Option<String>,
}

/// A single step in a plan
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanStep {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capability_id: String,
    pub input: serde_json::Value,
    pub depends_on: Vec<String>,
    pub estimated_duration_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub retry_policy: RetryPolicy,
    pub continue_on_failure: bool,
    pub metadata: StepMetadata,
}

impl PlanStep {
    pub fn new(name: String, capability_id: String, input: serde_json::Value) -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            name,
            description: String::new(),
            capability_id,
            input,
            depends_on: Vec::new(),
            estimated_duration_ms: None,
            timeout_ms: None,
            retry_policy: RetryPolicy::default(),
            continue_on_failure: false,
            metadata: StepMetadata::default(),
        }
    }

    pub fn with_dependency(mut self, step_id: impl Into<String>) -> Self {
        self.depends_on.push(step_id.into());
        self
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}

/// Step metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct StepMetadata {
    pub tags: Vec<String>,
    pub expected_output_schema: Option<serde_json::Value>,
    pub side_effects: Vec<String>,
}

/// Retry policy for steps
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_base: f64,
    pub retry_on: Vec<RetryCondition>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            exponential_base: 2.0,
            retry_on: vec![RetryCondition::TransientError, RetryCondition::Timeout],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetryCondition {
    TransientError,
    Timeout,
    RateLimited,
    Unavailable,
    Any,
}

/// Execution context passed to steps
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionContext {
    pub plan_id: String,
    pub step_id: String,
    pub agent_id: String,
    pub variables: HashMap<String, serde_json::Value>,
    pub previous_results: HashMap<String, serde_json::Value>,
    pub started_at: DateTime<Utc>,
}

/// Result of a capability execution within a plan
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityExecutionResult {
    pub step_id: String,
    pub capability_id: String,
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

/// Plan execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanExecutionStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    PartiallyCompleted,
}