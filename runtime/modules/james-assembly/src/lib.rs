//! James-Assembly - Wire all first-party modules into a running JAMES system.
//!
//! Constructs the shared infrastructure (EventBus, CapabilityRegistry), then
//! builds and starts every first-party module in dependency order. Exposes
//! handles so the app layer can drive modules (chat, tasks, scheduler, etc.).

use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use james_agents::{
    AgentConfig, AiPlannerProvider, CapabilityResolver, ExecutorCandidate, HeuristicPlanner,
    LlmPlanner, PlanExecutionResult, PlanExecutor, ResolutionContext, UserIntent,
};
use james_capability_broker::{CapabilityBroker, CapabilityExecutor, CapabilityRequestV2, RequestedEffect};
use james_capabilities::CapabilityRegistry;
use james_core::JamesCore;
use james_events::{EventBus, EventEnvelope};
use serde_json::json;
use tokio::sync::RwLock;
use tracing::info;

use james_ai::{AiModule, AiProvider, ChatMessage, InferenceRequest, MessageRole};
use james_browser::BrowserModule;
use james_chat::ChatModule;
use james_dashboard::DashboardModule;
use james_memory::{MemoryEntry, MemoryModule, MemoryQuery};
use james_modelrouter::{ModelRouterModule, RouterConfig};
use james_models::ModelsModule;
use james_scheduler::SchedulerConfig as SchedulerModuleConfig;
use james_scheduler::SchedulerModule;
use james_stt::SttModule;
use james_tasks::TasksConfig as TasksModuleConfig;
use james_tasks::TasksModule;
use james_textinput::TextInputModule;
use james_textinput::TextInputConfig;
use james_textoutput::TextOutputModule;
use james_textoutput::TextOutputConfig;
use james_tts::TtsModule;
use james_void::VoidModule;
use james_voice::VoiceModule;
use james_webresearch::WebResearchModule;
use james_selfmade::{DevelopmentAgent, SelfMadeModule};

/// Fully assembled JAMES system: core + all first-party modules.
pub struct JamesAssembly {
    core: JamesCore,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    broker: Arc<CapabilityBroker>,
    /// Resolves capability ids to executor candidates (Block C). Executors
    /// are registered here once at startup and looked up at execution time.
    resolver: Arc<CapabilityResolver>,

    text_input: Arc<TextInputModule>,
    text_output: Arc<TextOutputModule>,
    chat: Arc<ChatModule>,
    models: Arc<ModelsModule>,
    model_router: Arc<ModelRouterModule>,
    ai: Arc<AiModule>,
    memory: Arc<MemoryModule>,
    tasks: Arc<TasksModule>,
    scheduler: Arc<SchedulerModule>,
    stt: Arc<SttModule>,
    tts: Arc<TtsModule>,
    voice: Arc<VoiceModule>,
    browser: Arc<BrowserModule>,
    web_research: Arc<WebResearchModule>,
    selfmade: Arc<SelfMadeModule>,
    void: Arc<VoidModule>,
    dashboard: Arc<DashboardModule>,
    /// Registry of live `Agent` instances (typed plan lifecycle, state
    /// machine, events, metrics) constructed from the same broker/resolver.
    agent_registry: Arc<james_agents::AgentRegistry>,
    /// Bounded ring of recently observed events, used by the UI's dashboard
    /// and the Void's semantic brain projection. Collection is real: events
    /// are drained from the bus, never synthesized.
    event_history: Arc<RwLock<VecDeque<EventEnvelope>>>,
    event_drain: Option<tokio::task::JoinHandle<()>>,
}

/// Configuration for the assembly. Uses module defaults when `None`.
pub struct AssemblyOptions {
    pub memory_path: Option<String>,
    pub tasks_path: Option<String>,
    pub scheduler_path: Option<String>,
    pub enable_web: bool,
    pub enable_voice: bool,
    pub enable_dashboard: bool,
    pub ollama_endpoint: Option<String>,
}

struct MemoryCapabilityExecutor {
    memory: Arc<MemoryModule>,
}

#[async_trait]
impl CapabilityExecutor for MemoryCapabilityExecutor {
    async fn execute(
        &self,
        capability_id: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        use james_memory::MemoryType;

        let memory_types = match capability_id {
            "memory.read" => None,
            "memory.working" => Some(vec![MemoryType::Working]),
            "memory.episodic" => Some(vec![MemoryType::Episodic]),
            "memory.semantic" => Some(vec![MemoryType::Semantic]),
            "memory.procedural" => Some(vec![MemoryType::Procedural]),
            "memory.relationship" => Some(vec![MemoryType::Relationship]),
            other => anyhow::bail!("unsupported memory capability: {other}"),
        };
        let limit = input
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(10)
            .min(100) as usize;
        let query_text = input
            .get("query")
            .and_then(serde_json::Value::as_str)
            .map(String::from);
        let entries = self.memory.query(MemoryQuery {
            memory_types,
            query_text,
            tags: None,
            session_id: None,
            agent_id: None,
            limit,
            min_importance: None,
            since: None,
        }).await?;
        Ok(serde_json::json!({"memories": entries}))
    }
}


struct SelfMadeCapabilityExecutor {
    selfmade: Arc<SelfMadeModule>,
    ai: Arc<AiModule>,
}

struct AssemblyDevelopmentAgent {
    ai: Arc<AiModule>,
}

