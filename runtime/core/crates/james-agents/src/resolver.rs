//! Capability resolver — selects the best executor for a capability
//! requirement out of the registered candidates.
//!
//! Resolution pipeline (Block C):
//! `Requirement → Candidates → Compatibility → Selection`
//!
//! The broker stays the single enforcement point (permissions, policy,
//! schema, audit). The resolver never executes anything; it only picks
//! which [`CapabilityExecutor`] the broker should run.

use std::sync::Arc;

use dashmap::DashMap;
use james_capability_broker::CapabilityExecutor;
use serde::{Deserialize, Serialize};

/// A single registered executor candidate for a capability id.
#[derive(Clone, Serialize)]
pub struct ExecutorCandidate {
    /// Capability id this candidate implements (e.g. `memory.read`).
    pub capability_id: String,
    /// Named provider for diagnostics and preference matching.
    pub provider: String,
    /// Priority; higher wins when multiple candidates qualify.
    pub priority: i32,
    /// Whether this candidate is currently usable.
    pub available: bool,
    /// Structured capability list the provider reports it can serve.
    pub capabilities: Vec<String>,
    /// Free-form health/resources detail for diagnostics.
    pub health: Option<String>,
    /// The executor itself. Serialization skips it by design.
    #[serde(skip)]
    pub executor: Arc<dyn CapabilityExecutor>,
}

impl std::fmt::Debug for ExecutorCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutorCandidate")
            .field("capability_id", &self.capability_id)
            .field("provider", &self.provider)
            .field("priority", &self.priority)
            .field("available", &self.available)
            .field("capabilities", &self.capabilities)
            .field("health", &self.health)
            .field("executor", &"<Arc<dyn CapabilityExecutor>>")
            .finish()
    }
}

impl ExecutorCandidate {
    /// Convenience constructor with defaults: available, priority 0, no extras.
    pub fn new(
        capability_id: impl Into<String>,
        provider: impl Into<String>,
        executor: Arc<dyn CapabilityExecutor>,
    ) -> Self {
        let capability_id = capability_id.into();
        Self {
            capability_id: capability_id.clone(),
            provider: provider.into(),
            priority: 0,
            available: true,
            capabilities: vec![capability_id],
            health: None,
            executor,
        }
    }
}

/// Constraints for a single resolution request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolutionContext {
    /// Only consider providers in this list (in this order).
    pub preferred_providers: Vec<String>,
    /// Minimum priority; lower-priority candidates are skipped.
    pub min_priority: Option<i32>,
    /// Whether unavailable candidates may be returned as a fallback.
    pub include_unavailable: bool,
}

impl ResolutionContext {
    /// A resolution that prefers a single provider, otherwise any provider.
    pub fn preferring(provider: &str) -> Self {
        Self {
            preferred_providers: vec![provider.to_string()],
            ..Default::default()
        }
    }
}

/// How the resolver answered a resolution request.
#[derive(Debug, Clone, Serialize)]
pub struct ResolutionOutcome {
    pub capability_id: String,
    pub selected: Option<ExecutorCandidate>,
    pub candidates_considered: Vec<String>,
    pub filtered_out: Vec<(String, String)>,
}

/// Runtime health state for a provider. Health is maintained separately from
/// candidate registration so providers can change state without being
/// re-registered and long-lived agents can observe failover and recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealth {
    Available,
    Degraded,
    Unavailable,
}

impl ProviderHealth {
    fn rank(self) -> u8 {
        match self {
            Self::Available => 0,
            Self::Degraded => 1,
            Self::Unavailable => 2,
        }
    }

    fn is_routable(self) -> bool {
        !matches!(self, Self::Unavailable)
    }
}

/// Default resolver: chooses the best candidate for a capability id.
///
/// Selection order (stable, deterministic):
/// 1. preference rank from `ResolutionContext::preferred_providers`
/// 2. availability (`available == true` first)
/// 3. priority (higher wins)
/// 4. registration order (earlier wins)
pub struct CapabilityResolver {
    candidates: DashMap<String, Vec<ExecutorCandidate>>,
    provider_health: DashMap<String, ProviderHealth>,
    capability_health: DashMap<(String, String), ProviderHealth>,
}

impl Default for CapabilityResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityResolver {
    pub fn new() -> Self {
        Self {
            candidates: DashMap::new(),
            provider_health: DashMap::new(),
            capability_health: DashMap::new(),
        }
    }

    /// Register a candidate. Returns `false` when a same-provider candidate
    /// for the same capability id already exists.
    pub fn register(&self, candidate: ExecutorCandidate) -> bool {
        let mut entry = self.candidates.entry(candidate.capability_id.clone()).or_default();
        if entry.iter().any(|c| c.provider == candidate.provider) {
            return false;
        }
        entry.push(candidate);
        true
    }

