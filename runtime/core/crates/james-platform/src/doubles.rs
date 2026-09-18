//! In-memory test doubles for every platform port.
//!
//! Test contract (platform-abstraction.md §Testvertrag): each port has a
//! test double, tests for success / missing resource / PermissionDenied /
//! Unsupported — with no dependency on real hardware in core tests.

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::*;

/// In-memory file system test double. Paths are treated as-is (no OS access).
#[derive(Default)]
pub struct MemoryFileSystem {
    files: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    dirs: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl MemoryFileSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed_file(&self, path: &str, contents: &str) {
        self.files.write().insert(path.to_string(), contents.as_bytes().to_vec());
        if let Some(parent) = PathBuf::from(path).parent() {
            self.dirs.write().insert(parent.to_string_lossy().to_string());
        }
    }

    fn list_dir(&self, path: &str) -> Vec<FileEntry> {
        let mut entries = Vec::new();
        for file in self.files.read().keys() {
            if let Some(rel) = file.strip_prefix(&format!("{path}/")) {
                if !rel.contains('/') {
                    entries.push(FileEntry {
                        path: file.clone(),
                        name: rel.to_string(),
                        is_dir: false,
                        size_bytes: Some(self.files.read()[file].len() as u64),
                        modified_at: None,
                    });
                }
            }
        }
        for dir in self.dirs.read().iter() {
            if dir == path {
                continue;
            }
            if let Some(rel) = dir.strip_prefix(&format!("{path}/")) {
                if !rel.contains('/') {
                    entries.push(FileEntry {
                        path: dir.clone(),
                        name: rel.to_string(),
                        is_dir: true,
                        size_bytes: None,
                        modified_at: None,
                    });
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }
}

#[async_trait]
impl FileSystemPort for MemoryFileSystem {
    async fn read(&self, path: &str) -> Result<Vec<u8>, PlatformError> {
        self.files
            .read()
            .get(path)
            .cloned()
            .ok_or_else(|| PlatformError::NotFound(path.to_string()))
    }

    async fn read_text(&self, path: &str) -> Result<String, PlatformError> {
        let data = self.read(path).await?;
        String::from_utf8(data).map_err(|e| PlatformError::InvalidInput(e.to_string()))
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<(), PlatformError> {
        self.files.write().insert(path.to_string(), data.to_vec());
        if let Some(parent) = PathBuf::from(path).parent() {
            self.dirs.write().insert(parent.to_string_lossy().to_string());
        }
        Ok(())
    }

    async fn write_text(&self, path: &str, contents: &str) -> Result<(), PlatformError> {
        self.write(path, contents.as_bytes()).await
    }

    async fn append_text(&self, path: &str, contents: &str) -> Result<(), PlatformError> {
        let mut guard = self.files.write();
        let entry = guard.entry(path.to_string()).or_default();
        entry.extend_from_slice(contents.as_bytes());
        drop(guard);
        if let Some(parent) = PathBuf::from(path).parent() {
            self.dirs.write().insert(parent.to_string_lossy().to_string());
        }
        Ok(())
    }

    async fn exists(&self, path: &str) -> bool {
        self.files.read().contains_key(path) || self.dirs.read().contains(path)
    }

    async fn is_dir(&self, path: &str) -> bool {
        self.dirs.read().contains(path)
    }

    async fn create_dir_all(&self, path: &str) -> Result<(), PlatformError> {
        self.dirs.write().insert(path.to_string());
        Ok(())
    }

    async fn remove(&self, path: &str) -> Result<(), PlatformError> {
        if self.files.write().remove(path).is_some() {
            return Ok(());
        }
        if self.dirs.write().remove(path) {
            return Ok(());
        }
        Err(PlatformError::NotFound(path.to_string()))
    }

    async fn list(&self, path: &str) -> Result<DirectoryListing, PlatformError> {
        if !self.dirs.read().contains(path) && !self.files.read().contains_key(path) {
            return Err(PlatformError::NotFound(path.to_string()));
        }
        Ok(DirectoryListing {
            path: path.to_string(),
            entries: self.list_dir(path),
        })
    }

    async fn stat(&self, path: &str) -> Result<FileEntry, PlatformError> {
        if let Some(data) = self.files.read().get(path) {
            return Ok(FileEntry {
                path: path.to_string(),
                name: PathBuf::from(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string()),
                is_dir: false,
                size_bytes: Some(data.len() as u64),
                modified_at: None,
            });
        }
        if self.dirs.read().contains(path) {
            return Ok(FileEntry {
                path: path.to_string(),
                name: path.to_string(),
                is_dir: true,
                size_bytes: None,
                modified_at: None,
            });
        }
        Err(PlatformError::NotFound(path.to_string()))
    }

    async fn rename(&self, from: &str, to: &str) -> Result<(), PlatformError> {
        let data = self
            .files
            .write()
            .remove(from)
            .ok_or_else(|| PlatformError::NotFound(from.to_string()))?;
        self.files.write().insert(to.to_string(), data);
        Ok(())
    }
}

/// In-memory process test double. Empty by default (no OS processes).
#[derive(Default)]
pub struct MemoryProcesses;

#[async_trait]
impl ProcessPort for MemoryProcesses {
    async fn spawn(&self, request: SpawnRequest) -> Result<SpawnResult, PlatformError> {
        // Fake echo behavior so contract tests can assert shape.
        if request.program.is_empty() {
            return Err(PlatformError::InvalidInput("program must not be empty".into()));
        }
        Ok(SpawnResult {
            pid: 1,
            stdout: Some(format!("{} {}", request.program, request.args.join(" "))),
            stderr: None,
            exit_code: Some(0),
            timed_out: false,
        })
    }

    async fn list(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        Ok(Vec::new())
    }

    async fn get(&self, pid: u32) -> Result<ProcessInfo, PlatformError> {
        Err(PlatformError::NotFound(format!("pid {pid}")))
    }

    async fn kill(&self, _pid: u32) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("no processes in memory double".into()))
    }

    async fn wait(&self, _pid: u32, _timeout_ms: u64) -> Result<Option<i32>, PlatformError> {
        Ok(None)
    }
}

/// Static system-info test double.
#[derive(Clone)]
pub struct MemorySystemInfo(pub SystemInfo);

impl Default for MemorySystemInfo {
    fn default() -> Self {
        Self(SystemInfo {
            os_name: "in-memory".into(),
            os_version: "test".into(),
            hostname: "localhost".into(),
            arch: "x86_64".into(),
            cpu_count: Some(1),
            total_memory_bytes: Some(1 << 30),
            free_memory_bytes: Some(1 << 30),
            total_disk_bytes: Some(1 << 30),
            free_disk_bytes: Some(1 << 30),
        })
    }
}

#[async_trait]
impl SystemInfoPort for MemorySystemInfo {
    async fn system_info(&self) -> Result<SystemInfo, PlatformError> {
        Ok(self.0.clone())
    }

    async fn uptime_secs(&self) -> Result<u64, PlatformError> {
        Ok(0)
    }
}

/// Static battery/power test double.
#[derive(Clone)]
pub struct MemoryPower(pub PowerStatus);

#[async_trait]
impl PowerPort for MemoryPower {
    async fn status(&self) -> Result<PowerStatus, PlatformError> {
        Ok(self.0.clone())
    }
}

/// Data-directory test double rooted at a given base directory.
#[derive(Clone)]
pub struct MemoryDataDirectories {
    pub root: PathBuf,
}

#[async_trait]
impl DataDirectoryPort for MemoryDataDirectories {
    async fn root(&self) -> Result<PathBuf, PlatformError> {
        Ok(self.root.clone())
    }

    async fn area(&self, area: DataArea) -> Result<DataDirectory, PlatformError> {
        Ok(DataDirectory {
            area,
            path: self.root.join(area.as_str()).to_string_lossy().to_string(),
        })
    }

    async fn ensure(&self, area: DataArea) -> Result<PathBuf, PlatformError> {
        let path = self.root.join(area.as_str());
        Ok(path)
    }
}

/// Unsupported adapter: every port returns Unsupported, proving the core
/// never mistakes "no implementation" for success.
pub struct UnsupportedPlatform;

#[async_trait]
impl FeatureDetectionPort for UnsupportedPlatform {
    async fn features(&self) -> Result<Vec<FeatureStatus>, PlatformError> {
        Ok(Vec::new())
    }

    async fn descriptor(&self) -> Result<PlatformDescriptor, PlatformError> {
        Ok(PlatformDescriptor {
            name: "unsupported".into(),
            os: "unknown".into(),
            version: "0.0.0".into(),
            supports: Vec::new(),
        })
    }
}

#[async_trait]
impl FileSystemPort for UnsupportedPlatform {
    async fn read(&self, _path: &str) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("read".into()))
    }
    async fn read_text(&self, _path: &str) -> Result<String, PlatformError> {
        Err(PlatformError::Unsupported("read_text".into()))
    }
    async fn write(&self, _path: &str, _data: &[u8]) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("write".into()))
    }
    async fn write_text(&self, _path: &str, _contents: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("write_text".into()))
    }
    async fn append_text(&self, _path: &str, _contents: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("append_text".into()))
    }
    async fn exists(&self, _path: &str) -> bool {
        false
    }
    async fn is_dir(&self, _path: &str) -> bool {
        false
    }
    async fn create_dir_all(&self, _path: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("create_dir_all".into()))
    }
    async fn remove(&self, _path: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("remove".into()))
    }
    async fn list(&self, _path: &str) -> Result<DirectoryListing, PlatformError> {
        Err(PlatformError::Unsupported("list".into()))
    }
    async fn stat(&self, _path: &str) -> Result<FileEntry, PlatformError> {
        Err(PlatformError::Unsupported("stat".into()))
    }
    async fn rename(&self, _from: &str, _to: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("rename".into()))
    }
}

