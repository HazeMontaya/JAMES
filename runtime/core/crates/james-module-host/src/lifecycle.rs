//! Module lifecycle management

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{ModuleHostError, ModuleManifest, ModuleMeta};
use crate::ModuleState::{self, *};

/// Module lifecycle manager
pub struct ModuleLifecycle {
    // State transitions are validated by the state machine
}

impl ModuleLifecycle {
    /// Validate a state transition
    pub fn can_transition(from: ModuleState, to: ModuleState) -> bool {
        use ModuleState::*;
        match (from, to) {
            // Discovery flow
            (Discovered, Validating) => true,
            (Validating, Installed) => true,
            (Validating, Failed) => true,
            
            // Installation flow
            (Installed, Registered) => true,
            (Installed, Failed) => true,
            
            // Registration flow
            (Registered, Enabled) => true,
            (Registered, Disabled) => true,
            (Registered, Failed) => true,
            
            // Enable/Disable
            (Enabled, Starting) => true,
            (Enabled, Disabled) => true,
            (Enabled, Updating) => true,
            (Enabled, Blocked) => true,
            (Disabled, Enabled) => true,
            (Disabled, Failed) => true,
            
            // Start/Stop
            (Starting, Running) => true,
            (Starting, Failed) => true,
            (Starting, Blocked) => true,
            (Running, Stopping) => true,
            (Running, Failed) => true,
            (Running, Disabled) => true,
            (Running, Updating) => true,
            (Running, Blocked) => true,
            (Stopping, Stopped) => true,
            (Stopping, Failed) => true,
            
            // Blocked
            (Blocked, Enabled) => true,
            (Blocked, Failed) => true,
            (Blocked, Updating) => true,
            (Blocked, Disabled) => true,
            (Blocked, Unloaded) => true,
            
            // Updating
            (Updating, Running) => true,
            (Updating, Enabled) => true,
            (Updating, Failed) => true,
            (Updating, Blocked) => true,
            (Updating, Stopped) => true,
            
            // Recovery
            (Failed, Discovered) => true,  // Retry from discovery
            (Stopped, Enabled) => true,     // Re-enable
            (Stopped, Disabled) => true,    // Disable after stop
            (Stopped, Updating) => true,    // Update while stopped
            (Failed, Enabled) => true,      // Retry after failure
            (Failed, Updating) => true,     // Update before retry
            
            // Unloaded can go back to discovery
            (Unloaded, Discovered) => true,
            (Unloaded, Failed) => true,
            
            // Same state (no-op)
            (a, b) if a == b => true,
            
            _ => false,
        }
    }
    
    /// Get all valid next states from a given state
    pub fn valid_transitions(from: ModuleState) -> Vec<ModuleState> {
        use ModuleState::*;
        match from {
            Discovered => vec![Validating, Failed],
            Validating => vec![Installed, Failed],
            Installed => vec![Registered, Failed],
            Registered => vec![Enabled, Disabled, Failed],
            Enabled => vec![Starting, Disabled, Updating, Blocked],
            Disabled => vec![Enabled, Failed],
            Starting => vec![Running, Failed, Blocked],
            Running => vec![Stopping, Disabled, Failed, Updating, Blocked],
            Blocked => vec![Enabled, Failed, Updating, Disabled, Unloaded],
            Updating => vec![Running, Enabled, Failed, Blocked, Stopped],
            Stopping => vec![Stopped, Failed],
            Stopped => vec![Enabled, Disabled, Failed, Updating],
            Failed => vec![Discovered, Enabled, Updating, Failed],
            Unloaded => vec![Discovered, Failed],
        }
    }
    
    /// Check if a state is terminal (no automatic transitions out)
    pub fn is_terminal(state: ModuleState) -> bool {
        matches!(state, ModuleState::Stopped | ModuleState::Failed)
    }
    
    /// Check if a state represents an active module
    pub fn is_active(state: ModuleState) -> bool {
        matches!(
            state,
            ModuleState::Starting | ModuleState::Running | ModuleState::Updating
        )
    }
}

/// Module handle for external interaction
pub struct ModuleHandle {
    module_id: String,
    // In a real implementation, this would hold a reference to the module instance
}

impl ModuleHandle {
    pub fn id(&self) -> &str {
        &self.module_id
    }
}

/// Module host with lifecycle management
pub struct ModuleHost {
    modules: std::collections::HashMap<String, ModuleMeta>,
    // In a real implementation, this would hold:
    // - Module instances
    // - Communication channels
    // - Resource handles
}

