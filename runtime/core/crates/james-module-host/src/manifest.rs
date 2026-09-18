//! Module manifest and validation

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use thiserror::Error;
use uuid::Uuid;

/// Module manifest - describes a JAMES module
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleManifest {
    /// Unique module identifier (reverse domain style: com.example.module)
    pub id: String,
    
    /// Human-readable name
    pub name: String,
    
    /// Semantic version
    pub version: String,
    
    /// Human-readable description
    pub description: String,
    
    /// Module type classification
    #[serde(default)]
    pub module_type: ModuleType,
    
    /// Entry point (shared library path, WASM module, script, etc.)
    pub entry_point: String,
    
    /// Capabilities this module provides
    #[serde(default)]
    pub capabilities: Vec<String>,
    
    /// Dependencies on other modules
    #[serde(default)]
    pub dependencies: Vec<ModuleDependency>,
    
    /// Required permissions
    #[serde(default)]
    pub permissions: Vec<ModulePermission>,
    
    /// Optional configuration schema (JSON Schema)
    #[serde(default)]
    pub configuration_schema: Option<serde_json::Value>,
    
    /// Default configuration
    #[serde(default)]
    pub default_config: Option<serde_json::Value>,
    
    /// Module author
    #[serde(default)]
    pub author: Option<String>,
    
    /// Module homepage
    #[serde(default)]
    pub homepage: Option<String>,
    
    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,
    
    /// License
    #[serde(default = "default_license")]
    pub license: String,
    
    /// Keywords/tags
    #[serde(default)]
    pub tags: Vec<String>,
    
    /// Minimum JAMES core version required
    #[serde(default = "default_min_core_version")]
    pub min_core_version: String,
    
    /// Supported platforms
    #[serde(default)]
    pub platforms: Vec<String>,
}

/// Module type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ModuleType {
    /// Core system service
    Service,
    /// User-facing interface
    Interface,
    /// AI/ML provider
    AiProvider,
    /// Data storage
    Storage,
    /// Device integration
    Device,
    /// Automation/workflow
    Automation,
    /// Communication
    Communication,
    /// Security/Encryption
    Security,
    /// Utility/Helper
    Utility,
    /// Custom type
    Custom,
}

impl Default for ModuleType {
    fn default() -> Self {
        ModuleType::Service
    }
}

/// Module dependency
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleDependency {
    /// Dependency module ID
    pub name: String,
    
    /// Version constraint (semver)
    #[serde(default = "default_version_constraint")]
    pub version: String,
    
    /// Whether this dependency is optional
    #[serde(default)]
    pub optional: bool,
    
    /// Reason for dependency
    #[serde(default)]
    pub reason: Option<String>,
}

fn default_version_constraint() -> String {
    "*".to_string()
}

/// Module permission
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModulePermission {
    /// Permission identifier (e.g., "filesystem.read", "network.connect")
    pub permission: String,
    
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    
    /// Whether this permission is required (vs optional)
    #[serde(default = "default_true")]
    pub required: bool,
    
    /// Risk level
    #[serde(default = "default_risk_low")]
    pub risk_level: RiskLevel,
}

fn default_true() -> bool {
    true
}

fn default_risk_low() -> RiskLevel {
    RiskLevel::Low
}

/// Risk level for permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Module manifest validator
pub struct ModuleManifestValidator;

impl ModuleManifestValidator {
    /// Validate a module manifest
    pub fn validate(manifest: &ModuleManifest) -> Result<(), ManifestValidationError> {
        // Validate ID format (reverse domain style)
        if !Self::is_valid_module_id(&manifest.id) {
            return Err(ManifestValidationError::InvalidId(
                "Module ID must be in reverse domain format (e.g., com.example.module)".to_string()
            ));
        }
        
        // Validate version (semver)
        if !Self::is_valid_version(&manifest.version) {
            return Err(ManifestValidationError::InvalidVersion(
                "Version must be valid semver".to_string()
            ));
        }
        
        // Validate entry point
        if manifest.entry_point.trim().is_empty() {
            return Err(ManifestValidationError::InvalidEntryPoint(
                "Entry point cannot be empty".to_string()
            ));
        }
        
        // Validate capabilities format
        for cap in &manifest.capabilities {
            if !Self::is_valid_capability_id(cap) {
                return Err(ManifestValidationError::InvalidCapability(
                    format!("Invalid capability ID: {}", cap)
                ));
            }
        }
        
        // Validate dependencies
        let mut seen = std::collections::HashSet::new();
        for dep in &manifest.dependencies {
            if !Self::is_valid_module_id(&dep.name) {
                return Err(ManifestValidationError::InvalidDependency(
                    format!("Invalid dependency ID: {}", dep.name)
                ));
            }
            if !seen.insert(dep.name.clone()) {
                return Err(ManifestValidationError::DuplicateDependency(
                    format!("Duplicate dependency: {}", dep.name)
                ));
            }
        }
        
        // Validate permissions
        for perm in &manifest.permissions {
            if perm.permission.trim().is_empty() {
                return Err(ManifestValidationError::InvalidPermission(
                    "Permission ID cannot be empty".to_string()
                ));
            }
        }
        
        // Validate version constraints (only when dependencies exist)
        if !manifest.dependencies.is_empty() {
            let constraints = manifest
                .dependencies
                .iter()
                .map(|d| d.version.as_str())
                .collect::<Vec<_>>()
                .join(",");
            if let Err(e) = semver::VersionReq::parse(&constraints) {
                return Err(ManifestValidationError::InvalidVersionConstraint(e.to_string()));
            }
        }
        
        Ok(())
    }
    
