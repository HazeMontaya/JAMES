//! Main Python Bridge - ties together NATS, capability sync, and execution

use james_capability_broker::CapabilityBroker;
use james_capabilities::CapabilityRegistry;
use james_core::JamesCore;
use james_events::EventBus;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tracing::info;

use crate::capability_sync::{CapabilitySync, start_capability_sync_task};
use crate::config::BridgeConfig;
use crate::executor::PythonExecutor;
use crate::nats_bridge::NatsBridge;

/// Main bridge struct that manages all components
pub struct PythonBridge {
    config: BridgeConfig,
    nats_bridge: Arc<RwLock<Option<Arc<NatsBridge>>>>,
    capability_sync: Option<CapabilitySync>,
    capability_broker: Option<Arc<CapabilityBroker>>,
    registry: Option<Arc<CapabilityRegistry>>,
    event_bus: Option<Arc<EventBus>>,
    james_core: Option<Arc<RwLock<JamesCore>>>,
    python_executor: Option<Arc<PythonExecutor>>,
    background_tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl PythonBridge {
    /// Create a new bridge with default configuration
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            config,
            nats_bridge: Arc::new(RwLock::new(None)),
            capability_sync: None,
            capability_broker: None,
            registry: None,
            event_bus: None,
            james_core: None,
            python_executor: None,
            background_tasks: Mutex::new(Vec::new()),
        }
    }

    /// Create bridge from existing JamesCore (for integration)
    pub fn from_james_core(config: BridgeConfig, core: Arc<RwLock<JamesCore>>) -> Self {
        let mut bridge = Self::new(config);
        bridge.james_core = Some(core);
        bridge
    }

    /// Initialize the bridge (connect NATS, set up registries)
    pub async fn initialize(&mut self) -> anyhow::Result<()> {
        info!("Initializing Python Bridge");

        // Get or create JamesCore
        let core = if let Some(core_lock) = &self.james_core {
            let core_guard = core_lock.read().await;
            // JamesCore doesn't implement Clone, so we just use the reference
            // The core is already in self.james_core, we just need its components
            core_guard
        } else {
            // Create minimal core for standalone bridge
            let config = james_core::CoreConfig::default();
            let core = JamesCore::new(config).await?;
            self.james_core = Some(Arc::new(RwLock::new(core)));
            self.james_core.as_ref().unwrap().read().await
        };

        // Extract components from core (need to hold the lock while extracting)
        let event_bus = core.event_bus();
        let capability_registry = core.capability_registry();
        self.event_bus = Some(event_bus.clone());
        self.registry = Some(capability_registry.clone());
        self.capability_broker = Some(Arc::new(CapabilityBroker::new(capability_registry.clone())
            .with_event_bus(event_bus.clone())));

        // Create NATS bridge
        let mut nats_bridge = NatsBridge::new(self.config.clone());
        nats_bridge.set_event_bus(event_bus.clone());
        nats_bridge.connect().await?;

        let nats_bridge_arc = Arc::new(nats_bridge);
        *self.nats_bridge.write().await = Some(nats_bridge_arc.clone());

        // Create capability sync
        let registry = capability_registry.clone();
        self.capability_sync = Some(CapabilitySync::new(registry));

        // Create Python executor
        self.python_executor = Some(Arc::new(PythonExecutor::new(nats_bridge_arc.clone())));

        // Register Python executor with broker
        if let Some(_broker) = &self.capability_broker {
            // The executor is registered per-capability when syncing
        }

// Start background capability sync
        let sync = Arc::new((*self.capability_sync.as_ref().unwrap()).clone());
        let nats = nats_bridge_arc.clone();
        let handle = tokio::spawn(async move {
            start_capability_sync_task(sync, nats, 60).await;
        });
        self.background_tasks.lock().await.push(handle);

        info!("Python Bridge initialized successfully");
        Ok(())
    }

    /// Start the bridge (start core if not already running)
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if let Some(core_lock) = &self.james_core {
            let mut core = core_lock.write().await;
            core.start().await?;
        }
        Ok(())
    }

    /// Execute a capability through the bridge (uses Python executor)
    pub async fn execute_capability(
        &self,
        capability_id: &str,
        caller: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let nats_bridge = self.nats_bridge.read().await
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("NATS bridge not initialized"))?;

        let response = nats_bridge
            .execute_capability(capability_id, caller, input)
            .await?;

        if !response.success {
            return Err(anyhow::anyhow!(
                "Python capability '{}' failed: {}",
                capability_id,
                response.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        response.output.ok_or_else(|| {
            anyhow::anyhow!("Python capability '{}' returned no output", capability_id)
        })
    }

    /// Execute via broker (full enforcement chain)
    pub async fn execute_via_broker(
        &self,
        capability_id: &str,
        caller: &str,
        input: serde_json::Value,
    ) -> anyhow::Result<james_capability_broker::CapabilityOutcome> {
        let broker = self.capability_broker.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Capability broker not initialized"))?;

        let executor = self.python_executor.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Python executor not initialized"))?;

        let request = james_capability_broker::CapabilityRequest {
            caller: caller.to_string(),
            capability_id: capability_id.to_string(),
            input,
        };

        broker.execute(request, executor.as_ref()).await
    }

    /// Get the capability broker for direct access
    pub fn capability_broker(&self) -> Option<&Arc<CapabilityBroker>> {
        self.capability_broker.as_ref()
    }

    /// Get the registry for direct access
    pub fn registry(&self) -> Option<&Arc<CapabilityRegistry>> {
        self.registry.as_ref()
    }

    /// Get the event bus for direct access
    pub fn event_bus(&self) -> Option<&Arc<EventBus>> {
        self.event_bus.as_ref()
    }

    /// Get the JamesCore for direct access
    pub fn james_core(&self) -> Option<&Arc<RwLock<JamesCore>>> {
        self.james_core.as_ref()
    }

    /// Sync Python capabilities to Rust registry
    pub async fn sync_capabilities(&self) -> anyhow::Result<()> {
        if let Some(nats_bridge) = self.nats_bridge.read().await.as_ref() {
            let caps = nats_bridge.list_python_capabilities().await;
            if let Some(sync) = &self.capability_sync {
                sync.sync_all(&caps).await?;
            }
        }
        Ok(())
    }

    /// Get list of Python capabilities
    pub async fn list_python_capabilities(&self) -> Vec<crate::nats_bridge::PythonCapabilityInfo> {
        if let Some(nats_bridge) = self.nats_bridge.read().await.as_ref() {
            nats_bridge.list_python_capabilities().await
        } else {
            Vec::new()
        }
    }

    /// Shutdown the bridge
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        info!("Shutting down Python Bridge");

        // Abort background tasks
        let mut tasks = self.background_tasks.lock().await;
        for task in tasks.drain(..) {
            task.abort();
        }

        // Shutdown NATS bridge
        if let Some(nats_bridge) = self.nats_bridge.write().await.take() {
            nats_bridge.shutdown().await;
        }

        // Stop JamesCore if we own it
        if let Some(core_lock) = &self.james_core {
            let mut core = core_lock.write().await;
            let _ = core.stop().await;
        }

        info!("Python Bridge shutdown complete");
        Ok(())
    }
}

impl Drop for PythonBridge {
    fn drop(&mut self) {
        // Best effort cleanup
        let mut tasks = self.background_tasks.blocking_lock();
        for task in tasks.drain(..) {
            task.abort();
        }
    }
}