//! James-Memory - Persistent memory system for JAMES
//!
//! Provides working, episodic, semantic, and procedural memory with JSON file persistence.

use std::path::Path;
use std::sync::Arc;
use anyhow::Result;
use james_capabilities::{CapabilityDefinition, CapabilityRegistry, ExecutionTarget, RiskLevel};
use james_events::{Event, EventBus};
use james_module_host::{ModuleManifest, ModuleType};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

/// Memory types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryType {
    Working,
    Episodic,
    Semantic,
    Procedural,
    Relationship,
}

/// Memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: serde_json::Value,
    pub importance: f32,
    pub access_count: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub tags: Vec<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
}

/// Memory query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub memory_types: Option<Vec<MemoryType>>,
    pub query_text: Option<String>,
    pub tags: Option<Vec<String>>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub limit: usize,
    pub min_importance: Option<f32>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
}

/// Memory module configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub database_path: String,
    pub max_working_memory: usize,
    pub embedding_dimensions: usize,
    pub auto_consolidate: bool,
    pub consolidation_interval_hours: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            database_path: ".james/memory.json".to_string(),
            max_working_memory: 100,
            embedding_dimensions: 384,
            auto_consolidate: true,
            consolidation_interval_hours: 24,
        }
    }
}

/// Memory module state
pub struct MemoryModule {
    config: MemoryConfig,
    event_bus: Arc<EventBus>,
    capability_registry: Arc<CapabilityRegistry>,
    running: Arc<RwLock<bool>>,
    entries: Arc<RwLock<Vec<MemoryEntry>>>,
    working_memory: Arc<RwLock<Vec<MemoryEntry>>>,
}

impl MemoryModule {
    pub fn new(
        config: MemoryConfig,
        event_bus: Arc<EventBus>,
        capability_registry: Arc<CapabilityRegistry>,
    ) -> Self {
        Self {
            config,
            event_bus,
            capability_registry,
            running: Arc::new(RwLock::new(false)),
            entries: Arc::new(RwLock::new(Vec::new())),
            working_memory: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;

        self.load().await?;
        self.rebuild_working_memory().await;

        info!("James-Memory started with database: {}", self.config.database_path);
        self.event_bus.publish(Event::new("module.memory.started", "james-memory")
            .with_payload(serde_json::json!({"database": self.config.database_path}))).await?;

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.save().await?;
        *self.running.write().await = false;
        info!("James-Memory stopped");
        self.event_bus.publish(Event::new("module.memory.stopped", "james-memory")).await?;
        Ok(())
    }

    async fn load(&self) -> Result<()> {
        let path = Path::new(&self.config.database_path);
        if path.exists() {
            let raw = tokio::fs::read_to_string(path).await?;
            if !raw.trim().is_empty() {
                let entries: Vec<MemoryEntry> = serde_json::from_str(&raw)?;
                *self.entries.write().await = entries;
            }
        }
        Ok(())
    }

    async fn save(&self) -> Result<()> {
        let path = Path::new(&self.config.database_path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let entries = self.entries.read().await.clone();
        let raw = serde_json::to_string_pretty(&entries)?;
        tokio::fs::write(path, raw).await?;
        Ok(())
    }

    async fn rebuild_working_memory(&self) {
        let entries = self.entries.read().await.clone();
        let mut wm = entries.into_iter()
            .filter(|e| e.memory_type == MemoryType::Working)
            .collect::<Vec<_>>();
        wm.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        wm.truncate(self.config.max_working_memory);
        *self.working_memory.write().await = wm;
    }

    /// Store a memory entry
    pub async fn store(&self, mut entry: MemoryEntry) -> Result<String> {
        if entry.id.is_empty() {
            entry.id = Uuid::now_v7().to_string();
        }
        entry.created_at = chrono::Utc::now();
        entry.updated_at = chrono::Utc::now();

        {
            let mut entries = self.entries.write().await;
            if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
                *existing = entry.clone();
            } else {
                entries.push(entry.clone());
            }
        }

        if entry.memory_type == MemoryType::Working {
            let mut wm = self.working_memory.write().await;
            wm.retain(|e| e.id != entry.id);
            wm.insert(0, entry.clone());
            if wm.len() > self.config.max_working_memory {
                wm.truncate(self.config.max_working_memory);
            }
        }

        self.save().await?;

        self.event_bus.publish(Event::new("memory.stored", "james-memory")
            .with_payload(serde_json::json!({"memory_id": entry.id, "type": entry.memory_type}))).await?;

        Ok(entry.id)
    }

    /// Query memories
    pub async fn query(&self, query: MemoryQuery) -> Result<Vec<MemoryEntry>> {
        let entries = self.entries.read().await;
        let mut results = entries.iter().filter(|e| {
            if let Some(types) = &query.memory_types {
                if !types.contains(&e.memory_type) {
                    return false;
                }
            }
            if let Some(tags) = &query.tags {
                if !tags.iter().all(|t| e.tags.contains(t)) {
                    return false;
                }
            }
            if let Some(session_id) = &query.session_id {
                if e.session_id.as_deref() != Some(session_id.as_str()) {
                    return false;
                }
            }
            if let Some(agent_id) = &query.agent_id {
                if e.agent_id.as_deref() != Some(agent_id.as_str()) {
                    return false;
                }
            }
            if let Some(since) = &query.since {
                if e.created_at < *since {
                    return false;
                }
            }
            if let Some(min_imp) = query.min_importance {
                if e.importance < min_imp {
                    return false;
                }
            }
            true
        }).cloned().collect::<Vec<_>>();

        results.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(query.limit);
        Ok(results)
    }

    /// Get working memory
    pub async fn get_working_memory(&self) -> Vec<MemoryEntry> {
        self.working_memory.read().await.clone()
    }

    /// Add to working memory
    pub async fn add_to_working(&self, content: String, metadata: serde_json::Value) -> Result<String> {
        let entry = MemoryEntry {
            id: String::new(),
            memory_type: MemoryType::Working,
            content,
            embedding: None,
            metadata,
            importance: 1.0,
            access_count: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            expires_at: None,
            tags: Vec::new(),
            session_id: None,
            agent_id: None,
        };
        self.store(entry).await
    }

    /// Consolidate memories (move important working to episodic/semantic)
    pub async fn consolidate(&self) -> Result<usize> {
        let mut count = 0;
        {
            let mut entries = self.entries.write().await;
            for entry in entries.iter_mut() {
                if entry.memory_type == MemoryType::Working && entry.importance > 0.7 {
                    entry.memory_type = MemoryType::Episodic;
                    entry.updated_at = chrono::Utc::now();
                    count += 1;
                }
            }
        }
        if count > 0 {
            self.save().await?;
            self.rebuild_working_memory().await;
            info!("Consolidated {} memories", count);
        }
        Ok(count)
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

/// Module manifest
pub fn manifest() -> ModuleManifest {
    ModuleManifest {
        id: "james.memory".to_string(),
        name: "James-Memory".to_string(),
        version: "0.1.0".to_string(),
        description: "Persistent memory system (working/episodic/semantic/procedural/relationship)".to_string(),
        module_type: ModuleType::Service,
        entry_point: "james_memory".to_string(),
        capabilities: vec![
            "memory.read".to_string(),
            "memory.working".to_string(),
            "memory.episodic".to_string(),
            "memory.semantic".to_string(),
            "memory.procedural".to_string(),
            "memory.relationship".to_string(),
        ],
        dependencies: vec![],
        permissions: vec![],
        configuration_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "database_path": {"type": "string", "default": ".james/memory.json"},
                "max_working_memory": {"type": "integer", "default": 100},
                "embedding_dimensions": {"type": "integer", "default": 384},
                "auto_consolidate": {"type": "boolean", "default": true},
                "consolidation_interval_hours": {"type": "integer", "default": 24}
            }
        })),
        default_config: Some(serde_json::json!({
            "database_path": ".james/memory.json",
            "max_working_memory": 100,
            "embedding_dimensions": 384,
            "auto_consolidate": true,
            "consolidation_interval_hours": 24
        })),
        author: Some("JAMES Project".to_string()),
        homepage: None,
        repository: None,
        license: "MIT".to_string(),
        tags: vec!["memory".to_string(), "ai".to_string(), "persistence".to_string()],
        min_core_version: "0.1.0".to_string(),
        platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
    }
}