#[async_trait]
impl ProcessPort for UnsupportedPlatform {
    async fn spawn(&self, _request: SpawnRequest) -> Result<SpawnResult, PlatformError> {
        Err(PlatformError::Unsupported("spawn".into()))
    }
    async fn list(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        Err(PlatformError::Unsupported("list".into()))
    }
    async fn get(&self, _pid: u32) -> Result<ProcessInfo, PlatformError> {
        Err(PlatformError::Unsupported("get".into()))
    }
    async fn kill(&self, _pid: u32) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("kill".into()))
    }
    async fn wait(&self, _pid: u32, _timeout_ms: u64) -> Result<Option<i32>, PlatformError> {
        Err(PlatformError::Unsupported("wait".into()))
    }
}

#[async_trait]
impl ApplicationPort for UnsupportedPlatform {
    async fn launch(&self, _executable: &str, _args: Vec<String>) -> Result<u32, PlatformError> {
        Err(PlatformError::Unsupported("launch".into()))
    }
    async fn open_with_default(&self, _path_or_url: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("open_with_default".into()))
    }
    async fn is_running(&self, _app_id: &str) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("is_running".into()))
    }
}

#[async_trait]
impl NetworkPort for UnsupportedPlatform {
    async fn interfaces(&self) -> Result<Vec<NetworkInterface>, PlatformError> {
        Err(PlatformError::Unsupported("interfaces".into()))
    }
    async fn is_online(&self) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("is_online".into()))
    }
    async fn reachable(&self, _host: &str, _timeout_ms: u64) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("reachable".into()))
    }
}

