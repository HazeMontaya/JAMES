//! James-System - Runnable JAMES system (core + all first-party modules).
//!
//! This is a convenience launcher for the fully assembled system. The
//! zero-module path is `james-app` (core only); this binary proves the
//! assembly wiring works end-to-end.

use anyhow::Result;
use async_trait::async_trait;
use james_agents::UserIntent;
use james_assembly::{AssemblyOptions, JamesAssembly};
use james_app_api::{serve, AppState, AgentHandler, ChatHandler, DashboardProvider, DashboardSnapshot};
use james_core::{CoreConfig, init_tracing, LogFields};
use std::sync::Arc;
use std::path::Path;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::signal;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, error};

struct AssemblyChatHandler {
    assembly: Arc<Mutex<JamesAssembly>>,
}

struct AssemblyAgentHandler {
    assembly: Arc<Mutex<JamesAssembly>>,
}

struct AssemblyData {
    assembly: Arc<Mutex<JamesAssembly>>,
}

#[async_trait]
impl DashboardProvider for AssemblyData {
    async fn snapshot(&self, section: &str) -> anyhow::Result<serde_json::Value> {
        self.assembly.lock().await.data_snapshot(section).await
    }
}

fn void_static_dir() -> String {
    ["ui/void", "../ui/void", "../../ui/void"]
        .iter()
        .find(|path| Path::new(*path).join("index.html").exists())
        .unwrap_or(&"ui/void")
        .to_string()
}

#[async_trait]
impl ChatHandler for AssemblyChatHandler {
    async fn respond(&self, message: &str) -> anyhow::Result<String> {
        self.assembly.lock().await.respond(message).await
    }
}

#[async_trait]
impl AgentHandler for AssemblyAgentHandler {
    async fn execute_intent(&self, intent: UserIntent) -> anyhow::Result<james_agents::PlanExecutionResult> {
        let (_agent_id, result) = self.assembly.lock().await.execute_agent_intent(intent).await?;
        Ok(result)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let log = LogFields::new("james-system");
    info!(
        "{} Starting JAMES System v{} (core + all first-party modules)",
        log.prefix(),
        env!("CARGO_PKG_VERSION")
    );

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

    let core = james_core::JamesCore::new(config).await?;
    let options = AssemblyOptions {
        enable_web: true,
        enable_voice: true,
        enable_dashboard: true,
        ..AssemblyOptions::default()
    };

    let assembly = Arc::new(Mutex::new(JamesAssembly::new(core, options).await?));
    assembly.lock().await.start().await?;

    let event_bus = {
        let assembly = assembly.lock().await;
        assembly.event_bus()
    };

    let dashboard = Arc::new(RwLock::new(DashboardSnapshot {
        core_status: "Running".to_string(),
        capabilities: assembly.lock().await.capability_registry().count(),
        ..Default::default()
    }));

    // Keep the lightweight overview snapshot honest by re-deriving it from
    // the assembled modules every few seconds.
    {
        let dashboard = dashboard.clone();
        let assembly = assembly.clone();
        tokio::spawn(async move {
            let mut tick =
                tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tick.tick().await;
                if let Ok(value) = assembly.lock().await.data_snapshot("status").await {
                    let mut snap = dashboard.write().await;
                    snap.core_status = value
                        .get("core_status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Running")
                        .to_string();
                    snap.modules = value.get("modules").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.capabilities = value.get("capabilities").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.memory_entries = value.get("memory_entries").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.tasks_pending = value.get("tasks_pending").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.tasks_running = value.get("tasks_running").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.tasks_completed = value.get("tasks_completed").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.agents = value.get("agents").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.events_dropped = value.get("events_dropped").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    snap.uptime_secs = value.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0);
                }
            }
        });
    }

    let api_state = AppState {
        bus: event_bus,
        status: Arc::new(RwLock::new(james_events::CoreStatus::Running)),
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at: chrono::Utc::now(),
        static_dir: void_static_dir(),
        dashboard: dashboard.clone(),
        preview_unauthenticated: true,
        chat_handler: Some(Arc::new(AssemblyChatHandler {
            assembly: assembly.clone(),
        })),
        data: Some(Arc::new(AssemblyData {
            assembly: assembly.clone(),
        })),
        broker: None,
        executor: None,
        agent_handler: Some(Arc::new(AssemblyAgentHandler {
            assembly: assembly.clone(),
        })),
        intent: None,
        token_store: None,
    };
    let api_listener = tokio::net::TcpListener::bind(("127.0.0.1", 38241)).await?;
    let api_handle = tokio::spawn(serve(api_listener, api_state));

    info!("{} JAMES System running. Type a message or press Ctrl+C to stop.", log.prefix());

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    loop {
        tokio::select! {
            signal_result = signal::ctrl_c() => {
                signal_result?;
                break;
            }
            line_result = lines.next_line() => {
                match line_result? {
                    Some(line) if !line.trim().is_empty() => {
                        match assembly.lock().await.respond(line.trim()).await {
                            Ok(response) => info!(
                                "{} JAMES: {}",
                                log.prefix(),
                                response
                            ),
                            Err(error) => error!(
                                "{} request failed: {}",
                                log.prefix(),
                                error
                            ),
                        }
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }

    info!("{} Shutdown signal received", log.prefix());
    api_handle.abort();
    assembly.lock().await.stop().await?;

    info!("{} JAMES System stopped", log.prefix());
    Ok(())
}