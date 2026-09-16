//! Module loader - handles loading and unloading of modules

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{ModuleManifest, ModuleMeta, ModuleState, ModuleHostError};

/// Module loader configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleLoaderConfig {
    pub module_dirs: Vec<std::path::PathBuf>,
    pub load_timeout: std::time::Duration,
    pub allow_unsigned: bool,
}

impl Default for ModuleLoaderConfig {
    fn default() -> Self {
        Self {
            module_dirs: vec![std::path::PathBuf::from("modules")],
            load_timeout: std::time::Duration::from_secs(30),
            allow_unsigned: false,
        }
    }
}

/// Loaded module handle
pub struct LoadedModule {
    pub manifest: ModuleManifest,
    pub state: std::sync::Arc<tokio::sync::RwLock<crate::ModuleState>>,
    pub handle: ModuleHandle,
}

/// Module handle for interaction
#[derive(Clone)]
pub struct ModuleHandle {
    pub id: String,
    pub name: String,
    // In a real implementation, this would hold:
    // - Dynamic library handle (for native modules)
    // - WASM instance (for WASM modules)
    // - Process handle (for out-of-process modules)
    // - Communication channels
}

impl ModuleHandle {
    pub fn id(&self) -> &str {
        &self.id
    }
    
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Native module loader (shared library .so/.dll/.dylib)
pub struct NativeModuleLoader {
    config: ModuleLoaderConfig,
    loaded: Arc<RwLock<HashMap<String, LoadedModule>>>,
}

impl NativeModuleLoader {
    pub fn new(config: ModuleLoaderConfig) -> Self {
        Self {
            config,
            loaded: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    async fn find_module_file(&self, manifest: &crate::ModuleManifest) -> Result<PathBuf, ModuleHostError> {
        for dir in &self.config.module_dirs {
            // Try various extensions
            let extensions = if cfg!(target_os = "windows") {
                vec!["dll"]
            } else if cfg!(target_os = "macos") {
                vec!["dylib", "so"]
            } else {
                vec!["so"]
            };
            
            for ext in extensions {
                let path = dir.join(format!("{}.{}", manifest.entry_point, ext));
                if path.exists() {
                    return Ok(path);
                }
                
                // Also try with lib prefix (Unix convention)
                let path = dir.join(format!("lib{}.{}", manifest.entry_point, ext));
                if path.exists() {
                    return Ok(path);
                }
            }
        }
        
        Err(ModuleHostError::LoadFailed(
            format!("Module binary not found for {}", manifest.id)
        ))
    }
    
    pub async fn load(&self, manifest: &crate::ModuleManifest) -> Result<ModuleHandle, ModuleHostError> {
        let path = self.find_module_file(manifest).await?;
        
        info!("Loading native module {} from {:?}", manifest.id, path);
        
        // In a real implementation, we would:
        // 1. Load the shared library using libloading or similar
        // 2. Resolve entry points (init, start, stop, etc.)
        // 3. Call module_init()
        // 4. Return a handle
        
        // For now, return a mock handle
        let handle = ModuleHandle {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
        };
        
        Ok(handle)
    }
    
    pub async fn unload(&self, handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        info!("Unloading module {}", handle.id);
        // In real implementation: call module_shutdown(), dlclose(), etc.
        Ok(())
    }
    
    pub async fn health_check(&self, handle: &ModuleHandle) -> Result<bool, ModuleHostError> {
        // In real implementation: call module_health_check()
        Ok(true)
    }
}

/// WASM module loader (for sandboxed modules)
pub struct WasmModuleLoader {
    config: ModuleLoaderConfig,
    loaded: Arc<RwLock<HashMap<String, LoadedModule>>>,
}

impl WasmModuleLoader {
    pub fn new(config: ModuleLoaderConfig) -> Self {
        Self {
            config,
            loaded: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn load(&self, manifest: &crate::ModuleManifest) -> Result<ModuleHandle, ModuleHostError> {
        // WASM loading implementation
        Err(ModuleHostError::LoadFailed("WASM module loading not yet implemented".to_string()))
    }
    
    pub async fn unload(&self, handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        Ok(())
    }
    
    pub async fn health_check(&self, handle: &ModuleHandle) -> Result<bool, ModuleHostError> {
        Ok(false)
    }
}

/// Process-based module loader (for out-of-process modules)
pub struct ProcessModuleLoader {
    config: ModuleLoaderConfig,
}

impl ProcessModuleLoader {
    pub fn new(config: ModuleLoaderConfig) -> Self {
        Self { config }
    }
    
    pub async fn load(&self, manifest: &crate::ModuleManifest) -> Result<ModuleHandle, ModuleHostError> {
        // Spawn module as separate process
        Err(ModuleHostError::LoadFailed("Process module loading not yet implemented".to_string()))
    }
    
    pub async fn unload(&self, handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        Ok(())
    }
    
    pub async fn health_check(&self, handle: &ModuleHandle) -> Result<bool, ModuleHostError> {
        Ok(false)
    }
}

/// Main module loader - coordinates different backends
pub struct ModuleLoader {
    config: ModuleLoaderConfig,
    native: NativeModuleLoader,
    wasm: WasmModuleLoader,
    process: ProcessModuleLoader,
}

impl ModuleLoader {
    pub fn new(config: ModuleLoaderConfig) -> Self {
        let native = NativeModuleLoader::new(config.clone());
        let wasm = WasmModuleLoader::new(config.clone());
        let process = ProcessModuleLoader::new(config.clone());
        
        Self {
            config,
            native,
            wasm,
            process,
        }
    }
    
    /// Discover modules in configured directories
    pub async fn discover(&self) -> Result<Vec<ModuleManifest>, ModuleHostError> {
        let mut manifests = Vec::new();
        
        for dir in &self.config.module_dirs {
            if !dir.exists() {
                continue;
            }
            
            let entries = std::fs::read_dir(dir)?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Look for manifest.toml or manifest.json
                    let manifest_path = path.join("manifest.toml");
                    if manifest_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                            if let Ok(manifest) = toml::from_str::<ModuleManifest>(&content) {
                                manifests.push(manifest);
                            }
                        }
                    }
                    
                    let manifest_path = path.join("manifest.json");
                    if manifest_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                            if let Ok(manifest) = serde_json::from_str::<ModuleManifest>(&content) {
                                manifests.push(manifest);
                            }
                        }
                    }
                }
            }
        }
        
