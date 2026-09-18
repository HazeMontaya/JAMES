//! James-WebResearch - Web search and research for JAMES

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebResearchConfig {
    pub search_provider: String,
    pub max_results: usize,
    pub timeout_secs: u64,
}

impl Default for WebResearchConfig {
    fn default() -> Self {
        Self { search_provider: "duckduckgo".to_string(), max_results: 10, timeout_secs: 30 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub rank: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchReport {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub summary: Option<String>,
    pub sources: Vec<String>,
}

pub struct WebResearchModule {
    config: WebResearchConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
}

impl WebResearchModule {
    pub fn new(config: WebResearchConfig, event_bus: Arc<EventBus>, capability_registry: Arc<CapabilityRegistry>) -> Self {
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)) }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-WebResearch started (provider={})", self.config.search_provider);
        self.event_bus.publish(Event::new("module.webresearch.started", "james-webresearch")).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-WebResearch stopped");
        self.event_bus.publish(Event::new("module.webresearch.stopped", "james-webresearch")).await?;
        Ok(())
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let url = format!("https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query));
        let resp = reqwest::get(&url).await?.json::<serde_json::Value>().await?;
        let mut results = Vec::new();
        if let Some(abstract_text) = resp.get("Abstract").and_then(|a| a.as_str()) {
            if !abstract_text.is_empty() {
                results.push(SearchResult {
                    title: resp.get("Heading").and_then(|h| h.as_str()).unwrap_or("Result").to_string(),
                    url: resp.get("AbstractURL").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                    snippet: abstract_text.to_string(), rank: 1,
                });
            }
        }
        if let Some(related) = resp.get("RelatedTopics").and_then(|r| r.as_array()) {
            for (i, topic) in related.iter().take(self.config.max_results).enumerate() {
                if let Some(text) = topic.get("Text").and_then(|t| t.as_str()) {
                    results.push(SearchResult {
                        title: topic.get("FirstURL").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                        url: topic.get("FirstURL").and_then(|u| u.as_str()).unwrap_or("").to_string(),
                        snippet: text.to_string(), rank: i + 2,
                    });
                }
            }
        }
        Ok(results)
    }

    pub async fn fetch_url(&self, url: &str) -> Result<String> {
        let resp = reqwest::get(url).await?.text().await?;
        Ok(html_escape::decode_html_entities(&resp).to_string())
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.webresearch".to_string(), name: "James-WebResearch".to_string(), version: "0.1.0".to_string(),
        description: "Web search and research capabilities for JAMES".to_string(), module_type: ModuleType::Service,
        entry_point: "james_webresearch".to_string(),
        capabilities: vec!["web.search".to_string(), "web.fetch".to_string(), "web.research".to_string()],
        dependencies: vec![], permissions: vec![],
        configuration_schema: None, default_config: None,
        author: Some("JAMES Project".to_string()), homepage: None, repository: None,
        license: "MIT".to_string(), tags: vec!["web".to_string(), "research".to_string(), "search".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id, name, desc) in [
        ("web.search", "Web Search", "Search the web"),
        ("web.fetch", "Web Fetch", "Fetch URL content"),
        ("web.research", "Web Research", "Conduct deep research"),
    ] {
        registry.register(CapabilityDefinition {
            id: id.to_string(), name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("web".to_string()),
            version: "1.0.0".to_string(), provider: "james.webresearch".to_string(),
            description: desc.to_string(), risk_level: RiskLevel::Low,
            required_permissions: vec![], dependencies: vec![],
            input_schema: None, output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["web".to_string()], deprecated: false, experimental: false,
        }, "james.webresearch".to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_webresearch_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.webresearch");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}