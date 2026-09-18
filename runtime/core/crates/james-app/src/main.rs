use james_app_api::{preview_bind_allowed, serve, AppState, DashboardSnapshot, IntentProvider, AuthConfig, EnvTokenStore, TokenStore};
use james_core::{init_tracing, CoreConfig, JamesCore, LogFields};
use james_agents::{model::*, planner::*, executor::*, Planner, PlanExecutor, UserIntent, PlanStep, Plan, RetryPolicy, StepMetadata, PlanExecutionResult};
use james_python_bridge::{BridgeConfig as PythonBridgeConfig, CapabilitySync, NatsBridge, PythonExecutor};
use anyhow::Result;
use std::sync::Arc;
use std::path::Path;
use std::time::Instant;
use tokio::signal;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashMap;

/// Bridges the UI orchestrator's latest intent into the REST surface.
struct IntentBridge {
    orchestrator: Arc<james_ui::UiOrchestrator>,
}

#[async_trait::async_trait]
impl IntentProvider for IntentBridge {
    async fn latest(&self) -> Result<serde_json::Value> {
        let intent = self.orchestrator.latest().await;
        serde_json::to_value(intent).map_err(|e| anyhow::anyhow!(e))
    }
}

/// Simple template-based planner for user intents.
struct TemplatePlanner {
    templates: Arc<RwLock<HashMap<String, PlanningTemplate>>>,
}

impl TemplatePlanner {
    fn new() -> Self {
        let mut templates = HashMap::new();
        
        // System info template - uses platform capabilities
        templates.insert("sys_info".to_string(), PlanningTemplate {
            name: "sys_info".to_string(),
            pattern: "system".to_string(),
            steps: vec![
                PlanStepTemplate {
                    name: "get_system_info".to_string(),
                    capability_id: "platform.system.info".to_string(),
                    input_template: serde_json::json!({}),
                    depends_on: vec![],
                    estimated_duration_ms: Some(500),
                }
            ],
            variables: HashMap::new(),
        });
        
        // File read template
        templates.insert("fs_read".to_string(), PlanningTemplate {
            name: "fs_read".to_string(),
            pattern: "read".to_string(),
            steps: vec![
                PlanStepTemplate {
                    name: "read_file".to_string(),
                    capability_id: "platform.fs.read".to_string(),
                    input_template: serde_json::json!({
                        "path": "${path}"
                    }),
                    depends_on: vec![],
                    estimated_duration_ms: Some(1000),
                }
            ],
            variables: HashMap::new(),
        });
        
        // File write template
        templates.insert("fs_write".to_string(), PlanningTemplate {
            name: "fs_write".to_string(),
            pattern: "write".to_string(),
            steps: vec![
                PlanStepTemplate {
                    name: "write_file".to_string(),
                    capability_id: "platform.fs.write".to_string(),
                    input_template: serde_json::json!({
                        "path": "${path}",
                        "content": "${content}"
                    }),
                    depends_on: vec![],
                    estimated_duration_ms: Some(1000),
                }
            ],
            variables: HashMap::new(),
        });
        
        // Process list template
        templates.insert("proc_list".to_string(), PlanningTemplate {
            name: "proc_list".to_string(),
            pattern: "process".to_string(),
            steps: vec![
                PlanStepTemplate {
                    name: "list_processes".to_string(),
                    capability_id: "platform.process.list".to_string(),
                    input_template: serde_json::json!({}),
                    depends_on: vec![],
                    estimated_duration_ms: Some(1000),
                }
            ],
            variables: HashMap::new(),
        });
        
        // Task execution template - uses the capability specified in intent
        templates.insert("task_exec".to_string(), PlanningTemplate {
            name: "task_exec".to_string(),
            pattern: "execute".to_string(),
            steps: vec![
                PlanStepTemplate {
                    name: "run_capability".to_string(),
                    capability_id: "${capability}".to_string(),
                    input_template: serde_json::json!({}),
                    depends_on: vec![],
                    estimated_duration_ms: Some(1000),
                }
            ],
            variables: HashMap::new(),
        });
        
        Self {
            templates: Arc::new(RwLock::new(templates)),
        }
    }
    
