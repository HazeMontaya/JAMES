//! Agent planner — converts UserIntent into executable Plans

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;
use uuid::Uuid;

use crate::model::*;
use james_capabilities::CapabilityRegistry;
use james_events::EventBus;
use crate::{AgentError, AgentConfig};

/// Planner trait for converting intents to plans
#[async_trait]
pub trait Planner: Send + Sync {
    async fn create_plan(&self, intent: UserIntent) -> Result<Plan, AgentError>;
    async fn refine_plan(&self, plan: &mut Plan, feedback: &PlanFeedback) -> Result<(), AgentError>;
    async fn estimate_plan(&self, plan: &Plan) -> PlanEstimate;
}

/// Feedback for plan refinement
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanFeedback {
    pub plan_id: String,
    pub step_id: Option<String>,
    pub feedback_type: FeedbackType,
    pub message: String,
    pub suggested_changes: Vec<SuggestedChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    MissingStep,
    IncorrectCapability,
    WrongOrder,
    MissingDependency,
    Optimization,
    ConstraintViolation,
}

/// Suggested change for plan refinement
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SuggestedChange {
    pub change_type: ChangeType,
    pub step_id: Option<String>,
    pub new_step: Option<PlanStep>,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    AddStep,
    RemoveStep,
    ReplaceStep,
    ReorderSteps,
    ModifyStep,
}

/// Plan estimation result
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanEstimate {
    pub total_steps: usize,
    pub estimated_duration_ms: u64,
    pub estimated_cost_usd: Option<f64>,
    pub required_capabilities: Vec<String>,
    pub confidence: f64,
    pub warnings: Vec<String>,
}

/// LLM-based planner using AI module
pub struct LlmPlanner {
    config: AgentConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    ai_module: Arc<dyn AiPlannerProvider>,
    planning_templates: Arc<RwLock<HashMap<String, PlanningTemplate>>>,
}

/// AI provider trait for planning
#[async_trait]
pub trait AiPlannerProvider: Send + Sync {
    async fn generate_plan(&self, prompt: &str) -> Result<String, AgentError>;
    async fn refine_plan(&self, plan: &Plan, feedback: &PlanFeedback) -> Result<Plan, AgentError>;
}

/// Planning template for common patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningTemplate {
    pub name: String,
    pub pattern: String,
    pub steps: Vec<PlanStepTemplate>,
    pub variables: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStepTemplate {
    pub name: String,
    pub capability_id: String,
    pub input_template: serde_json::Value,
    pub depends_on: Vec<String>,
    pub estimated_duration_ms: Option<u64>,
}

