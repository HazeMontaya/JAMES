//! Module permissions and capability enforcement

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::{ModulePermission};

/// Permission checker for modules
pub struct ModulePermissionChecker {
    // Module ID -> granted permissions
    grants: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    
    // Capability -> required permissions mapping
    capability_requirements: HashMap<String, Vec<String>>,
    
    // Policy engine (simplified)
    policies: Vec<PermissionPolicy>,

    // Optional capability broker (forwarded enforcement, §6/§7 chain)
    broker: Arc<RwLock<Option<Arc<james_capability_broker::CapabilityBroker>>>>,
}

impl Default for ModulePermissionChecker {
    fn default() -> Self {
        let mut reqs = HashMap::new();
        
        // Define default capability -> permission mappings
        reqs.insert("system.files.read".to_string(), vec!["filesystem.read".to_string()]);
        reqs.insert("system.files.write".to_string(), vec!["filesystem.write".to_string()]);
        reqs.insert("system.process.start".to_string(), vec!["process.execute".to_string()]);
        reqs.insert("system.network.connect".to_string(), vec!["network.connect".to_string()]);
        reqs.insert("system.network.listen".to_string(), vec!["network.bind".to_string()]);
        reqs.insert("device.bluetooth".to_string(), vec!["bluetooth.access".to_string()]);
        reqs.insert("device.camera".to_string(), vec!["camera.access".to_string()]);
        reqs.insert("device.microphone".to_string(), vec!["microphone.access".to_string()]);
        reqs.insert("ai.local.inference".to_string(), vec!["ai.inference".to_string()]);
        reqs.insert("ai.cloud.inference".to_string(), vec!["ai.inference".to_string(), "network.connect".to_string()]);
        reqs.insert("web.browse".to_string(), vec!["network.connect".to_string()]);
        reqs.insert("web.scrape".to_string(), vec!["network.connect".to_string(), "web.scrape".to_string()]);
        reqs.insert("container.create".to_string(), vec!["container.manage".to_string()]);
        reqs.insert("container.exec".to_string(), vec!["container.manage".to_string(), "process.execute".to_string()]);
        
        Self {
            grants: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            capability_requirements: reqs,
            policies: Vec::new(),
            broker: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }
}

impl ModulePermissionChecker {
    /// Attach a capability broker for forwarded enforcement.
    pub async fn set_broker(&self, broker: Arc<james_capability_broker::CapabilityBroker>) {
        let mut guard = self.broker.write().await;
        *guard = Some(broker);
    }

    /// Broker decision for a capability a module wants to use.
    async fn broker_decision(&self, module_id: &str, capability: &str) -> Option<james_capability_broker::PolicyDecision> {
        let guard = self.broker.read().await;
        let broker = guard.as_ref()?;
        let request = james_capability_broker::CapabilityRequest {
            caller: module_id.to_string(),
            capability_id: capability.to_string(),
            input: serde_json::json!({}),
        };
        broker.decide(&request).await.ok()
    }

    /// Check if a module has a specific permission
    pub async fn has_permission(&self, module_id: &str, permission: &str) -> bool {
        let grants = self.grants.read().await;
        grants.get(module_id)
            .map(|perms| perms.contains(permission))
            .unwrap_or(false)
    }
    
    /// Check if a module has all required permissions for a capability.
    ///
    /// When a broker is attached, its decision is authoritative: an explicit
    /// Deny/Ask/Conditional blocks usage even if local grants exist.
    pub async fn can_use_capability(&self, module_id: &str, capability: &str) -> bool {
        if let Some(decision) = self.broker_decision(module_id, capability).await {
            return match decision {
                james_capability_broker::PolicyDecision::Allow => true,
                _ => false,
            };
        }
        let required = self.capability_requirements.get(capability);
        if let Some(required_perms) = required {
            for perm in required_perms {
                if !self.has_permission(module_id, perm).await {
                    return false;
                }
            }
            true
        } else {
            // No specific requirements = allowed
            true
        }
    }
    
    /// Grant a permission to a module
    pub async fn grant_permission(&self, module_id: &str, permission: &str) {
        let mut grants = self.grants.write().await;
        grants.entry(module_id.to_string()).or_default().insert(permission.to_string());
    }
    
    /// Revoke a permission from a module
    pub async fn revoke_permission(&self, module_id: &str, permission: &str) {
        let mut grants = self.grants.write().await;
        if let Some(perms) = grants.get_mut(module_id) {
            perms.remove(permission);
        }
    }
    