    /// Validate module ID format (reverse domain style)
    fn is_valid_module_id(id: &str) -> bool {
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() < 2 {
            return false;
        }
        for part in parts {
            if part.is_empty() {
                return false;
            }
            // Must start with letter, contain only alphanumeric and hyphen
            let chars: Vec<char> = part.chars().collect();
            if !chars[0].is_ascii_alphabetic() {
                return false;
            }
            for c in chars {
                if !c.is_ascii_alphanumeric() && c != '-' {
                    return false;
                }
            }
        }
        true
    }
    
    /// Validate capability ID format
    fn is_valid_capability_id(id: &str) -> bool {
        let parts: Vec<&str> = id.split('.').collect();
        if parts.len() < 2 {
            return false;
        }
        for part in parts {
            if part.is_empty() {
                return false;
            }
            for c in part.chars() {
                if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
                    return false;
                }
            }
        }
        true
    }
    
    /// Validate semantic version
    fn is_valid_version(version: &str) -> bool {
        semver::Version::parse(version).is_ok()
    }
}

/// Manifest validation errors
#[derive(Debug, Error)]
pub enum ManifestValidationError {
    #[error("Invalid module ID: {0}")]
    InvalidId(String),
    
    #[error("Invalid version: {0}")]
    InvalidVersion(String),
    
    #[error("Invalid entry point: {0}")]
    InvalidEntryPoint(String),
    
    #[error("Invalid capability ID: {0}")]
    InvalidCapability(String),
    
    #[error("Invalid dependency: {0}")]
    InvalidDependency(String),
    
    #[error("Duplicate dependency: {0}")]
    DuplicateDependency(String),
    
    #[error("Invalid permission: {0}")]
    InvalidPermission(String),
    
    #[error("Invalid version constraint: {0}")]
    InvalidVersionConstraint(String),
    
    #[error("Missing required field: {0}")]
    MissingField(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_manifest() {
        let manifest = ModuleManifest {
            id: "com.example.test".to_string(),
            name: "Test Module".to_string(),
            version: "1.0.0".to_string(),
            description: "A test module".to_string(),
            module_type: ModuleType::Service,
            entry_point: "lib.so".to_string(),
            capabilities: vec!["test.capability".to_string()],
            dependencies: vec![],
            permissions: vec![],
            configuration_schema: None,
            default_config: None,
            author: Some("Test Author".to_string()),
            homepage: None,
            repository: None,
            license: "MIT".to_string(),
            tags: vec![],
            min_core_version: "0.1.0".to_string(),
            platforms: vec!["windows".to_string(), "linux".to_string()],
        };
        
        assert!(ModuleManifestValidator::validate(&manifest).is_ok());
    }
    
    #[test]
    fn test_invalid_id_format() {
        let mut manifest = valid_test_manifest();
        manifest.id = "invalid_id".to_string();
        assert!(ModuleManifestValidator::validate(&manifest).is_err());
    }
    
    #[test]
    fn test_invalid_version() {
        let mut manifest = valid_test_manifest();
        manifest.version = "not-a-version".to_string();
        assert!(ModuleManifestValidator::validate(&manifest).is_err());
    }
    
    #[test]
    fn test_duplicate_dependencies() {
        let mut manifest = valid_test_manifest();
        manifest.dependencies = vec![
            ModuleDependency { name: "dep1".to_string(), version: "1.0.0".to_string(), optional: false, reason: None },
            ModuleDependency { name: "dep1".to_string(), version: "2.0.0".to_string(), optional: false, reason: None },
        ];
        assert!(ModuleManifestValidator::validate(&manifest).is_err());
    }
    
    fn valid_test_manifest() -> ModuleManifest {
        ModuleManifest {
            id: "com.example.test".to_string(),
            name: "Test Module".to_string(),
            version: "1.0.0".to_string(),
            description: "A test module".to_string(),
            module_type: ModuleType::Service,
            entry_point: "lib.so".to_string(),
            capabilities: vec![],
            dependencies: vec![],
            permissions: vec![],
            configuration_schema: None,
            default_config: None,
            author: None,
            homepage: None,
            repository: None,
            license: "MIT".to_string(),
            tags: vec![],
            min_core_version: "0.1.0".to_string(),
            platforms: vec![],
        }
    }
}

fn default_license() -> String {
    "MIT".to_string()
}

fn default_min_core_version() -> String {
    "0.1.0".to_string()
}

fn default_version() -> String {
    "1.0.0".to_string()
}