impl LlmPlanner {
    pub fn new(
        config: AgentConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
        ai_module: Arc<dyn AiPlannerProvider>,
    ) -> Self {
        Self {
            config,
            event_bus,
            capability_registry,
            ai_module,
            planning_templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a planning template
    pub async fn register_template(&self, template: PlanningTemplate) {
        self.planning_templates.write().await.insert(template.name.clone(), template);
    }

    /// Create a plan from user intent
    pub async fn create_plan(&self, intent: UserIntent) -> Result<Plan, AgentError> {
        debug!("Creating plan for intent: {}", intent.id);

        // Check for matching template first
        if let Some(plan) = self.try_template_planning(&intent).await? {
            return Ok(plan);
        }

        // Fall back to LLM-based planning
        self.llm_planning(&intent).await
    }

    /// Try to create plan from registered templates
    async fn try_template_planning(&self, intent: &UserIntent) -> Result<Option<Plan>, AgentError> {
        let templates = self.planning_templates.read().await;
        
        for template in templates.values() {
            if self.template_matches(template, intent) {
                let plan = self.instantiate_template(template, intent).await?;
                return Ok(Some(plan));
            }
        }
        Ok(None)
    }

    /// Check if template matches intent
    fn template_matches(&self, template: &PlanningTemplate, intent: &UserIntent) -> bool {
        // Simple pattern matching - could be enhanced with embeddings
        intent.raw_input.to_lowercase().contains(&template.pattern.to_lowercase())
    }

    /// Instantiate a template into a concrete plan
    async fn instantiate_template(&self, template: &PlanningTemplate, intent: &UserIntent) -> Result<Plan, AgentError> {
        let mut steps = Vec::new();
        let mut variables = HashMap::new();

        // Extract variables from intent
        for (key, value) in &intent.parsed_intent.parameters {
            variables.insert(key.clone(), value.clone());
        }

        // Add intent as variable
        variables.insert(
            "intent".to_string(),
            serde_json::to_value(&intent.parsed_intent)
                .map_err(|e| AgentError::PlanValidationFailed(e.to_string()))?,
        );
        variables.insert(
            "user_id".to_string(),
            serde_json::to_value(&intent.user_id)
                .map_err(|e| AgentError::PlanValidationFailed(e.to_string()))?,
        );

        // Create steps from template
        for step_template in &template.steps {
            let mut input = step_template.input_template.clone();
            
            // Substitute variables in input
            self.substitute_variables(&mut input, &variables);

            let step = PlanStep {
                id: Uuid::now_v7().to_string(),
                name: step_template.name.clone(),
                description: String::new(),
                capability_id: step_template.capability_id.clone(),
                input,
                depends_on: step_template.depends_on.clone(),
                estimated_duration_ms: step_template.estimated_duration_ms,
                timeout_ms: None,
                retry_policy: RetryPolicy::default(),
                continue_on_failure: false,
                metadata: StepMetadata::default(),
            };
            steps.push(step);
        }

        let plan = Plan::new(
            format!("{} - {}", template.name, intent.raw_input),
            format!("Auto-generated from template: {}", template.name),
            intent.id.clone(),
            steps,
        );

        Ok(plan)
    }

    /// LLM-based planning
    async fn llm_planning(&self, intent: &UserIntent) -> Result<Plan, AgentError> {
        // Build prompt for LLM
        let prompt = self.build_planning_prompt(intent);
        
        // Get plan from AI
        let response = self.ai_module.generate_plan(&prompt).await?;
        
        // Parse response into plan
        self.parse_llm_plan(&response, intent).await
    }

    /// Build planning prompt
    fn build_planning_prompt(&self, intent: &UserIntent) -> String {
        let available_caps: Vec<String> = self.capability_registry.list_all()
            .iter()
            .map(|c| c.definition.id.clone())
            .collect();

        format!(
            r#"You are a planner for JAMES, an autonomous agent system.
User intent: {}
Parsed action: {}
Parameters: {}
Available capabilities: {}

Create a JSON plan with steps. Each step must use an available capability.
Return ONLY valid JSON in this format:
{{
  "name": "Plan name",
  "description": "Plan description",
  "steps": [
    {{
      "name": "Step name",
      "capability_id": "capability.id",
      "input": {{}},
      "depends_on": [],
      "estimated_duration_ms": 5000
    }}
  ]
}}"#,
            intent.raw_input,
            intent.parsed_intent.action,
            serde_json::to_string(&intent.parsed_intent.parameters).unwrap_or_default(),
            available_caps.join(", ")
        )
    }

    /// Parse LLM response into plan
    async fn parse_llm_plan(&self, response: &str, intent: &UserIntent) -> Result<Plan, AgentError> {
        // Try to extract JSON from response
        let json_start = response.find('{').unwrap_or(0);
        let json_end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
        let json_str = &response[json_start..json_end];

        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| AgentError::PlanValidationFailed(format!("Failed to parse LLM plan: {}", e)))?;

        let name = parsed.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("LLM Generated Plan")
            .to_string();
        let description = parsed.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("Generated by LLM")
            .to_string();

        let steps_json = parsed.get("steps")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentError::PlanValidationFailed("No steps array in plan".to_string()))?;

        let mut steps = Vec::new();
        for step_json in steps_json {
            let capability_id = step_json.get("capability_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::PlanValidationFailed("Step missing capability_id".to_string()))?;

            // Verify capability exists
            if self.capability_registry.get(capability_id).is_none() {
                return Err(AgentError::PlanValidationFailed(
                    format!("Unknown capability: {}", capability_id)
                ));
            }

            let input = step_json.get("input").cloned().unwrap_or(serde_json::json!({}));
            let depends_on = step_json.get("depends_on")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let estimated_duration_ms = step_json.get("estimated_duration_ms")
                .and_then(|v| v.as_u64());

            let step = PlanStep {
                id: Uuid::now_v7().to_string(),
                name: step_json.get("name").and_then(|v| v.as_str()).unwrap_or("Unnamed").to_string(),
                description: String::new(),
                capability_id: capability_id.to_string(),
                input,
                depends_on,
                estimated_duration_ms,
                timeout_ms: None,
                retry_policy: RetryPolicy::default(),
                continue_on_failure: false,
                metadata: StepMetadata::default(),
            };
            steps.push(step);
        }

        Ok(Plan::new(name, description, intent.id.clone(), steps))
    }

    /// Substitute variables in JSON
    fn substitute_variables(&self, value: &mut serde_json::Value, variables: &HashMap<String, serde_json::Value>) {
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
                    self.substitute_variables(v, variables);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr.iter_mut() {
                    self.substitute_variables(v, variables);
                }
            }
            _ => {}
        }
    }
}

/// Simple heuristic planner (no LLM required)
pub struct HeuristicPlanner {
    config: AgentConfig,
    capability_registry: Arc<CapabilityRegistry>,
    event_bus: Arc<EventBus>,
}

impl HeuristicPlanner {
    pub fn new(config: AgentConfig, capability_registry: Arc<CapabilityRegistry>, event_bus: Arc<EventBus>) -> Self {
        Self { config, capability_registry, event_bus }
    }

    /// Create a simple linear plan for common intents
    pub async fn create_plan(&self, intent: UserIntent) -> Result<Plan, AgentError> {
        let action = intent.parsed_intent.action.clone();
        let params = intent.parsed_intent.parameters.clone();

        match action.as_str() {
            "search" | "research" | "find" => self.create_research_plan(intent, &params).await,
            "code" | "write" | "implement" => self.create_code_plan(intent, &params).await,
            "browse" | "navigate" | "open" => self.create_browser_plan(intent, &params).await,
            "analyze" => self.create_analysis_plan(intent, &params).await,
            _ => self.create_generic_plan(intent, &params).await,
        }
    }