    /// Grant all required permissions for a capability
    pub async fn grant_capability_permissions(&self, module_id: &str, capability: &str) -> Result<(), String> {
        if let Some(required) = self.capability_requirements.get(capability) {
            for perm in required {
                self.grant_permission(module_id, perm).await;
            }
            Ok(())
        } else {
            Err(format!("Unknown capability: {}", capability))
        }
    }
    
    /// Check if module has all permissions for its declared capabilities
    pub async fn validate_module_permissions(&self, module_id: &str, capabilities: &[String]) -> Result<Vec<String>, Vec<String>> {
        let mut missing = Vec::new();
        let mut satisfied = Vec::new();
        
        for cap in capabilities {
            if self.can_use_capability(module_id, cap).await {
                satisfied.push(cap.clone());
            } else {
                missing.push(cap.clone());
            }
        }
        
        if missing.is_empty() {
            Ok(satisfied)
        } else {
            Err(missing)
        }
    }
    
    /// Get all permissions for a module
    pub async fn get_module_permissions(&self, module_id: &str) -> Vec<String> {
        self.grants.read().await
            .get(module_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    /// Get all modules with a specific permission
    pub async fn modules_with_permission(&self, permission: &str) -> Vec<String> {
        let grants = self.grants.read().await;
        grants.iter()
            .filter(|(_, perms)| perms.contains(permission))
            .map(|(id, _)| id.clone())
            .collect()
    }
    
    /// Revoke all permissions for a module (on uninstall)
    pub async fn revoke_all_permissions(&self, module_id: &str) {
        let mut grants = self.grants.write().await;
        grants.remove(module_id);
    }
    
    /// Add a custom policy
    pub fn add_policy(&mut self, policy: PermissionPolicy) {
        self.policies.push(policy);
    }
    
    /// Evaluate all policies for a request
    pub async fn evaluate_policies(&self, module_id: &str, action: &str, resource: &str) -> PolicyDecision {
        for policy in &self.policies {
            if policy.applies_to(module_id, action, resource) {
                return policy.decide(module_id, action, resource);
            }
        }
        PolicyDecision::Allow // Default allow if no policy matches
    }
    
    /// Initialize permissions for a newly installed module
    pub async fn initialize_module_permissions(&self, module_id: &str, permissions: &[String], capabilities: &[String]) {
        // Grant explicitly declared permissions
        for perm in permissions {
            self.grant_permission(module_id, perm).await;
        }
        
        // Grant implied permissions from capabilities
        for cap in capabilities {
            let _ = self.grant_capability_permissions(module_id, cap).await;
        }
    }
}

/// Policy decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    Allow,
    Deny,
    RequireApproval,
}

/// Permission policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub module_pattern: Option<String>, // Glob pattern for module IDs
    pub action_pattern: Option<String>, // Action pattern (e.g., "capability.*")
    pub resource_pattern: Option<String>, // Resource pattern
    pub decision: PolicyDecision,
    pub priority: i32, // Higher priority = evaluated first
}

impl PermissionPolicy {
    pub fn applies_to(&self, module_id: &str, action: &str, resource: &str) -> bool {
        if let Some(pattern) = &self.module_pattern {
            if !glob_match(pattern, module_id) {
                return false;
            }
        }
        if let Some(pattern) = &self.action_pattern {
            if !glob_match(pattern, action) {
                return false;
            }
        }
        if let Some(pattern) = &self.resource_pattern {
            if !glob_match(pattern, resource) {
                return false;
            }
        }
        true
    }
    