    /// Register a candidate, replacing an existing same-provider one.
    pub fn register_or_replace(&self, candidate: ExecutorCandidate) {
        let mut entry = self.candidates.entry(candidate.capability_id.clone()).or_default();
        entry.retain(|c| c.provider != candidate.provider);
        entry.push(candidate);
    }

    /// Set runtime health for every capability supplied by a provider.
    /// Unavailable removes the provider from normal resolution; Degraded
    /// remains routable but ranks behind healthy providers.
    pub fn set_provider_health(&self, provider: &str, health: ProviderHealth) -> usize {
        self.provider_health.insert(provider.to_string(), health);
        self.candidates
            .iter()
            .map(|entry| entry.value().iter().filter(|c| c.provider == provider).count())
            .sum()
    }

    /// Read effective provider health. Unknown providers are healthy by default.
    pub fn provider_health(&self, provider: &str) -> ProviderHealth {
        self.provider_health
            .get(provider)
            .map(|v| *v)
            .unwrap_or(ProviderHealth::Available)
    }

    /// Set health for one capability/provider route without affecting the
    /// provider's other capabilities.
    pub fn set_capability_health(
        &self,
        capability_id: &str,
        provider: &str,
        health: ProviderHealth,
    ) {
        self.capability_health.insert(
            (capability_id.to_string(), provider.to_string()),
            health,
        );
    }

    /// Read effective health for a concrete capability route. A route-specific
    /// state overrides provider-wide state.
    pub fn capability_health(
        &self,
        capability_id: &str,
        provider: &str,
    ) -> ProviderHealth {
        self.capability_health
            .get(&(capability_id.to_string(), provider.to_string()))
            .map(|v| *v)
            .unwrap_or_else(|| self.provider_health(provider))
    }

    /// Remove runtime health state and return to the default healthy state.
    pub fn clear_provider_health(&self, provider: &str) -> bool {
        self.provider_health.remove(provider).is_some()
    }

    /// Remove all candidates for a provider.
    pub fn unregister_provider(&self, provider: &str) -> usize {
        let affected: Vec<String> = self
            .candidates
            .iter()
            .filter(|e| e.value().iter().any(|c| c.provider == provider))
            .map(|e| e.key().clone())
            .collect();
        let mut removed = 0;
        for id in &affected {
            if self.unregister(id, provider) {
                removed += 1;
            }
        }
        removed
    }

    /// Remove a single candidate by capability id and provider.
    pub fn unregister(&self, capability_id: &str, provider: &str) -> bool {
        let removed_any = if let Some(mut entry) = self.candidates.get_mut(capability_id) {
            let before = entry.len();
            entry.retain(|c| c.provider != provider);
            entry.len() < before
        } else {
            false
        };
        if removed_any {
            // Drop empty vectors so `len`/`is_empty` stay accurate.
            self.candidates.remove_if(capability_id, |_, v| v.is_empty());
        }
        removed_any
    }