impl Default for ModuleHost {
    fn default() -> Self {
        Self {
            modules: std::collections::HashMap::new(),
        }
    }
}

impl ModuleHost {
    /// Register a new module
    pub fn register(&mut self, meta: ModuleMeta) -> Result<(), String> {
        if self.modules.contains_key(&meta.id) {
            return Err(format!("Module {} already exists", meta.id));
        }
        self.modules.insert(meta.id.clone(), meta);
        Ok(())
    }
    
    /// Transition a module to a new state
    pub fn transition(&mut self, id: &str, new_state: ModuleState) -> Result<(), String> {
        let meta = self.modules.get_mut(id)
            .ok_or_else(|| format!("Module {} not found", id))?;
        
        if !crate::lifecycle::ModuleLifecycle::can_transition(meta.state, new_state) {
            return Err(format!(
                "Invalid state transition: {:?} -> {:?}",
                meta.state, new_state
            ));
        }
        
        meta.state = new_state;
        Ok(())
    }
    
    /// Get module metadata
    pub fn get(&self, id: &str) -> Option<&crate::ModuleMeta> {
        self.modules.get(id)
    }
    
    /// Get all modules
    pub fn all(&self) -> Vec<&crate::ModuleMeta> {
        self.modules.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_state_transitions() {
        use ModuleState::*;
        
        // Valid transitions
        assert!(ModuleLifecycle::can_transition(Discovered, Validating));
        assert!(ModuleLifecycle::can_transition(Validating, Installed));
        assert!(ModuleLifecycle::can_transition(Installed, Registered));
        assert!(ModuleLifecycle::can_transition(Registered, Enabled));
        assert!(ModuleLifecycle::can_transition(Enabled, Starting));
        assert!(ModuleLifecycle::can_transition(Starting, Running));
        assert!(ModuleLifecycle::can_transition(Running, Stopping));
        assert!(ModuleLifecycle::can_transition(Stopping, Stopped));
        assert!(ModuleLifecycle::can_transition(Stopped, Enabled));
        assert!(ModuleLifecycle::can_transition(Failed, Discovered));
        assert!(ModuleLifecycle::can_transition(Failed, Enabled));
        
        // Blocked/Updating transitions (concept §2.2)
        assert!(ModuleLifecycle::can_transition(Running, Blocked));
        assert!(ModuleLifecycle::can_transition(Blocked, Enabled));
        assert!(ModuleLifecycle::can_transition(Blocked, Updating));
        assert!(ModuleLifecycle::can_transition(Blocked, Unloaded));
        assert!(ModuleLifecycle::can_transition(Running, Updating));
        assert!(ModuleLifecycle::can_transition(Updating, Running));
        assert!(ModuleLifecycle::can_transition(Enabled, Blocked));
        assert!(ModuleLifecycle::can_transition(Updating, Failed));
        assert!(!ModuleLifecycle::can_transition(Blocked, Running));

        // Invalid transitions
        assert!(!ModuleLifecycle::can_transition(Discovered, Installed));
        assert!(!ModuleLifecycle::can_transition(Running, Enabled));
        assert!(!ModuleLifecycle::can_transition(Stopped, Running));
        
        // Self transitions allowed
        assert!(ModuleLifecycle::can_transition(Running, Running));
    }
    
    #[test]
    fn test_blocked_updating_recovery() {
        use ModuleState::*;
        // Security block -> recovery paths
        assert!(ModuleLifecycle::valid_transitions(Blocked).contains(&Enabled));
        assert!(ModuleLifecycle::valid_transitions(Blocked).contains(&Unloaded));
        assert!(!ModuleLifecycle::valid_transitions(Blocked).contains(&Running));
        // Update round-trip
        assert!(ModuleLifecycle::is_active(Updating));
        assert!(!ModuleLifecycle::is_active(Blocked));
        assert!(!ModuleLifecycle::is_terminal(Blocked));
        assert!(!ModuleLifecycle::is_terminal(Updating));
    }
    
    #[test]
    fn test_valid_transitions() {
        let transitions = ModuleLifecycle::valid_transitions(ModuleState::Running);
        assert!(transitions.contains(&ModuleState::Stopping));
        assert!(ModuleLifecycle::valid_transitions(ModuleState::Failed).contains(&ModuleState::Enabled));
    }
}