#[async_trait]
impl DevicePort for UnsupportedPlatform {
    async fn devices(&self, _category: Option<DeviceCategory>) -> Result<Vec<DeviceInfo>, PlatformError> {
        Err(PlatformError::Unsupported("devices".into()))
    }
    async fn present(&self, _device_id: &str) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("present".into()))
    }
}

#[async_trait]
impl AudioPort for UnsupportedPlatform {
    async fn play(&self, _audio: &[u8], _format: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("play".into()))
    }
    async fn stop(&self) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("stop".into()))
    }
    async fn is_playing(&self) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("is_playing".into()))
    }
}

#[async_trait]
impl CameraPort for UnsupportedPlatform {
    async fn capture(&self, _device_id: &str) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("capture".into()))
    }
}

#[async_trait]
impl DisplayPort for UnsupportedPlatform {
    async fn screenshot(&self) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("screenshot".into()))
    }
    async fn resolution(&self) -> Result<(u32, u32), PlatformError> {
        Err(PlatformError::Unsupported("resolution".into()))
    }
}

#[async_trait]
impl ComputePort for UnsupportedPlatform {
    async fn device_name(&self) -> Result<String, PlatformError> {
        Err(PlatformError::Unsupported("device_name".into()))
    }
    async fn compute(&self, _kernel: &str, _data: Vec<u8>) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("compute".into()))
    }
    async fn is_compute_available(&self) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("is_compute_available".into()))
    }
}