/// Capabilities registration
pub async fn register_capabilities(registry: &CapabilityRegistry) -> Result<()> {
    let caps = vec![
        ("memory.read", "Read Memory", "Read matching memory entries"),
        ("memory.working", "Working Memory", "Short-term working memory"),
        ("memory.episodic", "Episodic Memory", "Event-based episodic memory"),
        ("memory.semantic", "Semantic Memory", "Fact-based semantic memory"),
        ("memory.procedural", "Procedural Memory", "Skill/procedure memory"),
        ("memory.relationship", "Relationship Memory", "Entity relationship memory"),
    ];

    for (id, name, desc) in caps {
        let cap = CapabilityDefinition {
            id: id.to_string(),
            name: name.to_string(),
            category: james_capabilities::CapabilityCategory::Custom("memory".to_string()),
            version: "1.0.0".to_string(),
            provider: "james.memory".to_string(),
            description: desc.to_string(),
            risk_level: RiskLevel::Low,
            required_permissions: vec![],
            dependencies: vec![],
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "object"},
                    "limit": {"type": "integer", "default": 10}
                }
            })),
            output_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "memories": {"type": "array", "items": {"type": "object"}}
                }
            })),
            execution_target: ExecutionTarget::Local,
            tags: vec!["memory".to_string()],
            deprecated: false,
            experimental: false,
        };
        registry.register(cap, "james.memory".to_string()).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use james_events::EventBus;
    use james_module_host::ModuleManifestValidator;

    #[tokio::test]
    async fn test_memory_manifest() {
        let manifest = manifest();
        assert_eq!(manifest.id, "james.memory");
        assert_eq!(manifest.capabilities.len(), 6);
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }
}