    async fn create_research_plan(&self, intent: UserIntent, params: &HashMap<String, serde_json::Value>) -> Result<Plan, AgentError> {
        let query = params.get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let steps = vec![
            PlanStep::new(
                "Search web".to_string(),
                "web.search".to_string(),
                serde_json::json!({ "query": query, "max_results": 10 }),
            ),
            PlanStep::new(
                "Extract content".to_string(),
                "web.extract".to_string(),
                serde_json::json!({ "urls": "${step_0.results[*].url}" }),
            ).with_dependency("${step_0.id}"),
            PlanStep::new(
                "Store findings".to_string(),
                "memory.write".to_string(),
                serde_json::json!({ 
                    "content": "${step_1.extracted_content}", 
                    "tags": ["research", query] 
                }),
            ).with_dependency("${step_1.id}"),
        ];

        Ok(Plan::new(
            format!("Research: {}", query),
            format!("Research plan for: {}", query),
            intent.id,
            steps,
        ))
    }

    async fn create_code_plan(&self, intent: UserIntent, params: &HashMap<String, serde_json::Value>) -> Result<Plan, AgentError> {
        let task = params.get("task")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let steps = vec![
            PlanStep::new(
                "Analyze requirements".to_string(),
                "code.analyze".to_string(),
                serde_json::json!({ "task": task }),
            ),
            PlanStep::new(
                "Generate code".to_string(),
                "code.generate".to_string(),
                serde_json::json!({ "spec": "${step_0.analysis}" }),
            ).with_dependency("${step_0.id}"),
            PlanStep::new(
                "Run tests".to_string(),
                "code.test".to_string(),
                serde_json::json!({ "code": "${step_1.code}" }),
            ).with_dependency("${step_1.id}"),
        ];

        Ok(Plan::new(
            format!("Code: {}", task),
            format!("Code generation plan for: {}", task),
            intent.id,
            steps,
        ))
    }

    async fn create_browser_plan(&self, intent: UserIntent, params: &HashMap<String, serde_json::Value>) -> Result<Plan, AgentError> {
        let url = params.get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let steps = vec![
            PlanStep::new(
                "Navigate to URL".to_string(),
                "browser.navigate".to_string(),
                serde_json::json!({ "url": url }),
            ),
        ];

        Ok(Plan::new(
            format!("Browse: {}", url),
            format!("Browser navigation to {}", url),
            intent.id,
            steps,
        ))
    }

    async fn create_analysis_plan(&self, intent: UserIntent, params: &HashMap<String, serde_json::Value>) -> Result<Plan, AgentError> {
        let target = params.get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let steps = vec![
            PlanStep::new(
                "Read source".to_string(),
                "filesystem.read".to_string(),
                serde_json::json!({ "path": target }),
            ),
            PlanStep::new(
                "Analyze content".to_string(),
                "code.analyze".to_string(),
                serde_json::json!({ "content": "${step_0.content}" }),
            ).with_dependency("${step_0.id}"),
        ];

        Ok(Plan::new(
            format!("Analyze: {}", target),
            format!("Analysis plan for {}", target),
            intent.id,
            steps,
        ))
    }

    async fn create_generic_plan(&self, intent: UserIntent, _params: &HashMap<String, serde_json::Value>) -> Result<Plan, AgentError> {
        // Single step plan as fallback
        let steps = vec![
            PlanStep::new(
                "Execute intent".to_string(),
                "generic.execute".to_string(),
                serde_json::json!({ "intent": intent.raw_input }),
            ),
        ];

        Ok(Plan::new(
            format!("Execute: {}", intent.raw_input),
            format!("Generic execution of: {}", intent.raw_input),
            intent.id,
            steps,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_capabilities::{CapabilityCategory, CapabilityDefinition, ExecutionTarget, RiskLevel};

    async fn register_test_capability(registry: &CapabilityRegistry, id: &str) {
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

    #[tokio::test]
    async fn test_heuristic_planner_research() {
        let event_bus = Arc::new(EventBus::with_default_buffer());
        let capability_registry = Arc::new(CapabilityRegistry::new());

        // Register test capabilities
        for cap_id in ["web.search", "web.extract", "memory.write"] {
            register_test_capability(&capability_registry, cap_id).await;
        }

        let planner = HeuristicPlanner::new(
            AgentConfig::default(),
            capability_registry,
            event_bus,
        );

        let intent = UserIntent::new(
            "user1".to_string(),
            "search for rust async patterns".to_string(),
            ParsedIntent {
                action: "search".to_string(),
                target: Some("rust async patterns".to_string()),
                parameters: {
                    let mut m = HashMap::new();
                    m.insert("query".to_string(), serde_json::json!("rust async patterns"));
                    m
                },
                constraints: vec![],
                expected_output: None,
            },
        );

        let plan = planner.create_plan(intent).await.unwrap();
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].capability_id, "web.search");
        assert_eq!(plan.steps[1].capability_id, "web.extract");
        assert_eq!(plan.steps[2].capability_id, "memory.write");
    }
}