#[async_trait]
impl NotificationPort for UnsupportedPlatform {
    async fn notify(&self, _request: NotificationRequest) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("notify".into()))
    }
}

#[async_trait]
impl SystemInfoPort for UnsupportedPlatform {
    async fn system_info(&self) -> Result<SystemInfo, PlatformError> {
        Err(PlatformError::Unsupported("system_info".into()))
    }
    async fn uptime_secs(&self) -> Result<u64, PlatformError> {
        Err(PlatformError::Unsupported("uptime_secs".into()))
    }
}

#[async_trait]
impl PowerPort for UnsupportedPlatform {
    async fn status(&self) -> Result<PowerStatus, PlatformError> {
        Err(PlatformError::Unsupported("status".into()))
    }
}

#[async_trait]
impl PlatformSecurityPort for UnsupportedPlatform {
    async fn seal_secret(&self, _name: &str, _secret: &[u8]) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("seal_secret".into()))
    }
    async fn unseal_secret(&self, _name: &str) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("unseal_secret".into()))
    }
    async fn delete_secret(&self, _name: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("delete_secret".into()))
    }
}

#[async_trait]
impl DataDirectoryPort for UnsupportedPlatform {
    async fn root(&self) -> Result<PathBuf, PlatformError> {
        Err(PlatformError::Unsupported("root".into()))
    }
    async fn area(&self, _area: DataArea) -> Result<DataDirectory, PlatformError> {
        Err(PlatformError::Unsupported("area".into()))
    }
    async fn ensure(&self, _area: DataArea) -> Result<PathBuf, PlatformError> {
        Err(PlatformError::Unsupported("ensure".into()))
    }
}

// Silence unused-macro warning: macro not used, but documented for future ports.
// In-memory aggregate platform for services/tests. Combines all memory doubles
// and implements every port + the `Platform` aggregation marker.
#[derive(Default)]
pub struct MemoryPlatform {
    pub fs: MemoryFileSystem,
    pub processes: MemoryProcesses,
    pub system: MemorySystemInfo,
    pub power: MemoryPower,
    pub data: MemoryDataDirectories,
}

impl MemoryPlatform {
    pub fn with_root(root: PathBuf) -> Self {
        Self {
            data: MemoryDataDirectories { root },
            ..Self::default()
        }
    }
}

impl Default for MemoryDataDirectories {
    fn default() -> Self {
        Self {
            root: PathBuf::from(".james-test"),
        }
    }
}

impl Default for MemoryPower {
    fn default() -> Self {
        Self(PowerStatus {
            on_ac: true,
            on_battery: false,
            battery_percent: Some(100),
            battery_charging: None,
            remaining_secs: None,
        })
    }
}

#[async_trait]
impl FeatureDetectionPort for MemoryPlatform {
    async fn features(&self) -> Result<Vec<FeatureStatus>, PlatformError> {
        Ok(vec![
            FeatureStatus {
                feature: "filesystem".into(),
                supported: true,
                details: "in-memory fs".into(),
            },
            FeatureStatus {
                feature: "process".into(),
                supported: true,
                details: "in-memory echo".into(),
            },
            FeatureStatus {
                feature: "system_info".into(),
                supported: true,
                details: "static".into(),
            },
        ])
    }

    async fn descriptor(&self) -> Result<PlatformDescriptor, PlatformError> {
        Ok(PlatformDescriptor {
            name: "memory-platform".into(),
            os: "in-memory".into(),
            version: "0.1.0".into(),
            supports: vec!["filesystem".into(), "process".into(), "system_info".into()],
        })
    }
}

