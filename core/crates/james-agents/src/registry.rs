//! Executor registry — maps capability ids to their [`CapabilityExecutor`]
//! implementations so agents can resolve executors at runtime.

use std::sync::Arc;

use dashmap::DashMap;
use james_capability_broker::CapabilityExecutor;

/// Registry of capability executors.
pub struct ExecutorRegistry {
    executors: DashMap<String, Arc<dyn CapabilityExecutor>>,
    provider: String,
}

impl Default for ExecutorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self {
            executors: DashMap::new(),
            provider: "unknown".to_string(),
        }
    }

    /// Create a registry tagged with a provider name (for diagnostics).
    pub fn with_provider(provider: impl Into<String>) -> Self {
        Self {
            executors: DashMap::new(),
            provider: provider.into(),
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Register an executor. Returns `false` if one is already registered.
    pub fn register(
        &self,
        capability_id: impl Into<String>,
        executor: Arc<dyn CapabilityExecutor>,
    ) -> bool {
        let key = capability_id.into();
        if self.executors.contains_key(&key) {
            return false;
        }
        self.executors.insert(key, executor);
        true
    }

    /// Register or replace an executor for a capability id.
    pub fn register_or_replace(
        &self,
        capability_id: impl Into<String>,
        executor: Arc<dyn CapabilityExecutor>,
    ) {
        self.executors.insert(capability_id.into(), executor);
    }

    /// Look up an executor by capability id.
    pub fn get(&self, capability_id: &str) -> Option<Arc<dyn CapabilityExecutor>> {
        self.executors.get(capability_id).map(|e| e.clone())
    }

    /// Remove an executor; returns it if present.
    pub fn unregister(&self, capability_id: &str) -> Option<Arc<dyn CapabilityExecutor>> {
        self.executors.remove(capability_id).map(|(_, e)| e)
    }

    /// All registered capability ids.
    pub fn ids(&self) -> Vec<String> {
        self.executors.iter().map(|e| e.key().clone()).collect()
    }

    /// Number of registered executors.
    pub fn len(&self) -> usize {
        self.executors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.executors.is_empty()
    }

    /// Bulk-register a map of capability ids to executors.
    pub fn register_many(
        &self,
        executors: Vec<(impl Into<String>, Arc<dyn CapabilityExecutor>)>,
    ) -> usize {
        let mut registered = 0;
        for (id, ex) in executors {
            if self.register(id, ex) {
                registered += 1;
            }
        }
        registered
    }

    /// Seed a [`CapabilityResolver`](crate::CapabilityResolver) with all
    /// registered executors, tagging them with this registry's provider.
    pub fn seed(&self, resolver: &crate::CapabilityResolver) {
        for entry in self.executors.iter() {
            let candidate = crate::ExecutorCandidate::new(
                entry.key().clone(),
                self.provider.clone(),
                entry.value().clone(),
            );
            resolver.register_or_replace(candidate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_capability_broker::NoopExecutor;

    #[test]
    fn test_register_and_get() {
        let reg = ExecutorRegistry::new();
        let ex: Arc<dyn CapabilityExecutor> = Arc::new(NoopExecutor);
        assert!(reg.register("memory.read", ex.clone()));
        assert!(!reg.register("memory.read", ex.clone())); // duplicate
        assert!(reg.get("memory.read").is_some());
        assert!(reg.get("missing").is_none());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_register_or_replace() {
        let reg = ExecutorRegistry::new();
        let ex: Arc<dyn CapabilityExecutor> = Arc::new(NoopExecutor);
        reg.register_or_replace("a.b", ex.clone());
        reg.register_or_replace("a.b", ex.clone());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn test_unregister() {
        let reg = ExecutorRegistry::new();
        let ex: Arc<dyn CapabilityExecutor> = Arc::new(NoopExecutor);
        reg.register("a.b", ex.clone());
        assert!(reg.unregister("a.b").is_some());
        assert!(reg.get("a.b").is_none());
        assert!(reg.unregister("a.b").is_none());
    }

    #[test]
    fn test_register_many() {
        let reg = ExecutorRegistry::new();
        let ex: Arc<dyn CapabilityExecutor> = Arc::new(NoopExecutor);
        let n = reg.register_many(vec![("a", ex.clone()), ("b", ex.clone())]);
        assert_eq!(n, 2);
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn test_provider_tag() {
        let reg = ExecutorRegistry::with_provider("test-provider");
        assert_eq!(reg.provider(), "test-provider");
    }
}