        Ok(manifests)
    }
    
    /// Load a module using the appropriate backend
    pub async fn load(&self, manifest: &ModuleManifest) -> Result<ModuleHandle, ModuleHostError> {
        // Determine backend based on manifest or config
        // For now, use native loader
        self.native.load(manifest).await
    }
    
    /// Unload a module
    pub async fn unload(&self, handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        self.native.unload(handle).await
    }
    
    /// Start a loaded module
    pub async fn start(&self, handle: &ModuleHandle, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        // In real implementation, call module_start()
        info!("Starting module {}", handle.id);
        Ok(())
    }

    /// Load and start a module in one call
    pub async fn load_and_start(&self, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        let handle = self.load(manifest).await?;
        self.start(&handle, manifest).await
    }
    
    /// Stop a running module (by manifest only)
    pub async fn stop_by_manifest(&self, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        info!("Stopping module {}", manifest.id);
        // In real implementation, find handle and call module_stop()
        Ok(())
    }

    /// Stop a running module
    pub async fn stop(&self, handle: &ModuleHandle, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        info!("Stopping module {}", handle.id);
        // In real implementation, call module_stop()
        Ok(())
    }

    /// Get all loaded modules
    pub async fn loaded_modules(&self) -> Vec<ModuleHandle> {
        self.native.loaded.read().await.values()
            .map(|m| m.handle.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_module_loader_discover() {
        let config = ModuleLoaderConfig::default();
        let loader = ModuleLoader::new(config);
        
        // Create a temp directory with a mock module
        let temp_dir = std::env::temp_dir().join("james-test-modules");
        std::fs::create_dir_all(&temp_dir).unwrap();
        
        let module_dir = temp_dir.join("test-module");
        std::fs::create_dir_all(&module_dir).unwrap();
        
        // Create a mock manifest
        let manifest = crate::ModuleManifest {
            id: "com.test.module".to_string(),
            name: "Test Module".to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            module_type: crate::ModuleType::Service,
            entry_point: "test_module".to_string(),
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
            platforms: vec!["windows".to_string()],
        };
        
        let manifest_path = module_dir.join("manifest.toml");
        std::fs::write(&manifest_path, toml::to_string(&manifest).unwrap()).unwrap();
        
        // Create a dummy binary
        #[cfg(target_os = "windows")]
        let binary_name = "test_module.dll";
        #[cfg(not(target_os = "windows"))]
        let binary_name = "libtest_module.so";
        
        std::fs::write(module_dir.join(binary_name), b"dummy").unwrap();
        
        // Test discovery with custom config
        let mut config = ModuleLoaderConfig::default();
        config.module_dirs = vec![temp_dir.clone()];
        
        let loader = ModuleLoader::new(config);
        let manifests = loader.discover().await.unwrap();
        
        assert!(!manifests.is_empty());
        assert_eq!(manifests[0].id, "com.test.module");
        
        // Cleanup
        std::fs::remove_dir_all(temp_dir).ok();
    }
}