impl Platform for MemoryPlatform {
    fn descriptor(&self) -> PlatformDescriptor {
        PlatformDescriptor {
            name: "memory-platform".into(),
            os: "in-memory".into(),
            version: "0.1.0".into(),
            supports: vec!["filesystem".into(), "process".into(), "system_info".into()],
        }
    }
}

#[async_trait]
impl FileSystemPort for MemoryPlatform {
    async fn read(&self, path: &str) -> Result<Vec<u8>, PlatformError> {
        self.fs.read(path).await
    }
    async fn read_text(&self, path: &str) -> Result<String, PlatformError> {
        self.fs.read_text(path).await
    }
    async fn write(&self, path: &str, data: &[u8]) -> Result<(), PlatformError> {
        self.fs.write(path, data).await
    }
    async fn write_text(&self, path: &str, contents: &str) -> Result<(), PlatformError> {
        self.fs.write_text(path, contents).await
    }
    async fn append_text(&self, path: &str, contents: &str) -> Result<(), PlatformError> {
        self.fs.append_text(path, contents).await
    }
    async fn exists(&self, path: &str) -> bool {
        self.fs.exists(path).await
    }
    async fn is_dir(&self, path: &str) -> bool {
        self.fs.is_dir(path).await
    }
    async fn create_dir_all(&self, path: &str) -> Result<(), PlatformError> {
        self.fs.create_dir_all(path).await
    }
    async fn remove(&self, path: &str) -> Result<(), PlatformError> {
        self.fs.remove(path).await
    }
    async fn list(&self, path: &str) -> Result<DirectoryListing, PlatformError> {
        self.fs.list(path).await
    }
    async fn stat(&self, path: &str) -> Result<FileEntry, PlatformError> {
        self.fs.stat(path).await
    }
    async fn rename(&self, from: &str, to: &str) -> Result<(), PlatformError> {
        self.fs.rename(from, to).await
    }
}

#[async_trait]
impl ProcessPort for MemoryPlatform {
    async fn spawn(&self, request: SpawnRequest) -> Result<SpawnResult, PlatformError> {
        self.processes.spawn(request).await
    }
    async fn list(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        self.processes.list().await
    }
    async fn get(&self, pid: u32) -> Result<ProcessInfo, PlatformError> {
        self.processes.get(pid).await
    }
    async fn kill(&self, pid: u32) -> Result<(), PlatformError> {
        self.processes.kill(pid).await
    }
    async fn wait(&self, pid: u32, timeout_ms: u64) -> Result<Option<i32>, PlatformError> {
        self.processes.wait(pid, timeout_ms).await
    }
}

#[async_trait]
impl SystemInfoPort for MemoryPlatform {
    async fn system_info(&self) -> Result<SystemInfo, PlatformError> {
        self.system.system_info().await
    }
    async fn uptime_secs(&self) -> Result<u64, PlatformError> {
        self.system.uptime_secs().await
    }
}

#[async_trait]
impl PowerPort for MemoryPlatform {
    async fn status(&self) -> Result<PowerStatus, PlatformError> {
        self.power.status().await
    }
}

#[async_trait]
impl DataDirectoryPort for MemoryPlatform {
    async fn root(&self) -> Result<PathBuf, PlatformError> {
        self.data.root().await
    }
    async fn area(&self, area: DataArea) -> Result<DataDirectory, PlatformError> {
        self.data.area(area).await
    }
    async fn ensure(&self, area: DataArea) -> Result<PathBuf, PlatformError> {
        self.data.ensure(area).await
    }
}

/// The remaining ports are not exercised by memory doubles — they return
/// Unsupported so the contract stays honest even in tests.
#[async_trait]
impl ApplicationPort for MemoryPlatform {
    async fn launch(&self, _executable: &str, _args: Vec<String>) -> Result<u32, PlatformError> {
        Err(PlatformError::Unsupported("launch".into()))
    }
    async fn open_with_default(&self, _path_or_url: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("open_with_default".into()))
    }
    async fn is_running(&self, _app_id: &str) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("is_running".into()))
    }
}

