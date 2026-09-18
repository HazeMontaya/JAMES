//! JAMES Python Bridge - NATS-based bridge between Rust core and Python services
//!
//! This crate provides a bridge that connects the Rust core (EventBus, CapabilityRegistry,
//! CapabilityBroker) with Python services (SkillEngine, Memory, WorldAccess) via NATS JetStream.

mod bridge;
mod config;
mod capability_sync;
mod executor;
mod nats_bridge;

pub use bridge::PythonBridge;
pub use config::BridgeConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_config_default() {
        let config = BridgeConfig::default();
        assert_eq!(config.nats_url, "nats://localhost:4222");
        assert_eq!(config.python_service_name, "james-python");
        assert!(config.auto_register_skills);
    }
}