    async fn create_plan(&self, intent: UserIntent) -> Result<Plan> {
        tracing::info!("TemplatePlanner: creating plan for raw_input='{}'", intent.raw_input);
        let templates = self.templates.read().await;
        
        // Try to match a template
        for template in templates.values() {
            tracing::debug!("TemplatePlanner: checking template '{}' with pattern '{}'", template.name, template.pattern);
            if intent.raw_input.to_lowercase().contains(&template.pattern.to_lowercase()) {
                tracing::info!("TemplatePlanner: matched template '{}' for input '{}'", template.name, intent.raw_input);
                return self.instantiate_template(template, &intent).await;
            }
        }
        
        // Default: single step with ai.inference
        tracing::warn!("TemplatePlanner: no template matched for '{}', using default ai.inference", intent.raw_input);
        let step = PlanStep::new(
            "ai_inference".to_string(),
            "ai.inference".to_string(),
            serde_json::json!({
                "prompt": intent.raw_input,
                "temperature": 0.7,
                "max_tokens": 2048
            })
        );
        
        Ok(Plan::new(
            "default_plan".to_string(),
            format!("Default plan for: {}", intent.raw_input),
            intent.id,
            vec![step],
        ))
    }
    
    async fn instantiate_template(&self, template: &PlanningTemplate, intent: &UserIntent) -> Result<Plan> {
        tracing::info!("TemplatePlanner: instantiating template '{}'", template.name);
        let mut steps = Vec::new();
        let mut variables = HashMap::new();
        
        // Extract variables from intent
        for (key, value) in &intent.parsed_intent.parameters {
            variables.insert(key.clone(), value.clone());
        }
        variables.insert("query".to_string(), serde_json::Value::String(intent.raw_input.clone()));
        variables.insert("user_id".to_string(), serde_json::Value::String(intent.user_id.clone()));
        
        for step_template in &template.steps {
            let mut input = step_template.input_template.clone();
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
            tracing::info!("TemplatePlanner: created step '{}' with capability_id='{}', input={}", step.name, step.capability_id, step.input);
            steps.push(step);
        }
        
        let plan = Plan::new(
            format!("{} - {}", template.name, intent.raw_input),
            format!("Generated from template: {}", template.name),
            intent.id.clone(),
            steps,
        );
        
        tracing::info!("TemplatePlanner: created plan '{}' with {} steps", plan.name, plan.steps.len());
        Ok(plan)
    }
    