#[async_trait]
impl NetworkPort for MemoryPlatform {
    async fn interfaces(&self) -> Result<Vec<NetworkInterface>, PlatformError> {
        Ok(Vec::new())
    }
    async fn is_online(&self) -> Result<bool, PlatformError> {
        Ok(false)
    }
    async fn reachable(&self, _host: &str, _timeout_ms: u64) -> Result<bool, PlatformError> {
        Ok(false)
    }
}

#[async_trait]
impl DevicePort for MemoryPlatform {
    async fn devices(&self, _category: Option<DeviceCategory>) -> Result<Vec<DeviceInfo>, PlatformError> {
        Err(PlatformError::Unsupported("devices".into()))
    }
    async fn present(&self, _device_id: &str) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("present".into()))
    }
}

#[async_trait]
impl AudioPort for MemoryPlatform {
    async fn play(&self, _audio: &[u8], _format: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("play".into()))
    }
    async fn stop(&self) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("stop".into()))
    }
    async fn is_playing(&self) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("is_playing".into()))
    }
}

#[async_trait]
impl CameraPort for MemoryPlatform {
    async fn capture(&self, _device_id: &str) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("capture".into()))
    }
}

#[async_trait]
impl DisplayPort for MemoryPlatform {
    async fn screenshot(&self) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("screenshot".into()))
    }
    async fn resolution(&self) -> Result<(u32, u32), PlatformError> {
        Err(PlatformError::Unsupported("resolution".into()))
    }
}

#[async_trait]
impl ComputePort for MemoryPlatform {
    async fn device_name(&self) -> Result<String, PlatformError> {
        Err(PlatformError::Unsupported("device_name".into()))
    }
    async fn compute(&self, _kernel: &str, _data: Vec<u8>) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("compute".into()))
    }
    async fn is_compute_available(&self) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("is_compute_available".into()))
    }
}

#[async_trait]
impl NotificationPort for MemoryPlatform {
    async fn notify(&self, _request: NotificationRequest) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("notify".into()))
    }
}

#[async_trait]
impl PlatformSecurityPort for MemoryPlatform {
    async fn seal_secret(&self, _name: &str, _secret: &[u8]) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("seal_secret".into()))
    }
    async fn unseal_secret(&self, _name: &str) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("unseal_secret".into()))
    }
    async fn delete_secret(&self, _name: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("delete_secret".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_fs_roundtrip() {
        let fs = MemoryFileSystem::new();
        fs.write_text("/tmp/a.txt", "hello").await.unwrap();
        assert!(fs.exists("/tmp/a.txt").await);
        assert_eq!(fs.read_text("/tmp/a.txt").await.unwrap(), "hello");

        fs.append_text("/tmp/a.txt", " world").await.unwrap();
        assert_eq!(fs.read_text("/tmp/a.txt").await.unwrap(), "hello world");

        let listing = fs.list("/tmp").await.unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].name, "a.txt");
    }

    #[tokio::test]
    async fn test_memory_fs_not_found() {
        let fs = MemoryFileSystem::new();
        let err = fs.read("/nope").await.unwrap_err();
        assert!(matches!(err, PlatformError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_memory_fs_seed_and_rename() {
        let fs = MemoryFileSystem::new();
        fs.seed_file("/data/in.txt", "content");
        assert!(fs.exists("/data/in.txt").await);
        fs.rename("/data/in.txt", "/data/out.txt").await.unwrap();
        assert!(!fs.exists("/data/in.txt").await);
        assert!(fs.exists("/data/out.txt").await);
    }

    #[tokio::test]
    async fn test_memory_processes_echo_spawn() {
        let p = MemoryProcesses;
        let r = p
            .spawn(SpawnRequest {
                program: "echo".into(),
                args: vec!["hi".into()],
                working_dir: None,
                env: Vec::new(),
                capture_output: true,
                timeout_ms: None,
            })
            .await
            .unwrap();
        assert_eq!(r.exit_code, Some(0));
        assert_eq!(r.stdout.unwrap(), "echo hi");
    }

    #[tokio::test]
    async fn test_unsupported_platform_identity() {
        let p = UnsupportedPlatform;
        let desc = p.descriptor().await.unwrap();
        assert_eq!(desc.os, "unknown");
        assert!(desc.supports.is_empty());
    }
}