    /// Number of registered capability ids.
    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// All capability ids that have at least one candidate.
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.candidates.iter().map(|e| e.key().clone()).collect();
        ids.sort();
        ids
    }

    /// All providers that registered at least one candidate.
    pub fn providers(&self) -> Vec<String> {
        let mut providers: Vec<String> = self
            .candidates
            .iter()
            .flat_map(|e| {
                e.value()
                    .iter()
                    .map(|c| c.provider.clone())
                    .collect::<Vec<String>>()
            })
            .collect();
        providers.sort();
        providers.dedup();
        providers
    }

    /// All candidates for a capability id, in registration order.
    pub fn candidates(&self, capability_id: &str) -> Vec<ExecutorCandidate> {
        self.candidates.get(capability_id).map(|e| e.value().clone()).unwrap_or_default()
    }

    /// Concrete resolve: pick the best candidate for a capability id.
    pub fn resolve(
        &self,
        capability_id: &str,
        context: &ResolutionContext,
    ) -> ResolutionOutcome {
        let all = self.candidates(capability_id);
        let mut considered = all.clone();
        let mut filtered: Vec<(String, String)> = Vec::new();

        for candidate in &all {
            let health = self.capability_health(&candidate.capability_id, &candidate.provider);
            if !candidate.available && !context.include_unavailable {
                filtered.push((candidate.provider.clone(), "unavailable".to_string()));
            }
            if !health.is_routable() && !context.include_unavailable {
                filtered.push((candidate.provider.clone(), "provider unavailable".to_string()));
            }
            if let Some(min) = context.min_priority {
                if candidate.priority < min {
                    filtered.push((
                        candidate.provider.clone(),
                        format!("priority {} below minimum {min}", candidate.priority),
                    ));
                }
            }
        }

        considered.retain(|c| {
            let health = self.provider_health(&c.provider);
            (c.available || context.include_unavailable)
                && (health.is_routable() || context.include_unavailable)
                && context.min_priority.map_or(true, |min| c.priority >= min)
        });

        // Deterministic tie-break: slice::sort_by is stable, so among equal
        // elements registration order is preserved.
        let mut ranked: Vec<usize> = (0..considered.len()).collect();
        ranked.sort_by(|a, b| {
            let ca = &considered[*a];
            let cb = &considered[*b];
            let pa = preference_rank(&context.preferred_providers, &ca.provider);
            let pb = preference_rank(&context.preferred_providers, &cb.provider);
            pa.cmp(&pb)
                .then_with(|| self.capability_health(&ca.capability_id, &ca.provider).rank().cmp(&self.capability_health(&cb.capability_id, &cb.provider).rank()))
                .then_with(|| cb.available.cmp(&ca.available))
                .then_with(|| cb.priority.cmp(&ca.priority))
        });

        let selected = ranked.first().map(|&idx| considered[idx].clone());

        ResolutionOutcome {
            capability_id: capability_id.to_string(),
            selected,
            candidates_considered: considered.iter().map(|c| c.provider.clone()).collect(),
            filtered_out: filtered,
        }
    }

    /// Resolve and return the executor (convenience wrapper).
    pub fn resolve_executor(
        &self,
        capability_id: &str,
        context: &ResolutionContext,
    ) -> Option<Arc<dyn CapabilityExecutor>> {
        self.resolve(capability_id, context)
            .selected
            .map(|c| c.executor.clone())
    }
}

