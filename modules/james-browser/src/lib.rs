//! James-Browser - Browser automation for JAMES

use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub headless: bool,
    pub user_agent: Option<String>,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self { headless: true, user_agent: None, viewport_width: 1280, viewport_height: 720 }
    }
}

pub struct BrowserModule {
    config: BrowserConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
}

impl BrowserModule {
    pub fn new(config: BrowserConfig, event_bus: Arc<EventBus>, capability_registry: Arc<CapabilityRegistry>) -> Self {
        Self { config, event_bus, capability_registry, running: Arc::new(RwLock::new(false)) }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("James-Browser started (headless={})", self.config.headless);
        self.event_bus.publish(Event::new("module.browser.started", "james-browser")).await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("James-Browser stopped");
        self.event_bus.publish(Event::new("module.browser.stopped", "james-browser")).await?;
        Ok(())
    }

    pub async fn navigate(&self, url: &str) -> Result<BrowserResult> {
        info!("Navigating to {}", url);
        let resp = reqwest::get(url).await?;
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Ok(BrowserResult { url: url.to_string(), status, content_length: body.len(), title: extract_title(&body) })
    }

    pub async fn is_running(&self) -> bool { *self.running.read().await }
}

fn extract_title(html: &str) -> Option<String> {
    html.split("<title>").nth(1)?.split("</title>").next().map(|s| s.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserResult {
    pub url: String,
    pub status: u16,
    pub content_length: usize,
    pub title: Option<String>,
}

pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.browser".to_string(), name: "James-Browser".to_string(), version: "0.1.0".to_string(),
        description: "Browser automation for JAMES".to_string(), module_type: ModuleType::Service,
        entry_point: "james_browser".to_string(),
        capabilities: vec!["browser.navigate".to_string(), "browser.screenshot".to_string(), "browser.extract".to_string()],
        dependencies: vec![], permissions: vec![],
        configuration_schema: None, default_config: None,
        author: Some("JAMES Project".to_string()), homepage: None, repository: None,
        license: "MIT".to_string(), tags: vec!["browser".to_string(), "web".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    for (id, name, desc) in [
        ("browser.navigate", "Browser Navigate", "Navigate to URL"),
        ("browser.screenshot", "Browser Screenshot", "Capture screenshot"),
        ("browser.extract", "Browser Extract", "Extract page content"),
    ] {
        registry.register(CapabilityDefinition {
            id: id.to_string(), name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("browser".to_string()),
            version: "1.0.0".to_string(), provider: "james.browser".to_string(),
            description: desc.to_string(), risk_level: RiskLevel::Medium,
            required_permissions: vec![], dependencies: vec![],
            input_schema: None, output_schema: None,
            execution_target: ExecutionTarget::Local,
            tags: vec!["browser".to_string()], deprecated: false, experimental: false,
        }, "james.browser".to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_module_host::ModuleManifestValidator;
    #[tokio::test]
    async fn test_browser_manifest() {
        let m = manifest();
        assert_eq!(m.id, "james.browser");
        assert!(ModuleManifestValidator::validate(&m).is_ok());
    }
}