    fn substitute_variables(&self, value: &mut serde_json::Value, variables: &HashMap<String, serde_json::Value>) {
        match value {
            serde_json::Value::String(s) => {
                for (key, val) in variables {
                    let placeholder = format!("${{{}}}", key);
                    if s.contains(&placeholder) {
                        *s = s.replace(&placeholder, &val.to_string());
                    }
                }
            }
            serde_json::Value::Object(obj) => {
                for (_, v) in obj.iter_mut() {
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

/// Agent service that manages planning and execution
struct AgentService {
    planner: Arc<TemplatePlanner>,
    executor: Arc<PlanExecutor>,
}

impl AgentService {
    fn new(broker: Arc<james_capability_broker::CapabilityBroker>) -> Self {
        let planner = Arc::new(TemplatePlanner::new());
        let executor = Arc::new(PlanExecutor::new(broker));
        
        Self { planner, executor }
    }

    fn register_python_executor(&self, capability_id: impl Into<String>, executor: Arc<dyn james_capability_broker::CapabilityExecutor>) {
        self.executor.register_executor(capability_id, executor);
    }
    
    async fn execute_intent(&self, intent: UserIntent) -> Result<PlanExecutionResult> {
        tracing::info!("AgentService: execute_intent called for user_id={}, raw_input={}", intent.user_id, intent.raw_input);
        tracing::debug!("AgentService: TemplatePlanner creating plan for: {}", intent.raw_input);
        let plan = self.planner.create_plan(intent).await
            .map_err(|e| anyhow::anyhow!("Failed to create plan: {}", e))?;
        info!("AgentService: Created plan '{}' with {} steps", plan.name, plan.steps.len());
        for (i, step) in plan.steps.iter().enumerate() {
            tracing::debug!("AgentService: Step {}: name={}, capability_id={}, input={}", i, step.name, step.capability_id, step.input);
        }
        
        // Execute the plan
        tracing::info!("AgentService: PlanExecutor executing plan: {}", plan.name);
        let result = self.executor.execute(&plan, "user").await
            .map_err(|e| anyhow::anyhow!("Plan execution failed: {}", e))?;
        tracing::info!("AgentService: Plan executed successfully, success={}, plan_id={}, total_duration_ms={}", result.success, result.plan_id, result.total_duration_ms);
        for step_result in &result.step_results {
            tracing::debug!("AgentService: StepResult: step_id={}, success={}, error={:?}", step_result.step_id, step_result.success, step_result.error);
        }
        Ok(result)
    }
}

#[async_trait::async_trait]
impl james_app_api::AgentHandler for AgentService {
    async fn execute_intent(&self, intent: UserIntent) -> anyhow::Result<PlanExecutionResult> {
        self.execute_intent(intent).await
    }
}

fn void_static_dir() -> String {
    ["interfaces/void", "../interfaces/void"]
        .iter()
        .find(|path| Path::new(*path).join("index.html").exists())
        .unwrap_or(&"interfaces/void")
        .to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let log = LogFields::new("james-app");
    info!("{} Starting JAMES App v{}", log.prefix(), env!("CARGO_PKG_VERSION"));

    let mut config = CoreConfig::load().unwrap_or_else(|e| {
        error!("{} Failed to load config: {}, using defaults", log.prefix(), e);
        CoreConfig::default()
    });

    if let Err(errors) = config.validate() {
        error!(
            "{} Invalid configuration ({} problem(s)), using defaults: {}",
            log.prefix(),
            errors.len(),
            errors.join("; ")
        );
        config = CoreConfig::default();
    }

    if let Err(e) = config.save() {
        error!("{} Failed to save config: {}", log.prefix(), e);
    }

    // Single construction path: JamesCore::new wires all components.
    // (No manual field assignment, no duplicate initialization.)
    let mut core = JamesCore::new(config).await?;
    core.start().await?;

    // Capture shared handles before spawning dashboard refresh (JamesCore is
    // not Clone).
    let event_bus = core.event_bus();
    let capability_registry = core.capability_registry();
    let started_at = Instant::now();

    // Phase 2: platform runtime (Windows adapter / memory double per env).
    // Registers platform.* capabilities, wires the broker, publishes telemetry.
    let platform_ctx = Arc::new(
        james_platform_services::bootstrap::bootstrap_with_registry(
            Some(event_bus.clone()),
            capability_registry.clone(),
        ).await?,
    );
    let _telemetry =
        james_platform_services::bootstrap::spawn_telemetry(platform_ctx.clone(), event_bus.clone());
    let governed_capabilities = platform_ctx.capabilities.clone();
    info!(
        "{} platform context ready ({} platform capabilities governed)",
        log.prefix(),
        governed_capabilities.count()
    );

    // Phase 3: UI orchestrator. Projects real bus state into declarative
    // `ui.intent` snapshots for the Void renderer.
    let orchestrator = Arc::new(james_ui::UiOrchestrator::new(event_bus.clone()));
    let _orchestrator_handle = orchestrator.spawn();
    info!("{} UI orchestrator started (ui.intent stream)", log.prefix());

    // Phase 0.2: Authentication setup
    let auth_config = AuthConfig::default();
    let token_store: Option<Arc<dyn TokenStore>> = if auth_config.enabled {
        if auth_config.backend == "os" {
            #[cfg(windows)]
            {
                Some(Arc::new(james_app_api::WindowsTokenStore::new("JAMES_API_TOKEN")))
            }
            #[cfg(not(windows))]
            {
                warn!("{} OS credential store requested but not on Windows, falling back to env", log.prefix());
                Some(Arc::new(EnvTokenStore::new(&auth_config.token_env)))
            }
        } else {
            Some(Arc::new(EnvTokenStore::new(&auth_config.token_env)))
        }
    } else {
        None
    };
    let preview_unauthenticated = token_store.is_none() || auth_config.dev_mode;
    if preview_unauthenticated {
        warn!(
            "{} Running in preview mode (auth disabled or dev mode). Loopback only.",
            log.prefix()
        );
    }

    let initial_status = core.status().await;

    // General executor registry (Block C): one shared registry per provider,
    // seeded into a resolver the PlanExecutor consults before the broker.
    let executor_registry = Arc::new(james_agents::ExecutorRegistry::with_provider("james-platform"));
    use james_platform_services::ids;
    for cap_id in ids::ALL {
        executor_registry.register(*cap_id, platform_ctx.executor.clone());
    }
    let shared_resolver = Arc::new(james_agents::CapabilityResolver::new());
    executor_registry.seed(&shared_resolver);

    // Python capability bridge: Python tools share the same registry and broker
    // as platform capabilities. This makes PlanExecutor -> broker -> NATS ->
    // Python ToolRegistry a real execution path rather than a parallel service.
    let python_bridge_config = PythonBridgeConfig::load().unwrap_or_default();
    let mut python_nats = NatsBridge::new(python_bridge_config.clone());
    python_nats.set_event_bus(event_bus.clone());
    python_nats.connect().await?;
    let python_nats = Arc::new(python_nats);
    let python_executor: Arc<dyn james_capability_broker::CapabilityExecutor> =
        Arc::new(PythonExecutor::new(python_nats.clone()).with_caller("agent-runtime".to_string()));

    let python_sync = CapabilitySync::new(
        python_bridge_config,
        capability_registry.clone(),
        Some(event_bus.clone()),
    );
    let python_caps = python_nats.list_python_capabilities().await;
    python_sync.sync_all(&python_caps).await?;
    info!(
        "{} Python capability bridge ready: {} capabilities discovered",
        log.prefix(),
        python_caps.len()
    );

    // Agent service for intent execution
    let agent_service = Arc::new(AgentService::new(platform_ctx.broker.clone()));
    for cap in &python_caps {
        agent_service.register_python_executor(cap.id.clone(), python_executor.clone());
    }

    // Keep the shared registry and resolver synchronized when the Python
    // sidecar starts after the Rust app or reconnects to NATS.
    let sync_service = agent_service.clone();
    let sync_nats = python_nats.clone();
    let sync_registry = capability_registry.clone();
    let sync_bus = event_bus.clone();
    let sync_executor = python_executor.clone();
    let sync_broker = platform_ctx.broker.clone();
    let sync_config = PythonBridgeConfig::load().unwrap_or_default();
    let _python_sync_task = tokio::spawn(async move {
        let sync = CapabilitySync::new(sync_config, sync_registry, Some(sync_bus));
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let _expired_confirmations = sync_broker.cleanup_expired_confirmations().await;
            let caps = sync_nats.list_python_capabilities().await;
            if let Err(error) = sync.sync_all(&caps).await {
                tracing::warn!("Python capability sync failed: {}", error);
                continue;
            }
            for cap in caps {
                sync_service.register_python_executor(cap.id, sync_executor.clone());
            }
        }
    });
    agent_service.executor.attach_resolver(shared_resolver.clone());
    info!(
        "{} Agent service ready: {} platform capabilities resolvable",
        log.prefix(),
        ids::ALL.len()
    );
    info!("{} Agent service started with {} platform capabilities", log.prefix(), ids::ALL.len());
    
    let state = AppState {
        bus: event_bus.clone(),
        status: Arc::new(RwLock::new(initial_status.clone())),
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at: chrono::Utc::now(),
        static_dir: void_static_dir(),
        dashboard: Arc::new(RwLock::new(DashboardSnapshot {
            core_status: initial_status.as_str().to_string(),
            ..Default::default()
        })),
        preview_unauthenticated,
        chat_handler: None,
        data: None,
        broker: Some(platform_ctx.broker.clone()),
        executor: Some(platform_ctx.executor.clone()),
        agent_handler: Some(agent_service),
        intent: Some(Arc::new(IntentBridge {
            orchestrator: orchestrator.clone(),
        })),
        token_store,
    };

    let bind = "127.0.0.1";
    if !preview_bind_allowed(bind) {
        anyhow::bail!("refusing to bind {bind}: no auth yet, loopback only (A7b closes this)");
    }
    if preview_unauthenticated {
        warn!(
            "{} UNAUTHENTICATED local preview (dev mode or no auth config). Loopback only.",
            log.prefix()
        );
    }

    let dashboard = state.dashboard.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let mut snap = dashboard.write().await;
            snap.capabilities = capability_registry.count();
            snap.events_dropped = event_bus.dropped_count();
            snap.uptime_secs = started_at.elapsed().as_secs();
        }
    });

    let void_listener = tokio::net::TcpListener::bind((bind, 38241)).await?;
    info!(
        "{} Void transport listening on http://{}:{} (preview, unauthenticated in A7b)",
        log.prefix(), bind, 38241
    );
    let void_handle = tokio::spawn(serve(void_listener, state.clone()));

    let dashboard_listener = tokio::net::TcpListener::bind((bind, 38242)).await?;
    info!(
        "{} Dashboard transport listening on http://{}:{}",
        log.prefix(), bind, 38242
    );
    let dashboard_handle = tokio::spawn(serve(dashboard_listener, state));

    info!("{} JAMES Core running. Press Ctrl+C to stop.", log.prefix());
    signal::ctrl_c().await?;

    info!("{} Shutdown signal received", log.prefix());
    void_handle.abort();
    dashboard_handle.abort();
    _python_sync_task.abort();
    python_nats.shutdown().await;
    core.stop().await?;

    info!("{} JAMES Core stopped", log.prefix());
    Ok(())
}