//! Module loader - handles loading and unloading of modules

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use crate::abi::{
    NativeInitFn, NativeLifecycleFn, NativeModuleAbi, JAMES_MODULE_ABI_VERSION,
};
use crate::{ModuleHostError, ModuleManifest, ModuleState};

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
    pub state: std::sync::Arc<tokio::sync::RwLock<ModuleState>>,
    pub handle: ModuleHandle,
    native: Option<LoadedNativeState>,
}

/// Keep the dynamic library alive and hold the resolved ABI.
struct LoadedNativeState {
    _library: libloading::Library,
    abi: NativeModuleAbi,
}

/// Module handle for interaction
#[derive(Clone)]
pub struct ModuleHandle {
    pub id: String,
    pub name: String,
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

    /// Locate the shared library backing a manifest entry point.
    async fn find_module_file(&self, manifest: &ModuleManifest) -> Result<PathBuf, ModuleHostError> {
        for dir in &self.config.module_dirs {
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

    async fn find_loaded(&self, id: &str) -> Result<NativeModuleAbi, ModuleHostError> {
        let loaded = self.loaded.read().await;
        let entry = loaded
            .get(id)
            .ok_or_else(|| ModuleHostError::NotFound(id.to_string()))?;
        entry
            .native
            .as_ref()
            .map(|n| n.abi)
            .ok_or_else(|| ModuleHostError::InvalidState(format!("{} has no native payload", id)))
    }

    /// Resolve the exported symbols from an opened library.
    unsafe fn resolve_abi(library: &libloading::Library) -> Result<NativeModuleAbi, ModuleHostError> {
        let version: libloading::Symbol<unsafe extern "C" fn() -> u32> = library
            .get(b"james_module_abi_version\0")
            .map_err(|_| ModuleHostError::LoadFailed("missing james_module_abi_version".into()))?;
        let reported = version();

        if reported != JAMES_MODULE_ABI_VERSION {
            return Err(ModuleHostError::LoadFailed(format!(
                "unsupported ABI version {} (host expects {})",
                reported, JAMES_MODULE_ABI_VERSION
            )));
        }

        let init: Option<NativeInitFn> = match library.get(b"james_module_init\0") {
            Ok(sym) => Some(*sym),
            Err(_) => None,
        };
        let start: Option<NativeLifecycleFn> = match library.get(b"james_module_start\0") {
            Ok(sym) => Some(*sym),
            Err(_) => None,
        };
        let stop: Option<NativeLifecycleFn> = match library.get(b"james_module_stop\0") {
            Ok(sym) => Some(*sym),
            Err(_) => None,
        };
        let health: Option<NativeLifecycleFn> = match library.get(b"james_module_health_check\0") {
            Ok(sym) => Some(*sym),
            Err(_) => None,
        };

        if init.is_none() {
            return Err(ModuleHostError::LoadFailed("missing james_module_init".into()));
        }

        Ok(NativeModuleAbi { init, start, stop, health })
    }

    pub async fn load(&self, manifest: &ModuleManifest) -> Result<ModuleHandle, ModuleHostError> {
        let path = self.find_module_file(manifest).await?;

        info!("Loading native module {} from {:?}", manifest.id, path);

        let library = unsafe { libloading::Library::new(&path) }
            .map_err(|e| ModuleHostError::LoadFailed(format!("dlopen failed: {}", e)))?;

        let abi = unsafe { Self::resolve_abi(&library) }?;

        let manifest_json = serde_json::to_string(manifest)
            .map_err(|e| ModuleHostError::LoadFailed(format!("manifest serialization failed: {}", e)))?;
        let init_cstr = crate::abi::ManifestCString::new(&manifest_json)
            .map_err(|e| ModuleHostError::LoadFailed(e.to_string()))?;

        if let Some(init) = abi.init {
            let status = unsafe { init(init_cstr.as_ptr()) };
            if status != 0 {
                return Err(ModuleHostError::LoadFailed(format!(
                    "james_module_init returned status {}",
                    status
                )));
            }
        }

        let handle = ModuleHandle {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
        };

        let loaded = LoadedModule {
            manifest: manifest.clone(),
            state: Arc::new(RwLock::new(ModuleState::Registered)),
            handle: handle.clone(),
            native: Some(LoadedNativeState { _library: library, abi }),
        };

        self.loaded.write().await.insert(handle.id.clone(), loaded);

        Ok(handle)
    }

    pub async fn unload(&self, handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        info!("Unloading module {}", handle.id);
        if let Ok(abi) = self.find_loaded(&handle.id).await {
            if let Some(stop) = abi.stop {
                unsafe { stop() };
            }
        }
        self.loaded.write().await.remove(&handle.id);
        Ok(())
    }

    pub async fn health_check(&self, handle: &ModuleHandle) -> Result<bool, ModuleHostError> {
        let abi = self.find_loaded(&handle.id).await?;
        match abi.health {
            Some(health) => Ok(unsafe { health() } == 1),
            None => Ok(true),
        }
    }

    /// Invoke james_module_start on a loaded module.
    pub async fn start(&self, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        let abi = self.find_loaded(&manifest.id).await?;
        if let Some(start) = abi.start {
            let status = unsafe { start() };
            if status != 0 {
                return Err(ModuleHostError::LoadFailed(format!(
                    "james_module_start returned status {}",
                    status
                )));
            }
        }
        if let Some(loaded) = self.loaded.write().await.get_mut(&manifest.id) {
            *loaded.state.write().await = ModuleState::Running;
        }
        Ok(())
    }

    /// Invoke james_module_stop on a loaded module.
    pub async fn stop(&self, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        let abi = self.find_loaded(&manifest.id).await?;
        if let Some(stop) = abi.stop {
            let status = unsafe { stop() };
            if status != 0 {
                return Err(ModuleHostError::LoadFailed(format!(
                    "james_module_stop returned status {}",
                    status
                )));
            }
        }
        if let Some(loaded) = self.loaded.write().await.get_mut(&manifest.id) {
            *loaded.state.write().await = ModuleState::Stopped;
        }
        Ok(())
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

    pub async fn load(&self, _manifest: &ModuleManifest) -> Result<ModuleHandle, ModuleHostError> {
        Err(ModuleHostError::LoadFailed(
            "WASM module loading not yet implemented".to_string()
        ))
    }

    #[allow(clippy::unused_async)]
    pub async fn unload(&self, _handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        Ok(())
    }

    #[allow(clippy::unused_async)]
    pub async fn health_check(&self, _handle: &ModuleHandle) -> Result<bool, ModuleHostError> {
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

    pub async fn load(&self, _manifest: &ModuleManifest) -> Result<ModuleHandle, ModuleHostError> {
        Err(ModuleHostError::LoadFailed(
            "Process module loading not yet implemented".to_string()
        ))
    }

    #[allow(clippy::unused_async)]
    pub async fn unload(&self, _handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        Ok(())
    }

    #[allow(clippy::unused_async)]
    pub async fn health_check(&self, _handle: &ModuleHandle) -> Result<bool, ModuleHostError> {
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
        // All current module types map to the native (shared-library) backend.
        // WASM/process backends are reserved for future sandboxed deployments.
        let _ = &self.wasm;
        let _ = &self.process;
        self.native.load(manifest).await
    }

    /// Unload a module
    pub async fn unload(&self, handle: &ModuleHandle) -> Result<(), ModuleHostError> {
        self.native.unload(handle).await
    }

    /// Start a loaded module
    pub async fn start(&self, _handle: &ModuleHandle, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        self.native.start(manifest).await
    }

    /// Load and start a module in one call
    pub async fn load_and_start(&self, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        let handle = self.load(manifest).await?;
        self.start(&handle, manifest).await
    }

    /// Stop a running module (by manifest only)
    pub async fn stop_by_manifest(&self, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        self.native.stop(manifest).await
    }

    /// Stop a running module
    pub async fn stop(&self, _handle: &ModuleHandle, manifest: &ModuleManifest) -> Result<(), ModuleHostError> {
        self.native.stop(manifest).await
    }

    /// Health check a loaded module
    pub async fn health_check(&self, handle: &ModuleHandle) -> Result<bool, ModuleHostError> {
        self.native.health_check(handle).await
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
    use std::time::Duration;

    use super::*;

    fn test_manifest(id: &str, entry_point: &str) -> ModuleManifest {
        ModuleManifest {
            id: id.to_string(),
            name: id.to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            module_type: crate::ModuleType::Service,
            entry_point: entry_point.to_string(),
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
            platforms: vec!["windows".to_string(), "linux".to_string(), "macos".to_string()],
        }
    }

    #[tokio::test]
    async fn test_module_loader_discover() {
        let temp_dir = std::env::temp_dir().join("james-test-modules");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let module_dir = temp_dir.join("test-module");
        std::fs::create_dir_all(&module_dir).unwrap();

        let manifest = test_manifest("com.test.module", "test_module");

        let manifest_path = module_dir.join("manifest.toml");
        std::fs::write(&manifest_path, toml::to_string(&manifest).unwrap()).unwrap();

        let mut config = ModuleLoaderConfig::default();
        config.module_dirs = vec![temp_dir.clone()];

        let loader = ModuleLoader::new(config);
        let manifests = loader.discover().await.unwrap();

        assert!(!manifests.is_empty());
        assert_eq!(manifests[0].id, "com.test.module");

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    /// Locate the compiled fixture cdylib next to the running test binary.
    fn fixture_library_path() -> Option<PathBuf> {
        let name_stem = "james_module_host_fixture";

        let extensions: &[&str] = if cfg!(target_os = "windows") {
            &["dll"]
        } else if cfg!(target_os = "macos") {
            &["dylib", "so"]
        } else {
            &["so"]
        };

        // The fixture crate is a dev-dependency, so its cdylib artifact is
        // placed in the same `deps` directory as this test's executable.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.starts_with(name_stem)
                            || name.starts_with(&format!("{}-", name_stem))
                            || name.starts_with(&format!("lib{}", name_stem))
                        {
                            if extensions.iter().any(|e| name.ends_with(e)) {
                                return Some(p);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    #[tokio::test]
    async fn test_native_module_lifecycle_via_dlopen() {
        let Some(lib_path) = fixture_library_path() else {
            eprintln!("fixture cdylib not found; skipping dynamic loading test");
            return;
        };

        let temp_dir = std::env::temp_dir().join("james-native-fixture");
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Copy fixture into temp dir under a stable name so find_module_file works.
        let ext = lib_path.extension().and_then(|e| e.to_str()).unwrap_or("dll");
        let dest = temp_dir.join(format!("james_module_host_fixture.{}", ext));
        std::fs::copy(&lib_path, &dest).unwrap();

        let config = ModuleLoaderConfig {
            module_dirs: vec![temp_dir.clone()],
            load_timeout: Duration::from_secs(30),
            allow_unsigned: true,
        };
        let loader = ModuleLoader::new(config);

        let manifest = test_manifest("com.james.fixture", "james_module_host_fixture");
        let handle = loader.load(&manifest).await.expect("load should succeed");
        assert_eq!(handle.id(), "com.james.fixture");

        // Health before start: fixture reports 0 until started.
        assert!(!loader.health_check(&handle).await.unwrap());

        loader.start(&handle, &manifest).await.expect("start should succeed");
        assert!(loader.health_check(&handle).await.unwrap());

        loader.stop_by_manifest(&manifest).await.expect("stop should succeed");
        assert!(!loader.health_check(&handle).await.unwrap());

        loader.unload(&handle).await.expect("unload should succeed");
        assert!(loader.loaded_modules().await.is_empty());

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[tokio::test]
    async fn test_native_module_load_missing_binary_fails() {
        let config = ModuleLoaderConfig::default();
        let loader = ModuleLoader::new(config);

        let manifest = test_manifest("com.james.nonexistent", "does_not_exist");
        let result = loader.load(&manifest).await;
        assert!(result.is_err());
    }
}