/// Stable preference rank: 0 is most preferred, `usize::MAX` means "not listed".
fn preference_rank(preferences: &[String], provider: &str) -> usize {
    preferences
        .iter()
        .position(|p| p == provider)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_capability_broker::NoopExecutor;

    fn candidate(id: &str, provider: &str, priority: i32) -> ExecutorCandidate {
        ExecutorCandidate {
            capability_id: id.to_string(),
            provider: provider.to_string(),
            priority,
            available: true,
            capabilities: vec![id.to_string()],
            health: None,
            executor: Arc::new(NoopExecutor),
        }
    }

    #[test]
    fn test_register_duplicate_provider_rejected() {
        let resolver = CapabilityResolver::new();
        assert!(resolver.register(candidate("a.b", "p1", 0)));
        assert!(!resolver.register(candidate("a.b", "p1", 1)));
        assert!(resolver.register(candidate("a.b", "p2", 0)));
        assert_eq!(resolver.candidates("a.b").len(), 2);
    }

    #[test]
    fn test_register_or_replace_same_provider() {
        let resolver = CapabilityResolver::new();
        resolver.register_or_replace(candidate("a.b", "p1", 0));
        resolver.register_or_replace(candidate("a.b", "p1", 5));
        assert_eq!(resolver.candidates("a.b").len(), 1);
        assert_eq!(resolver.candidates("a.b")[0].priority, 5);
    }

    #[test]
    fn test_resolve_picks_highest_priority() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "low", 1));
        resolver.register(candidate("a.b", "high", 10));
        resolver.register(candidate("a.b", "mid", 5));

        let outcome = resolver.resolve("a.b", &ResolutionContext::default());
        assert_eq!(outcome.selected.unwrap().provider, "high");
    }

    #[test]
    fn test_resolve_prefers_provider() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "default", 100));
        resolver.register(candidate("a.b", "special", 1));

        let outcome = resolver.resolve("a.b", &ResolutionContext::preferring("special"));
        assert_eq!(outcome.selected.unwrap().provider, "special");
    }

    #[test]
    fn test_resolve_filters_unavailable() {
        let resolver = CapabilityResolver::new();
        let mut down = candidate("a.b", "down", 100);
        down.available = false;
        resolver.register(down);
        resolver.register(candidate("a.b", "up", 1));

        let outcome = resolver.resolve("a.b", &ResolutionContext::default());
        assert_eq!(outcome.selected.unwrap().provider, "up");
        assert!(outcome.filtered_out.iter().any(|(p, _)| p == "down"));
    }

    #[test]
    fn test_resolve_include_unavailable_falls_back() {
        let resolver = CapabilityResolver::new();
        let mut down = candidate("a.b", "down", 100);
        down.available = false;

        // With only an unavailable candidate, include_unavailable falls back to it.
        resolver.register(down.clone());
        let outcome = resolver.resolve(
            "a.b",
            &ResolutionContext {
                include_unavailable: true,
                ..Default::default()
            },
        );
        assert_eq!(outcome.selected.unwrap().provider, "down");

        // Without include_unavailable, nothing is returned.
        let outcome = resolver.resolve("a.b", &ResolutionContext::default());
        assert!(outcome.selected.is_none());

        // An available candidate still wins over an unavailable higher-priority one.
        resolver.register(candidate("a.b", "up", 1));
        let outcome = resolver.resolve(
            "a.b",
            &ResolutionContext {
                include_unavailable: true,
                ..Default::default()
            },
        );
        assert_eq!(outcome.selected.unwrap().provider, "up");
    }

    #[test]
    fn test_provider_health_failover_and_recovery() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "primary", 100));
        resolver.register(candidate("a.b", "backup", 1));

        assert_eq!(resolver.resolve("a.b", &ResolutionContext::default()).selected.unwrap().provider, "primary");

        assert_eq!(resolver.set_provider_health("primary", ProviderHealth::Unavailable), 1);
        assert_eq!(resolver.resolve("a.b", &ResolutionContext::default()).selected.unwrap().provider, "backup");

        assert_eq!(resolver.set_provider_health("primary", ProviderHealth::Degraded), 1);
        assert_eq!(resolver.resolve("a.b", &ResolutionContext::default()).selected.unwrap().provider, "backup");

        assert_eq!(resolver.set_provider_health("primary", ProviderHealth::Available), 1);
        assert_eq!(resolver.resolve("a.b", &ResolutionContext::default()).selected.unwrap().provider, "primary");
    }

    #[test]
    fn test_capability_health_overrides_provider_health() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "python", 100));
        resolver.register(candidate("x.y", "python", 100));
        resolver.register(candidate("a.b", "backup", 1));

        resolver.set_provider_health("python", ProviderHealth::Available);
        resolver.set_capability_health("a.b", "python", ProviderHealth::Unavailable);

        assert_eq!(
            resolver.resolve("a.b", &ResolutionContext::default()).selected.unwrap().provider,
            "backup"
        );
        assert_eq!(
            resolver.resolve("x.y", &ResolutionContext::default()).selected.unwrap().provider,
            "python"
        );
    }

    #[test]
    fn test_provider_health_defaults_and_clear() {
        let resolver = CapabilityResolver::new();
        assert_eq!(resolver.provider_health("unknown"), ProviderHealth::Available);
        resolver.set_provider_health("p1", ProviderHealth::Unavailable);
        assert_eq!(resolver.provider_health("p1"), ProviderHealth::Unavailable);
        assert!(resolver.clear_provider_health("p1"));
        assert_eq!(resolver.provider_health("p1"), ProviderHealth::Available);
        assert!(!resolver.clear_provider_health("p1"));
    }

    #[test]
    fn test_resolve_min_priority() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "low", 1));
        resolver.register(candidate("a.b", "high", 10));

        let outcome = resolver.resolve(
            "a.b",
            &ResolutionContext {
                min_priority: Some(5),
                ..Default::default()
            },
        );
        assert_eq!(outcome.selected.unwrap().provider, "high");
    }

    #[test]
    fn test_resolve_unknown_capability() {
        let resolver = CapabilityResolver::new();
        let outcome = resolver.resolve("missing", &ResolutionContext::default());
        assert!(outcome.selected.is_none());
        assert!(outcome.candidates_considered.is_empty());
    }

    #[test]
    fn test_unregister() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "p1", 0));
        resolver.register(candidate("a.b", "p2", 0));
        assert!(resolver.unregister("a.b", "p1"));
        assert_eq!(resolver.candidates("a.b").len(), 1);
        assert_eq!(resolver.unregister_provider("p2"), 1);
        assert!(resolver.is_empty());
    }

    #[test]
    fn test_metadata() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "p2", 0));
        resolver.register(candidate("x.y", "p1", 0));
        assert_eq!(resolver.ids(), vec!["a.b".to_string(), "x.y".to_string()]);
        assert_eq!(resolver.providers(), vec!["p1".to_string(), "p2".to_string()]);
        assert_eq!(resolver.len(), 2);
    }

    #[test]
    fn test_resolve_executor() {
        let resolver = CapabilityResolver::new();
        resolver.register(candidate("a.b", "p1", 1));
        assert!(resolver.resolve_executor("a.b", &ResolutionContext::default()).is_some());
        assert!(resolver.resolve_executor("nope", &ResolutionContext::default()).is_none());
    }
}