    pub fn decide(&self, _module_id: &str, _action: &str, _resource: &str) -> PolicyDecision {
        self.decision
    }
}

/// Simple glob matching
fn glob_match(pattern: &str, text: &str) -> bool {
    // Simple glob: * matches anything, ? matches single char
    let pattern: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    glob_match_from(&pattern, 0, &text_chars, 0)
}

fn glob_match_from(pattern: &[char], p: usize, text: &[char], t: usize) -> bool {
    let mut p = p;
    let mut t = t;
    
    while p < pattern.len() && t < text.len() {
        match pattern[p] {
            '*' => {
                // Try to match zero or more characters
                for i in (t..=text.len()).rev() {
                    if glob_match_from(pattern, p + 1, text, i) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                p += 1;
                t += 1;
            }
            c => {
                if c != text[t] {
                    return false;
                }
                p += 1;
                t += 1;
            }
        }
        
        // Handle trailing *
        while p < pattern.len() && pattern[p] == '*' {
            p += 1;
        }
    }
    
    // Handle trailing * in pattern
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    
    p == pattern.len() && t == text.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_permission_checker() {
        let checker = ModulePermissionChecker::default();
        
        // Grant permission
        checker.grant_permission("module1", "filesystem.read").await;
        assert!(checker.has_permission("module1", "filesystem.read").await);
        assert!(!checker.has_permission("module1", "filesystem.write").await);
        assert!(!checker.has_permission("module2", "filesystem.read").await);
        
        // Test capability permissions
        checker.grant_capability_permissions("module1", "system.files.read").await.unwrap();
        assert!(checker.can_use_capability("module1", "system.files.read").await);
        
        // Revoke
        checker.revoke_permission("module1", "filesystem.read").await;
        assert!(!checker.has_permission("module1", "filesystem.read").await);
    }
    
    #[tokio::test]
    async fn test_capability_permissions() {
        let checker = ModulePermissionChecker::default();
        
        // Grant capability permissions
        checker.grant_capability_permissions("mod1", "system.files.read").await.unwrap();
        assert!(checker.has_permission("mod1", "filesystem.read").await);
        
        checker.grant_capability_permissions("mod1", "system.process.start").await.unwrap();
        assert!(checker.has_permission("mod1", "process.execute").await);
        
        // Unknown capability
        let result = checker.grant_capability_permissions("mod1", "unknown.capability").await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_validate_permissions() {
        let checker = ModulePermissionChecker::default();
        
        checker.grant_permission("mod1", "filesystem.read").await;
        checker.grant_permission("mod1", "process.execute").await;
        
        let caps = vec![
            "system.files.read".to_string(),
            "system.process.start".to_string(),
            "system.network.connect".to_string(),
        ];
        
        let result = checker.validate_module_permissions("mod1", &caps).await;
        assert!(result.is_err()); // network.connect missing
        
        let missing = result.unwrap_err();
        assert_eq!(missing, vec!["system.network.connect"]);
        
        checker.grant_permission("mod1", "network.connect").await;
        let result = checker.validate_module_permissions("mod1", &caps).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), caps);
    }

    #[tokio::test]
    async fn test_broker_enforcement_blocks_denied_capability() {
        use james_capability_broker::{CapabilityBroker, PolicyRule};
        use james_capabilities::{CapabilityDefinition, CapabilityCategory, ExecutionTarget, RiskLevel};

        fn def(id: &str, perms: Vec<&str>) -> CapabilityDefinition {
            CapabilityDefinition {
                id: id.to_string(),
                name: id.to_string(),
                category: CapabilityCategory::Custom("test".to_string()),
                version: "1.0.0".to_string(),
                provider: "james-core".to_string(),
                description: "test".to_string(),
                risk_level: RiskLevel::Low,
                required_permissions: perms.into_iter().map(str::to_string).collect(),
                dependencies: vec![],
                input_schema: None,
                output_schema: None,
                execution_target: ExecutionTarget::Local,
                tags: vec![],
                deprecated: false,
                experimental: false,
            }
        }

        // Broker without grant -> denied even if local grants exist.
        let reg = james_capabilities::CapabilityRegistry::new();
        reg.register(def("test.broker.cap", vec!["filesystem.read"]), "james-core")
            .await
            .unwrap();
        let broker = CapabilityBroker::new(Arc::new(reg)).without_audit();
        let checker = ModulePermissionChecker::default();
        checker.set_broker(Arc::new(broker)).await;

        checker.grant_permission("mod1", "filesystem.read").await;
        assert!(!checker.can_use_capability("mod1", "test.broker.cap").await);

        // Broker grant mirrors local grant -> allowed.
        let reg2 = james_capabilities::CapabilityRegistry::new();
        reg2.register(def("test.broker.cap", vec!["filesystem.read"]), "james-core")
            .await
            .unwrap();
        let broker2 = CapabilityBroker::new(Arc::new(reg2)).without_audit();
        broker2.grant_capability_permissions("mod1", "test.broker.cap");
        let checker2 = ModulePermissionChecker::default();
        checker2.set_broker(Arc::new(broker2)).await;
        assert!(checker2.can_use_capability("mod1", "test.broker.cap").await);

        // Policy deny blocks despite grants.
        let reg3 = james_capabilities::CapabilityRegistry::new();
        reg3.register(def("test.broker.cap", vec!["filesystem.read"]), "james-core")
            .await
            .unwrap();
        let broker3 = CapabilityBroker::new(Arc::new(reg3)).without_audit();
        broker3.grant_capability_permissions("mod1", "test.broker.cap");
        broker3.add_policy(PolicyRule::deny("test.broker.cap", "test policy"));
        let checker3 = ModulePermissionChecker::default();
        checker3.set_broker(Arc::new(broker3)).await;
        assert!(!checker3.can_use_capability("mod1", "test.broker.cap").await);
    }
}