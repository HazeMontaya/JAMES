//! James-Assembly - Wire all first-party modules into a running JAMES system.
//!
//! Constructs the shared infrastructure (EventBus, CapabilityRegistry), then
//! builds and starts every first-party module in dependency order. Exposes
//! handles so the app layer can drive modules (chat, tasks, scheduler, etc.).

use std::sync::Arc;

use anyhow::Result;
use james_capabilities::CapabilityRegistry;
use james_core::JamesCore;
use james_events::EventBus;
use tokio::sync::RwLock;
use tracing::info;

use james_ai::{AiModule, AiProvider};
use james_browser::BrowserModule;
use james_chat::ChatModule;
use james_dashboard::DashboardModule;
use james_memory::MemoryModule;
use james_modelrouter::{ModelRouterModule, RouterConfig};
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

/// Fully assembled JAMES system: core + all first-party modules.
pub struct JamesAssembly {
    core: JamesCore,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,

    text_input: Arc<TextInputModule>,
    text_output: Arc<TextOutputModule>,
    chat: Arc<ChatModule>,
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
    void: Arc<VoidModule>,
    dashboard: Arc<DashboardModule>,
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

        // ---- Model registry + routing ----
        let model_router = Arc::new(ModelRouterModule::new(
            RouterConfig::default(),
            event_bus.clone(),
            capability_registry.clone(),
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

        // ---- Tasks (depends on memory) ----
        let tasks = Arc::new(TasksModule::new(
            TasksModuleConfig {
                database_path: options.tasks_path
                    .unwrap_or_else(|| ".james/tasks.json".to_string()),
                ..Default::default()
            },
            event_bus.clone(),
            capability_registry.clone(),
            memory.clone(),
        ));

        // ---- Scheduler (depends on tasks) ----
        let scheduler = Arc::new(SchedulerModule::new(
            SchedulerModuleConfig {
                database_path: options.scheduler_path
                    .unwrap_or_else(|| ".james/scheduler.json".to_string()),
                ..Default::default()
            },
            event_bus.clone(),
            capability_registry.clone(),
            tasks.clone(),
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

        Ok(Self {
            core,
            event_bus,
            capability_registry,
            text_input,
            text_output,
            chat,
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
            void,
            dashboard,
        })
    }

    /// Start core, then register module capabilities, then start modules in dependency order.
    pub async fn start(&mut self) -> Result<()> {
        self.core.start().await?;

        // Register capabilities from each module AFTER core's capability registry is running.
        register_all_capabilities(&self.capability_registry).await?;

        self.text_input.start().await?;
        self.text_output.start().await?;
        self.chat.start().await?;
        self.model_router.start().await?;
        self.ai.start().await?;
        self.memory.start().await?;
        self.tasks.start().await?;
        self.scheduler.start().await?;
        self.stt.start().await?;
        self.tts.start().await?;
        self.voice.start().await?;
        self.browser.start().await?;
        self.web_research.start().await?;
        self.void.start().await?;
        self.dashboard.start().await?;

        info!("JAMES assembly started: all first-party modules running");
        Ok(())
    }

    /// Stop modules in reverse order, then stop core.
    pub async fn stop(&mut self) -> Result<()> {
        self.dashboard.stop().await?;
        self.void.stop().await?;
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
        self.chat.stop().await?;
        self.text_output.stop().await?;
        self.text_input.stop().await?;

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

    pub fn chat(&self) -> Arc<ChatModule> {
        self.chat.clone()
    }

    pub fn ai(&self) -> Arc<AiModule> {
        self.ai.clone()
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
}