impl DevelopmentAgent for AssemblyDevelopmentAgent {
    fn propose(&self, context: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<james_selfmade::EvolutionProposal>> + Send + '_>> {
        let context = context.to_owned();
        Box::pin(async move {
        let prompt = format!(r#"You are JAMES's bounded software-development agent.
Return ONLY valid JSON matching this schema:
{{
  "id": "uuid",
  "mission_id": "string",
  "objective": "string",
  "strategy": "string",
  "evidence": {{
    "capability_count": 0,
    "repository_dirty": false,
    "git_head": null,
    "missing_capabilities": []
  }},
  "source_provenance": ["..."],
  "risk_class": "low|medium|high",
  "promotion_allowed": false,
  "created_at": "RFC3339 timestamp"
}}
Do not claim tests, files, patches, research or sources that are not present in the supplied context.
Never set promotion_allowed to true.
Context:
{context}"#);
        let response = self.ai.infer(InferenceRequest {
            model_id: None,
            messages: vec![
                ChatMessage { role: MessageRole::System, content: "You produce conservative machine-readable development proposals.".into(), name: None, tool_calls: None, tool_call_id: None },
                ChatMessage { role: MessageRole::User, content: prompt, name: None, tool_calls: None, tool_call_id: None },
            ],
            temperature: Some(0.1),
            max_tokens: Some(2048),
            stream: false,
            response_format: None,
            tools: None,
        }).await?;
        let raw = response.choices.first()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("development agent returned no proposal"))?;
        let proposal: james_selfmade::EvolutionProposal = serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("development agent returned invalid proposal JSON: {e}"))?;
        Ok(proposal)
        })
    }

    fn generate_patch(&self, context: &str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
        let context = context.to_owned();
        Box::pin(async move {
        let root = std::env::var_os("JAMES_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or(std::env::current_dir()?);
        let source_path = root.join("runtime/modules/james-selfmade/src/lib.rs");
        let source = tokio::fs::read_to_string(&source_path).await?;
        let prompt = [
            "You are JAMES's bounded coding agent.",
            "Produce ONLY a valid unified git diff.",
            "Modify only files necessary for the stated objective. Prefer small, testable changes.",
            "Do not modify security roots, authentication roots, audit integrity, kill controls, or promotion policy.",
            "The diff is applied only inside an isolated git worktree and automatically tested.",
            "Evolution context:", context, "Current selfmade source:", &source,
        ].join("\n");
        let response = self.ai.infer(InferenceRequest {
            model_id: None,
            messages: vec![
                ChatMessage { role: MessageRole::System, content: "Conservative Rust maintainer. Output only unified diff.".into(), name: None, tool_calls: None, tool_call_id: None },
                ChatMessage { role: MessageRole::User, content: prompt, name: None, tool_calls: None, tool_call_id: None },
            ],
            temperature: Some(0.1),
            max_tokens: Some(4096),
            stream: false,
            response_format: None,
            tools: None,
        }).await?;
        let raw = response.choices.first()
            .map(|choice| choice.message.content.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("development agent returned no patch"))?;
        if !raw.starts_with("diff --git ") {
            anyhow::bail!("development agent returned content that is not a unified git diff");
        }
        Ok(raw)
        })
    }
}



#[async_trait]
impl CapabilityExecutor for SelfMadeCapabilityExecutor {
    async fn execute(&self, capability_id: &str, _input: serde_json::Value) -> Result<serde_json::Value> {
        match capability_id {
            "selfmade.observe" => self.selfmade.observe_self().await,
            "selfmade.propose" => {
                let objective = _input
                    .get("objective")
                    .and_then(|value| value.as_str())
                    .unwrap_or("identify the highest-value safe improvement for JAMES");
                let cycle = self.selfmade.run_evolution_cycle(objective).await?;
                let agent = AssemblyDevelopmentAgent { ai: self.ai.clone() };
                let proposal = self.selfmade.create_agent_proposal(&cycle, &agent).await?;
                serde_json::to_value(proposal).map_err(Into::into)
            },
            "selfmade.develop" => {
                let objective = _input
                    .get("objective")
                    .and_then(|value| value.as_str())
                    .unwrap_or("make the highest-value safe improvement for JAMES");
                let cycle = self.selfmade.run_evolution_cycle(objective).await?;
                let agent = AssemblyDevelopmentAgent { ai: self.ai.clone() };
                let report = self.selfmade.develop_cycle(&cycle, &agent).await?;
                serde_json::to_value(report).map_err(Into::into)
            }
            "selfmade.assess" => {
                let objective = _input
                    .get("objective")
                    .and_then(|value| value.as_str())
                    .unwrap_or("identify the highest-value safe improvement for JAMES");
                serde_json::to_value(self.selfmade.assess_evolution(objective).await?)
                    .map_err(Into::into)
            }
            "selfmade.verify" => {
                let workspace = self.selfmade.ensure_workspace().await?;
                let report = self.selfmade.verify_workspace_public(&workspace).await?;
                serde_json::to_value(report).map_err(Into::into)
            }
            "selfmade.evaluate" => {
                serde_json::to_value(self.selfmade.evaluate_workspace().await?)
                    .map_err(Into::into)
            },
            "selfmade.rollback" => {
                let result = self.selfmade.rollback_workspace().await?;
                Ok(result)
            }
            _ => anyhow::bail!("selfmade capability is not executable through this adapter: {capability_id}"),
        }
    }
}

struct VoidCapabilityExecutor {
    void: Arc<VoidModule>,
}

#[async_trait]
impl CapabilityExecutor for VoidCapabilityExecutor {
    async fn execute(&self, capability_id: &str, input: serde_json::Value) -> Result<serde_json::Value> {
        if capability_id != "void.chat" { anyhow::bail!("unsupported Void capability: {capability_id}"); }
        let message = input.get("message").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("void.chat requires message"))?;
        let response = self.void.execute_chat(message).await?;
        Ok(serde_json::to_value(response)?)
    }
}

/// `AiPlannerProvider` adapter backed by the real assembly `AiModule`.
/// Lets `LlmPlanner` generate plans from an actual model response.
struct AssemblyAiPlannerProvider {
    ai: Arc<AiModule>,
}

#[async_trait]
impl AiPlannerProvider for AssemblyAiPlannerProvider {
    async fn generate_plan(&self, prompt: &str) -> std::result::Result<String, james_agents::AgentError> {
        let response = self.ai.infer(InferenceRequest {
            model_id: None,
            messages: vec![
                ChatMessage { role: MessageRole::System, content: "You are a strict JSON plan generator.".to_string(), name: None, tool_calls: None, tool_call_id: None },
                ChatMessage { role: MessageRole::User, content: prompt.to_string(), name: None, tool_calls: None, tool_call_id: None },
            ],
            temperature: Some(0.2),
            max_tokens: Some(2048),
            stream: false,
            response_format: None,
            tools: None,
        }).await
            .map_err(|e| james_agents::AgentError::PlanValidationFailed(e.to_string()))?;
        Ok(response.choices.first()
            .and_then(|c| Some(c.message.content.clone()))
            .unwrap_or_else(|| String::from("{}")))
    }

    async fn refine_plan(&self, plan: &james_agents::Plan, feedback: &james_agents::PlanFeedback) -> std::result::Result<james_agents::Plan, james_agents::AgentError> {
        Ok(plan.clone())
    }
}

impl Default for AssemblyOptions {
    fn default() -> Self {
        Self {
            memory_path: None,
            tasks_path: None,
            scheduler_path: None,
            enable_web: true,
            enable_voice: true,
            enable_dashboard: true,
            ollama_endpoint: None,
        }
    }
}

impl JamesAssembly {
    /// Build core + all modules from a `JamesCore` (already constructed by the app).
    pub async fn new(core: JamesCore, options: AssemblyOptions) -> Result<Self> {
        let event_bus = core.event_bus();
        let capability_registry = core.capability_registry();
        let broker = Arc::new(
            CapabilityBroker::new(capability_registry.clone())
                .with_event_bus(event_bus.clone()),
        );

        // ---- Base interface modules ----
        let text_input = Arc::new(TextInputModule::new(
            TextInputConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        ).0);

        let text_output = Arc::new(TextOutputModule::new(
            TextOutputConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        ));

        // ---- Chat (composes input + output) ----
        let (chat_module, _chat_rx) = ChatModule::new(
            james_chat::ChatConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        );
        let chat = Arc::new(chat_module);

        // ---- Canonical model registry + routing ----
        // ModelsModule owns model metadata/discovery. ModelRouter only resolves
        // requests against that registry; it does not create or lifecycle-own it.
        let models = Arc::new(ModelsModule::new(
            event_bus.clone(),
            capability_registry.clone(),
        ));
        let model_router = Arc::new(ModelRouterModule::new(
            RouterConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
            models.clone(),
        ));

        // ---- AI (with model router attached) ----
        let ai = Arc::new(AiModule::new(
            event_bus.clone(),
            capability_registry.clone(),
        ).with_router(model_router.clone()));

        // ---- Local AI provider (Ollama) ----
        let local_provider: Arc<dyn AiProvider> = {
            let p = james_ai_local::LocalAiProvider::new(options.ollama_endpoint.clone());
            Arc::new(p)
        };
        ai.register_provider(local_provider).await;

        // Discover models once at startup so routing works without a prior
        // dashboard read. Honest: healthcheck of the local endpoint fails
        // silently and routing reports "No models available" until a model
        // is reachable (e.g. Ollama serving a pulled model).
        if let Ok(models) = ai.list_models().await {
            let _ = model_router.sync_models(models).await;
        }

        // ---- Memory ----
        let memory = Arc::new(MemoryModule::new(
            james_memory::MemoryConfig {
                database_path: options.memory_path
                    .unwrap_or_else(|| ".james/memory.json".to_string()),
                ..Default::default()
            },
            event_bus.clone(),
            capability_registry.clone(),
        ));

        // ---- Canonical task manager + module facade ----
        let core_tasks = Arc::new(
            james_tasks_core::TaskManager::new(Some(event_bus.clone()))
                .with_capability_registry(capability_registry.clone())
                .with_max_concurrent(TasksModuleConfig::default().max_concurrent),
        );
        // One TaskManager instance is shared by tasks, scheduler, and agents.
        let tasks = Arc::new(TasksModule::from_manager(
            TasksModuleConfig {
                database_path: options.tasks_path
                    .unwrap_or_else(|| ".james/tasks.json".to_string()),
                ..Default::default()
            },
            event_bus.clone(),
            capability_registry.clone(),
            memory.clone(),
            core_tasks.clone(),
        ));

        // ---- Scheduler (delegates to the same canonical task manager) ----
        let scheduler = Arc::new(SchedulerModule::new(
            SchedulerModuleConfig {
                database_path: options.scheduler_path
                    .unwrap_or_else(|| ".james/scheduler.json".to_string()),
                ..Default::default()
            },
            event_bus.clone(),
            capability_registry.clone(),
            core_tasks.clone(),
        ));

        // ---- STT / TTS ----
        let stt = Arc::new(SttModule::new(
            james_stt::SttConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        ));
        let tts = Arc::new(TtsModule::new(
            james_tts::TtsConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        ));

        // ---- Voice (composes STT + TTS) ----
        let voice = Arc::new(VoiceModule::new(
            james_voice::VoiceConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
            stt.clone(),
            tts.clone(),
        ));

        // ---- Browser + WebResearch ----
        let browser = Arc::new(BrowserModule::new(
            james_browser::BrowserConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        ));
        let web_research = Arc::new(WebResearchModule::new(
            james_webresearch::WebResearchConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
        ));

        // ---- SelfMade autonomous development ----
        // The module works in an isolated git worktree and never mutates the
        // running checkout during proposal/verification.
        let selfmade_root = std::env::var_os("JAMES_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or(std::env::current_dir()?);
        let selfmade = Arc::new(SelfMadeModule::new(
            selfmade_root,
            event_bus.clone(),
            capability_registry.clone(),
        ));

        // ---- Void (web chat shell) ----
        let void = Arc::new(VoidModule::new(
            james_void::VoidConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
            chat.clone(),
            ai.clone(),
            memory.clone(),
            tasks.clone(),
            web_research.clone(),
        ));

        // ---- Dashboard ----
        let dashboard = Arc::new(DashboardModule::new(
            james_dashboard::DashboardConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
            chat.clone(),
            ai.clone(),
            memory.clone(),
            tasks.clone(),
            web_research.clone(),
            voice.clone(),
            scheduler.clone(),
        ));

        // ---- Agent runtime (full lifecycle: plan, state machine, metrics) ----
        // Shares the assembly broker and memory; executor candidates come from
        // the shared resolver. The core task manager tracks plan steps.
        let identity_registry = Arc::new(james_identity::IdentityRegistry::new());
        let executor_registry = Arc::new(james_agents::ExecutorRegistry::with_provider("james-assembly"));
        let resolver = Arc::new(CapabilityResolver::new());
        let mut agent_factory = james_agents::AgentFactory::new(
            event_bus.clone(),
            capability_registry.clone(),
            broker.clone(),
            identity_registry,
            memory.clone(),
            core_tasks,
            executor_registry,
        );
        // Built-in typed agents (researcher/coder/browser) must use the same
        // live resolver as direct intent execution; otherwise their factory
        // path would only see the empty legacy executor registry.
        agent_factory.attach_resolver(resolver.clone());
        let agent_registry = Arc::new(james_agents::AgentRegistry::new(
            Arc::new(agent_factory),
        ));

        Ok(Self {
            core,
            event_bus,
            capability_registry,
            broker,
            resolver,
            text_input,
            text_output,
            chat,
            models,
            model_router,
            ai,
            memory,
            tasks,
            scheduler,
            stt,
            tts,
            voice,
            browser,
            web_research,
            selfmade,
            void,
            dashboard,
            agent_registry,
            event_history: Arc::new(RwLock::new(VecDeque::new())),
            event_drain: None,
        })
    }

    /// Register every first-party capability executor with the assembly
    /// resolver (Block C: `Requirement → Candidates → Selection`). The
    /// broker remains the single enforcement point when a candidate runs.
    ///
    /// Only capabilities with a real executor are registered. Capabilities
    /// whose modules provide definitions but no implementation yet (browser
    /// screenshot/extract, voice/device, etc.) stay unregistered so plans
    /// resolve honestly to "no candidate".
    pub fn register_resolver_candidates(&self) {
        let selfmade_executor: Arc<dyn CapabilityExecutor> =
            Arc::new(SelfMadeCapabilityExecutor { selfmade: self.selfmade.clone(), ai: self.ai.clone() });
        self.resolver.register(ExecutorCandidate {
            capability_id: "selfmade.observe".to_string(),
            provider: "james-selfmade".to_string(),
            priority: 10,
            available: true,
            capabilities: vec!["selfmade.observe".to_string()],
            health: None,
            executor: selfmade_executor.clone(),
        });
        self.resolver.register(ExecutorCandidate {
            capability_id: "selfmade.propose".to_string(),
            provider: "james-selfmade".to_string(),
            priority: 10,
            available: true,
            capabilities: vec!["selfmade.propose".to_string()],
            health: None,
            executor: selfmade_executor.clone(),
        });
        self.resolver.register(ExecutorCandidate {
            capability_id: "selfmade.develop".to_string(),
            provider: "james-selfmade".to_string(),
            priority: 10,
            available: true,
            capabilities: vec!["selfmade.develop".to_string()],
            health: None,
            executor: selfmade_executor.clone(),
        });

        self.resolver.register(ExecutorCandidate {
            capability_id: "selfmade.assess".to_string(),
            provider: "james-selfmade".to_string(),
            priority: 10,
            available: true,
            capabilities: vec!["selfmade.assess".to_string()],
            health: None,
            executor: selfmade_executor.clone(),
        });
        self.resolver.register(ExecutorCandidate {
            capability_id: "selfmade.verify".to_string(),
            provider: "james-selfmade".to_string(),
            priority: 10,
            available: true,
            capabilities: vec!["selfmade.verify".to_string()],
            health: None,
            executor: selfmade_executor.clone(),
        });
        self.resolver.register(ExecutorCandidate {
            capability_id: "selfmade.evaluate".to_string(),
            provider: "james-selfmade".to_string(),
            priority: 10,
            available: true,
            capabilities: vec!["selfmade.evaluate".to_string()],
            health: None,
            executor: selfmade_executor.clone(),
        });
        self.resolver.register(ExecutorCandidate {
            capability_id: "selfmade.rollback".to_string(),
            provider: "james-selfmade".to_string(),
            priority: 10,
            available: true,
            capabilities: vec!["selfmade.rollback".to_string()],
            health: None,
            executor: selfmade_executor,
        });

        let void_executor: Arc<dyn CapabilityExecutor> = Arc::new(VoidCapabilityExecutor { void: self.void.clone() });
        self.resolver.register(ExecutorCandidate {
            capability_id: "void.chat".to_string(), provider: "james-void".to_string(), priority: 10,
            available: true, capabilities: vec!["void.chat".to_string()], health: None, executor: void_executor,
        });
        let memory_executor: Arc<dyn CapabilityExecutor> =
            Arc::new(MemoryCapabilityExecutor {
                memory: self.memory.clone(),
            });
        let memory_capabilities = [
            "memory.read",
            "memory.working",
            "memory.episodic",
            "memory.semantic",
            "memory.procedural",
            "memory.relationship",
        ];
        for capability_id in memory_capabilities {
            self.resolver.register(ExecutorCandidate {
                capability_id: capability_id.to_string(),
                provider: "james-memory".to_string(),
                priority: 10,
                available: true,
                capabilities: vec![capability_id.to_string()],
                health: None,
                executor: memory_executor.clone(),
            });
        }
    }

    /// Start core, then register module capabilities, then start modules in dependency order.
    pub async fn start(&mut self) -> Result<()> {
        self.core.start().await?;

        // Register capabilities from each module AFTER core's capability registry is running.
        register_all_capabilities(&self.capability_registry).await?;

        // Register executor candidates with the resolver (Block C).
        self.register_resolver_candidates();

        // Begin collecting the real event stream for the UI projection.
        let history = self.event_history.clone();
        let mut rx = self.event_bus.subscribe_all();
        self.event_drain = Some(tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                let mut ring = history.write().await;
                if ring.len() >= 500 {
                    ring.pop_front();
                }
                ring.push_back(envelope);
            }
        }));

        self.text_input.start().await?;
        self.text_output.start().await?;
        self.chat.start().await?;
        self.models.start().await?;
        self.model_router.start().await?
        self.ai.start().await?;
        self.memory.start().await?;
        self.tasks.start().await?;
        self.scheduler.start().await?;
        self.stt.start().await?;
        self.tts.start().await?;
        self.voice.start().await?;
        self.browser.start().await?;
        self.web_research.start().await?;
        self.selfmade.start().await?;
        // Establish the first durable self-model immediately at boot. Later
        // autonomy cycles consume this inventory instead of guessing about
        // JAMES capabilities or repository state.
        self.selfmade.observe_self().await?;
        self.void.start().await?;
        self.dashboard.start().await?;

        info!("JAMES assembly started: all first-party modules running");
        Ok(())
    }

    /// Stop modules in reverse order, then stop core.
    pub async fn stop(&mut self) -> Result<()> {
        self.dashboard.stop().await?;
        self.void.stop().await?;
        self.selfmade.stop().await?;
        self.web_research.stop().await?;
        self.browser.stop().await?;
        self.voice.stop().await?;
        self.tts.stop().await?;
        self.stt.stop().await?;
        self.scheduler.stop().await?;
        self.tasks.stop().await?;
        self.memory.stop().await?;
        self.ai.stop().await?;
        self.model_router.stop().await?;
        self.models.stop().await?;
        self.chat.stop().await?
        self.text_output.stop().await?;
        self.text_input.stop().await?;

        if let Some(handle) = self.event_drain.take() {
            handle.abort();
        }

        self.core.stop().await?;
        info!("JAMES assembly stopped");
        Ok(())
    }

    pub fn event_bus(&self) -> Arc<EventBus> {
        self.event_bus.clone()
    }

    pub fn capability_registry(&self) -> Arc<CapabilityRegistry> {
        self.capability_registry.clone()
    }

    pub fn resolver(&self) -> Arc<CapabilityResolver> {
        self.resolver.clone()
    }

    pub fn selfmade(&self) -> Arc<SelfMadeModule> {
        self.selfmade.clone()
    }

    /// Execute a user intent end-to-end (Block D):
    /// `Intent → Plan → resolver → CapabilityBroker → Executor → Result`.
    /// The assembly resolver selects candidates; the broker is the single
    /// enforcement point (permissions, policy, schema, audit).
    pub async fn execute_intent(
        &self,
        intent: UserIntent,
    ) -> Result<PlanExecutionResult> {
        let planner = Arc::new(HeuristicPlanner::new(
            AgentConfig::default(),
            self.capability_registry.clone(),
            self.event_bus.clone(),
        ));
        let plan = planner.create_plan(intent).await?;

        let executor = PlanExecutor::new(self.broker.clone());
        executor.attach_resolver(self.resolver.clone());

        let result = executor.execute(&plan, "user").await?;
        Ok(result)
    }

    /// Execute an intent using the LLM planner backed by the real AI module
    /// (Block D: `Intent → Plan → resolver → CapabilityBroker → Executor → Result`).
    /// Falls back to HeuristicPlanner if the AI module has no providers registered.
    pub async fn execute_ai_intent(
        &self,
        intent: UserIntent,
    ) -> Result<PlanExecutionResult> {
        let heuristic_plan = || async {
            let heuristic = HeuristicPlanner::new(
                AgentConfig::default(),
                self.capability_registry.clone(),
                self.event_bus.clone(),
            );
            heuristic.create_plan(intent.clone()).await
        };

        let plan = if self.ai.list_models().await.map_or(false, |m| !m.is_empty()) {
            let llm_planner = LlmPlanner::new(
                AgentConfig::default(),
                self.event_bus.clone(),
                self.capability_registry.clone(),
                Arc::new(AssemblyAiPlannerProvider { ai: self.ai.clone() }),
            );
            match llm_planner.create_plan(intent.clone()).await {
                Ok(plan) if plan.steps.iter().all(|step| {
                    self.resolver
                        .resolve_executor(&step.capability_id, &ResolutionContext::default())
                        .is_some()
                }) => plan,
                Ok(plan) => {
                    tracing::warn!(
                        "LLM plan contains capability steps without registered executors; falling back to heuristic planner"
                    );
                    heuristic_plan().await?
                }
                Err(error) => {
                    tracing::warn!(%error, "LLM planning failed; falling back to heuristic planner");
                    heuristic_plan().await?
                }
            }
        } else {
            heuristic_plan().await?
        };
        let executor = PlanExecutor::new(self.broker.clone());
        executor.attach_resolver(self.resolver.clone());
        Ok(executor.execute(&plan, "user").await?)
    }

    /// Spawn a typed agent with a given role name and config, attached to
    /// the assembly's resolver. The agent's executor candidates come from
    /// the shared resolver (Block D full-agent path).
    pub fn create_agent(
        &self,
        role: james_agents::AgentRole,
        name: &str,
        allowed_capabilities: Vec<String>,
    ) -> Arc<james_agents::Agent> {
        let allowed = if allowed_capabilities.is_empty() {
            vec!["*".to_string()]
        } else {
            allowed_capabilities
        };
        let config = AgentConfig {
            id: uuid::Uuid::now_v7().to_string(),
            name: name.to_string(),
            role,
            goal: Some("Execute plans against the assembly".to_string()),
            allowed_capabilities: allowed,
            max_concurrent_tasks: 4,
            timeout_secs: 300,
            budget_usd: None,
            workspace_id: None,
        };
        let agent = self.agent_registry.create_custom(config);
        agent.attach_resolver(self.resolver.clone());
        agent
    }

    /// Execute an intent through a full typed Agent lifecycle (state machine,
    /// events, metrics, broker enforcement), rather than a bare PlanExecutor.
    pub async fn execute_agent_intent(
        &self,
        intent: UserIntent,
    ) -> Result<(String, james_agents::PlanExecutionResult)> {
        let agent = self.create_agent(
            james_agents::AgentRole::Generic,
            "assembly-runtime",
            intent.context.available_capabilities.clone(),
        );
        let planner = Arc::new(HeuristicPlanner::new(
            agent.config().clone(),
            self.capability_registry.clone(),
            self.event_bus.clone(),
        ));
        let plan = planner.create_plan(intent).await?;
        let id = self.agent_registry.register(agent.clone());
        let result = agent.execute_plan(plan).await?;
        Ok((id, result))
    }

    /// The assembly's agent registry (create/register/list/remove).
    pub fn agent_registry(&self) -> Arc<james_agents::AgentRegistry> {
        self.agent_registry.clone()
    }

    /// Execute `memory.read` through the resolver path: resolve a candidate
    /// from the registry, then enforce via the broker.
    pub async fn execute_memory_read(
        &self,
        caller: impl Into<String>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let context = ResolutionContext::preferring("james-memory");
        let outcome = self.resolver.resolve("memory.read", &context);
        let candidate = outcome
            .selected
            .ok_or_else(|| anyhow::anyhow!("no executor candidate for memory.read"))?;
        let broker_outcome = self.broker.execute_v2(
            CapabilityRequestV2 {
                requested_effect: RequestedEffect::Read,
                ..CapabilityRequestV2::new(
                    caller.into(),
                    "memory.read",
                    serde_json::json!({"limit": limit}),
                )
            },
            candidate.executor.as_ref(),
        ).await?;
        let output = broker_outcome
            .output
            .ok_or_else(|| anyhow::anyhow!("memory.read returned no output"))?;
        serde_json::from_value(output["memories"].clone())
            .map_err(Into::into)
    }

    pub async fn respond(&self, message: &str) -> Result<String> {
        let trimmed = message.trim();
        if let Some(argument) = trimmed.strip_prefix("/memory") {
            let limit = argument.trim().parse::<usize>().unwrap_or(10).clamp(1, 100);
            let entries = self.execute_memory_read("james-chat", limit).await?;
            if entries.is_empty() {
                return Ok("No memory entries found.".to_string());
            }
            return Ok(entries
                .iter()
                .enumerate()
                .map(|(index, entry)| format!("{}. {}", index + 1, entry.content))
                .collect::<Vec<_>>()
                .join("\n"));
        }
        let candidate = self.resolver.resolve("void.chat", &ResolutionContext::preferring("james-void")).selected
            .ok_or_else(|| anyhow::anyhow!("no executor candidate for void.chat"))?;
        let outcome = self.broker.execute_v2(
            CapabilityRequestV2 {
                requested_effect: RequestedEffect::Communicate,
                ..CapabilityRequestV2::new(
                    "james-chat",
                    "void.chat",
                    serde_json::json!({"message": trimmed}),
                )
            },
            candidate.executor.as_ref(),
        ).await?;
        let output = outcome.output.ok_or_else(|| anyhow::anyhow!("void.chat returned no output"))?;
        Ok(output.get("content").and_then(|v| v.as_str()).unwrap_or("I’m processing your request.").to_string())
    }

    pub fn chat(&self) -> Arc<ChatModule> {
        self.chat.clone()
    }

    pub fn ai(&self) -> Arc<AiModule> {
        self.ai.clone()
    }

    /// Real, section-based dashboard data for the Void UI. Every value is
    /// read from the assembled modules or the event bus — nothing is mocked,
    /// and missing capabilities (agents, automation, resource sampling) are
    /// reported honestly instead of fabricated.
    pub async fn data_snapshot(&self, section: &str) -> Result<serde_json::Value> {
        match section {
            "status" => self.section_status().await,
            "tasks" => self.section_tasks().await,
            "memory" => self.section_memory().await,
            "modules" => self.section_modules().await,
            "capabilities" => self.section_capabilities().await,
            "events" => self.section_events().await,
            "resources" => self.section_resources().await,
            "security" => self.section_security().await,
            "settings" => self.section_settings().await,
            "ai" => self.section_ai().await,
            "agents" => self.section_agents().await,
            "devices" => Ok(json!({
                "port": "platform device port pending (F1-08)",
                "devices": []
            })),
            "automation" => self.section_automation().await,
            _ => Ok(json!({
                "error": format!("unknown dashboard section: {section}")
            })),
        }
    }

    async fn tasks_summary(&self) -> (usize, usize, usize) {
        let tasks = self.tasks.list_tasks(None, 1000).await.unwrap_or_default();
        let pending = tasks
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    james_tasks::TaskStatus::Created | james_tasks::TaskStatus::Queued
                )
            })
            .count();
        let running = tasks.iter().filter(|t| t.status == james_tasks::TaskStatus::Running).count();
        let completed = tasks
            .iter()
            .filter(|t| t.status == james_tasks::TaskStatus::Completed)
            .count();
        (pending, running, completed)
    }

    async fn section_status(&self) -> Result<serde_json::Value> {
        let (pending, running, completed) = self.tasks_summary().await;
        let memory_entries = self.memory.query(james_memory::MemoryQuery {
            memory_types: None,
            query_text: None,
            tags: None,
            session_id: None,
            agent_id: None,
            limit: 1,
            min_importance: None,
            since: None,
        })
        .await
        .map(|entries| entries.len())
        .unwrap_or(0);
        let observed = self.event_history.read().await.len();
        let modules_running = self
            .module_states()
            .await
            .into_iter()
            .filter(|(_, _, running)| *running)
            .count();
        Ok(json!({
            "core_status": "Running",
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": self.dashboard.get_status().await.uptime_secs,
            "capabilities": self.capability_registry.count(),
            "modules": modules_running,
            "memory_entries": memory_entries,
            "tasks_pending": pending,
            "tasks_running": running,
            "tasks_completed": completed,
            "agents": self.agent_registry.list().len(),
            "events_observed": observed,
            "events_dropped": self.event_bus.dropped_count(),
            "preview_unauthenticated": true,
        }))
    }

    async fn section_tasks(&self) -> Result<serde_json::Value> {
        let tasks = self.tasks.list_tasks(None, 200).await.unwrap_or_default();
        let items: Vec<serde_json::Value> = tasks
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "name": t.name,
                    "description": t.description,
                    "capability": t.capability,
                    "priority": format!("{:?}", t.priority),
                    "status": format!("{:?}", t.status),
                    "retries": t.retries,
                    "max_retries": t.max_retries,
                    "created_at": t.created_at,
                    "started_at": t.started_at,
                    "completed_at": t.completed_at,
                    "error": t.error,
                })
            })
            .collect();
        Ok(json!({ "tasks": items }))
    }

    async fn section_memory(&self) -> Result<serde_json::Value> {
        let entries = self
            .memory
            .query(james_memory::MemoryQuery {
                memory_types: None,
                query_text: None,
                tags: None,
                session_id: None,
                agent_id: None,
                limit: 100,
                min_importance: None,
                since: None,
            })
            .await
            .unwrap_or_default();
        let items: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "content": e.content,
                    "memory_type": format!("{:?}", e.memory_type),
                    "importance": e.importance,
                    "access_count": e.access_count,
                    "tags": e.tags,
                    "created_at": e.created_at,
                })
            })
            .collect();
        Ok(json!({ "entries": items, "total": items.len() }))
    }

    async fn section_modules(&self) -> Result<serde_json::Value> {
        let items: Vec<serde_json::Value> = self
            .module_states()
            .await
            .into_iter()
            .map(|(id, name, running)| json!({ "id": id, "name": name, "running": running }))
            .collect();
        Ok(json!({ "modules": items }))
    }

    async fn module_states(&self) -> Vec<(&'static str, &'static str, bool)> {
        vec![
            ("james.textinput", "Text Input", self.text_input.is_running().await),
            ("james.textoutput", "Text Output", self.text_output.is_running().await),
            ("james.chat", "Chat", self.chat.is_running().await),
            ("james.modelrouter", "Model Router", self.model_router.is_running().await),
            ("james.ai", "AI", self.ai.is_running().await),
            ("james.memory", "Memory", self.memory.is_running().await),
            ("james.tasks", "Tasks", self.tasks.is_running().await),
            ("james.scheduler", "Scheduler", self.scheduler.is_running().await),
            ("james.stt", "Speech-to-Text", self.stt.is_running().await),
            ("james.tts", "Text-to-Speech", self.tts.is_running().await),
            ("james.voice", "Voice", self.voice.is_running().await),
            ("james.browser", "Browser", self.browser.is_running().await),
            ("james.webresearch", "Web Research", self.web_research.is_running().await),
            ("james.void", "Void", self.void.is_running().await),
            ("james.dashboard", "Dashboard", self.dashboard.is_running().await),
        ]
    }

    async fn section_agents(&self) -> Result<serde_json::Value> {
        let agents = self.agent_registry.list();
        let mut items = Vec::with_capacity(agents.len());
        for agent in agents {
            let config = agent.config().clone();
            let metrics = agent.metrics().await;
            items.push(json!({
                "id": config.id,
                "name": config.name,
                "role": format!("{:?}", config.role),
                "goal": config.goal,
                "state": format!("{:?}", agent.state().await),
                "allowed_capabilities": config.allowed_capabilities,
                "max_concurrent_tasks": config.max_concurrent_tasks,
                "timeout_secs": config.timeout_secs,
                "budget_usd": config.budget_usd,
                "workspace_id": config.workspace_id,
                "metrics": {
                    "plans_executed": metrics.plans_executed,
                    "steps_completed": metrics.steps_completed,
                    "steps_failed": metrics.steps_failed,
                    "total_execution_time_ms": metrics.total_execution_time_ms,
                    "capabilities_used": metrics.capabilities_used,
                    "last_activity": metrics.last_activity,
                }
            }));
        }
        Ok(json!({
            "runtime": "typed agent registry",
            "count": items.len(),
            "agents": items
        }))
    }

    async fn section_automation(&self) -> Result<serde_json::Value> {
        Ok(json!({
            "runtime": "scheduler module",
            "running": self.scheduler.is_running().await,
            "note": "automation state is exposed by the scheduler; durable workflow persistence remains a separate runtime milestone"
        }))
    }

    async fn section_capabilities(&self) -> Result<serde_json::Value> {
        let registered = self.capability_registry.list_all();
        let items: Vec<serde_json::Value> = registered
            .iter()
            .map(|c| {
                json!({
                    "id": c.definition.id,
                    "name": c.definition.name,
                    "provider": c.definition.provider,
                    "category": format!("{:?}", c.definition.category),
                    "risk_level": format!("{:?}", c.definition.risk_level),
                    "status": format!("{:?}", c.status),
                    "usage_count": c.usage_count,
                    "experimental": c.definition.experimental,
                    "deprecated": c.definition.deprecated,
                    "description": c.definition.description,
                })
            })
            .collect();
        Ok(json!({ "capabilities": items }))
    }

    async fn section_events(&self) -> Result<serde_json::Value> {
        let history = self.event_history.read().await;
        let items: Vec<serde_json::Value> = history
            .iter()
            .rev()
            .take(120)
            .map(|envelope| {
                let event = &envelope.event;
                json!({
                    "event_type": event.event_type,
                    "source": event.source,
                    "target": event.target,
                    "severity": format!("{:?}", event.severity),
                    "timestamp": event.timestamp,
                    "correlation_id": event.correlation_id,
                    "causation_id": event.causation_id,
                    "payload": event.payload,
                })
            })
            .collect();
        Ok(json!({ "events": items }))
    }

    async fn section_resources(&self) -> Result<serde_json::Value> {
        let observed = self.event_history.read().await.len();
        let dropped = self.event_bus.dropped_count();
        let uptime = self.dashboard.get_status().await.uptime_secs;
        Ok(json!({
            "uptime_secs": uptime,
            "events_observed": observed,
            "events_dropped": dropped,
            "memory_entries": self.section_memory().await.ok()
                .and_then(|value| value.get("total").cloned())
                .unwrap_or(json!(0)),
            "tasks_active": self.tasks_summary().await.1,
            "platform_metrics": "cpu/gpu/ram sampling pending (Windows platform port, F1-08)",
            "collection": "partial",
        }))
    }

    async fn section_security(&self) -> Result<serde_json::Value> {
        let history = self.event_history.read().await;
        let trail: Vec<serde_json::Value> = history
            .iter()
            .rev()
            .filter(|envelope| {
                let event_type = envelope.event.event_type.as_str();
                event_type.starts_with("capability.")
                    || event_type.starts_with("approval.")
                    || event_type.starts_with("audit.")
                    || event_type == "void.task.delegated"
            })
            .take(60)
            .map(|envelope| {
                json!({
                    "event_type": envelope.event.event_type,
                    "source": envelope.event.source,
                    "severity": format!("{:?}", envelope.event.severity),
                    "timestamp": envelope.event.timestamp,
                    "payload": envelope.event.payload,
                })
            })
            .collect();
        Ok(json!({
            "boundary": "loopback-only (refuses non-loopback binds)",
            "authentication": "preview pending (A7b closes this)",
            "broker": "enforced: every capability request passes permission, policy, execution",
            "trail": trail
        }))
    }

    async fn section_settings(&self) -> Result<serde_json::Value> {
        Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "theme": "black / gold (luxury)",
            "motion": "semantic (state-driven profiles)",
            "sound": "disabled by default",
            "renderer": "Void + Dashboard share one state source",
            "preview_unauthenticated": true,
            "instance": env!("CARGO_PKG_NAME"),
        }))
    }

    async fn section_ai(&self) -> Result<serde_json::Value> {
        let models = self.ai.list_models().await.unwrap_or_default();
        // Keep the router's registry fresh (e.g. after `ollama pull`).
        let _ = self.model_router.sync_models(models.clone()).await;
        Ok(json!({
            "running": self.ai.is_running().await,
            "router_running": self.model_router.is_running().await,
            "models": models,
            "provider_note": "local provider (Ollama-compatible endpoint)",
        }))
    }

    pub fn broker(&self) -> Arc<CapabilityBroker> {
        self.broker.clone()
    }

    pub fn memory(&self) -> Arc<MemoryModule> {
        self.memory.clone()
    }

    pub fn tasks(&self) -> Arc<TasksModule> {
        self.tasks.clone()
    }

    pub fn scheduler(&self) -> Arc<SchedulerModule> {
        self.scheduler.clone()
    }

    pub fn voice(&self) -> Arc<VoiceModule> {
        self.voice.clone()
    }

    pub fn browser(&self) -> Arc<BrowserModule> {
        self.browser.clone()
    }

    pub fn web_research(&self) -> Arc<WebResearchModule> {
        self.web_research.clone()
    }

    pub fn void(&self) -> Arc<VoidModule> {
        self.void.clone()
    }

    pub fn dashboard(&self) -> Arc<DashboardModule> {
        self.dashboard.clone()
    }
}

/// Register every first-party module's capabilities into the shared registry.
async fn register_all_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    james_textinput::register_capabilities(registry).await?;
    james_textoutput::register_capabilities(registry).await?;
    james_chat::register_capabilities(registry).await?;
    james_models::register_capabilities(registry).await?;
    james_modelrouter::register_capabilities(registry).await?;
    james_ai::register_capabilities(registry).await?;
    james_memory::register_capabilities(registry).await?;
    james_tasks::register_capabilities(registry).await?;
    james_scheduler::register_capabilities(registry).await?;
    james_stt::register_capabilities(registry).await?;
    james_tts::register_capabilities(registry).await?;
    james_voice::register_capabilities(registry).await?;
    james_browser::register_capabilities(registry).await?;
    james_webresearch::register_capabilities(registry).await?;
    james_void::register_capabilities(registry).await?;
    james_dashboard::register_capabilities(registry).await?;
    james_selfmade::register_capabilities(registry).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_assembly_constructor_and_registers() {
        let core = JamesCore::new(james_core::CoreConfig::default()).await.unwrap();
        let mut assembly = JamesAssembly::new(core, AssemblyOptions::default()).await.unwrap();
        assert!(assembly.memory().is_running().await == false);
        assembly.start().await.unwrap();
        assert!(assembly.memory().is_running().await);
        let count = assembly.capability_registry().count();
        assert!(count > 10, "expected many capabilities, got {}", count);
        assembly.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_read_runs_through_broker() {
        let memory_path = std::env::temp_dir()
            .join(format!("james-memory-{}.json", uuid::Uuid::now_v7()));
        let core = JamesCore::new(james_core::CoreConfig::default()).await.unwrap();
        let mut assembly = JamesAssembly::new(core, AssemblyOptions {
            memory_path: Some(memory_path.to_string_lossy().into_owned()),
            ..AssemblyOptions::default()
        }).await.unwrap();

        assembly.start().await.unwrap();
        assembly.memory().add_to_working(
            "broker integration memory".to_string(),
            serde_json::json!({"test": true}),
        ).await.unwrap();

        let entries = assembly.execute_memory_read("test-agent", 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "broker integration memory");

        assembly.stop().await.unwrap();
        let _ = std::fs::remove_file(memory_path);
    }

    #[tokio::test]
    async fn test_all_memory_candidates_resolve_through_broker() {
        let memory_path = std::env::temp_dir()
            .join(format!("james-memory-{}.json", uuid::Uuid::now_v7()));
        let core = JamesCore::new(james_core::CoreConfig::default()).await.unwrap();
        let mut assembly = JamesAssembly::new(core, AssemblyOptions {
            memory_path: Some(memory_path.to_string_lossy().into_owned()),
            ..AssemblyOptions::default()
        }).await.unwrap();
        assembly.start().await.unwrap();

        assembly.memory().add_to_working(
            "a working memory entry".to_string(),
            serde_json::json!({"test": true}),
        ).await.unwrap();

        // Every memory.* capability registered by the module must resolve to a
        // candidate and execute through broker enforcement.
        for capability_id in ["memory.read", "memory.working", "memory.episodic",
            "memory.semantic", "memory.procedural", "memory.relationship"] {
            let candidate = assembly.resolver()
                .resolve(capability_id, &ResolutionContext::default())
                .selected
                .unwrap_or_else(|| panic!("no candidate for {capability_id}"));
            let outcome = assembly.broker().execute_v2(
                CapabilityRequestV2 {
                    requested_effect: RequestedEffect::Read,
                    ..CapabilityRequestV2::new(
                        "test-agent",
                        capability_id,
                        serde_json::json!({"limit": 10}),
                    )
                },
                candidate.executor.as_ref(),
            ).await.unwrap_or_else(|e| panic!("broker rejected {capability_id}: {e}"));
            assert!(outcome.executed, "{capability_id} did not execute");
            assert!(outcome.output.is_some(), "{capability_id} returned no output");
        }

        assembly.stop().await.unwrap();
        let _ = std::fs::remove_file(memory_path);
    }

    #[tokio::test]
    async fn test_memory_intent_uses_broker_path() {
        let memory_path = std::env::temp_dir()
            .join(format!("james-memory-intent-{}.json", uuid::Uuid::now_v7()));
        let core = JamesCore::new(james_core::CoreConfig::default()).await.unwrap();
        let mut assembly = JamesAssembly::new(core, AssemblyOptions {
            memory_path: Some(memory_path.to_string_lossy().into_owned()),
            ..AssemblyOptions::default()
        }).await.unwrap();
        assembly.start().await.unwrap();
        assembly.memory().add_to_working(
            "intent routed through capability broker".to_string(),
            serde_json::json!({}),
        ).await.unwrap();

        let response = assembly.respond("/memory 1").await.unwrap();
        assert_eq!(response, "1. intent routed through capability broker");

        assembly.stop().await.unwrap();
        let _ = std::fs::remove_file(memory_path);
    }

    #[tokio::test]
    async fn test_execute_intent_runs_through_resolver_and_broker() {
        use james_agents::ParsedIntent;
        use james_capabilities::{CapabilityCategory, CapabilityDefinition, ExecutionTarget, RiskLevel};

        let core = JamesCore::new(james_core::CoreConfig::default()).await.unwrap();
        let mut assembly = JamesAssembly::new(core, AssemblyOptions::default()).await.unwrap();
        assembly.start().await.unwrap();

        // Register a capability and provide an executor candidate for it.
        assembly.capability_registry()
            .register(CapabilityDefinition {
                id: "generic.execute".to_string(),
                name: "generic.execute".to_string(),
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
            }, "test".to_string()).await.unwrap();
        assembly.resolver().register_or_replace(ExecutorCandidate::new(
            "generic.execute",
            "test-provider",
            Arc::new(james_capability_broker::NoopExecutor),
        ));

        let intent = UserIntent::new(
            "user1".to_string(),
            "do the thing".to_string(),
            ParsedIntent {
                action: "do".to_string(),
                target: None,
                parameters: Default::default(),
                constraints: vec![],
                expected_output: None,
            },
        );

        let result = assembly.execute_intent(intent).await.unwrap();
        assert!(result.success, "plan should succeed end-to-end");
        assert_eq!(result.step_results.len(), 1);

        assembly.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_ai_intent_falls_back_to_heuristic_without_ai() {
        use james_agents::ParsedIntent;
        use james_capabilities::{CapabilityCategory, CapabilityDefinition, ExecutionTarget, RiskLevel};

        // With no AI providers registered, execute_ai_intent must behave
        // exactly like the heuristic path (Block D, honest fallback).
        let core = JamesCore::new(james_core::CoreConfig::default()).await.unwrap();
        let mut assembly = JamesAssembly::new(core, AssemblyOptions::default()).await.unwrap();
        assembly.start().await.unwrap();

        assembly.capability_registry()
            .register(CapabilityDefinition {
                id: "generic.execute".to_string(),
                name: "generic.execute".to_string(),
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
            }, "test".to_string()).await.unwrap();
        assembly.resolver().register_or_replace(ExecutorCandidate::new(
            "generic.execute",
            "test-provider",
            Arc::new(james_capability_broker::NoopExecutor),
        ));

        let intent = UserIntent::new(
            "user1".to_string(),
            "do the thing".to_string(),
            ParsedIntent {
                action: "do".to_string(),
                target: None,
                parameters: Default::default(),
                constraints: vec![],
                expected_output: None,
            },
        );

        let result = assembly.execute_ai_intent(intent).await.unwrap();
        assert!(result.success, "ai intent fallback plan should succeed");
        assert_eq!(result.step_results.len(), 1);

        assembly.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_execute_agent_intent_full_lifecycle() {
        use james_agents::ParsedIntent;
        use james_capabilities::{CapabilityCategory, CapabilityDefinition, ExecutionTarget, RiskLevel};

        // Full agent lifecycle: create agent → execute plan → state machine + metrics
        // runs through resolver → broker → executor. Agent emits lifecycle events
        // (plan.started/plan.completed) and tracks steps_completed.
        let core = JamesCore::new(james_core::CoreConfig::default()).await.unwrap();
        let mut assembly = JamesAssembly::new(core, AssemblyOptions::default()).await.unwrap();
        assembly.start().await.unwrap();

        assembly.capability_registry()
            .register(CapabilityDefinition {
                id: "generic.execute".to_string(),
                name: "generic.execute".to_string(),
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
            }, "test".to_string()).await.unwrap();
        assembly.resolver().register_or_replace(ExecutorCandidate::new(
            "generic.execute",
            "test-provider",
            Arc::new(james_capability_broker::NoopExecutor),
        ));

        let intent = UserIntent::new(
            "user1".to_string(),
            "do the thing".to_string(),
            ParsedIntent {
                action: "do".to_string(),
                target: None,
                parameters: Default::default(),
                constraints: vec![],
                expected_output: None,
            },
        );

        let (agent_id, result) = assembly.execute_agent_intent(intent).await.unwrap();
        assert!(result.success, "agent plan should succeed end-to-end");
        assert_eq!(result.step_results.len(), 1);
        assert!(!agent_id.is_empty(), "agent must be registered");

        // Verify agent is visible in registry and completed successfully
        let agent = assembly.agent_registry().get(&agent_id).unwrap();
        let state = agent.state().await;
        assert_eq!(state, james_agents::AgentState::Completed, "agent should be in Completed state after plan");
        let metrics = agent.metrics().await;
        assert_eq!(metrics.plans_executed, 1);
        assert_eq!(metrics.steps_completed, 1);

        assembly.stop().await.unwrap();
    }
}