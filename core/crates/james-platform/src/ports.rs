//! Platform port traits (platform-abstraction.md §Ports).
//!
//! The portable core compiles against these traits only. All types are
//! serializable, platform-neutral domain types.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::*;

/// Logical file system operations.
#[async_trait]
pub trait FileSystemPort: Send + Sync {
    async fn read(&self, path: &str) -> Result<Vec<u8>, PlatformError>;
    async fn read_text(&self, path: &str) -> Result<String, PlatformError>;
    async fn write(&self, path: &str, data: &[u8]) -> Result<(), PlatformError>;
    async fn write_text(&self, path: &str, contents: &str) -> Result<(), PlatformError>;
    async fn append_text(&self, path: &str, contents: &str) -> Result<(), PlatformError>;
    async fn exists(&self, path: &str) -> bool;
    async fn is_dir(&self, path: &str) -> bool;
    async fn create_dir_all(&self, path: &str) -> Result<(), PlatformError>;
    async fn remove(&self, path: &str) -> Result<(), PlatformError>;
    async fn list(&self, path: &str) -> Result<DirectoryListing, PlatformError>;
    async fn stat(&self, path: &str) -> Result<FileEntry, PlatformError>;
    async fn rename(&self, from: &str, to: &str) -> Result<(), PlatformError>;
}

/// Process control operations.
#[async_trait]
pub trait ProcessPort: Send + Sync {
    async fn spawn(&self, request: SpawnRequest) -> Result<SpawnResult, PlatformError>;
    async fn list(&self) -> Result<Vec<ProcessInfo>, PlatformError>;
    async fn get(&self, pid: u32) -> Result<ProcessInfo, PlatformError>;
    async fn kill(&self, pid: u32) -> Result<(), PlatformError>;
    async fn wait(&self, pid: u32, timeout_ms: u64) -> Result<Option<i32>, PlatformError>;
}

/// Application launch / file-open operations.
#[async_trait]
pub trait ApplicationPort: Send + Sync {
    async fn launch(&self, executable: &str, args: Vec<String>) -> Result<u32, PlatformError>;
    async fn open_with_default(&self, path_or_url: &str) -> Result<(), PlatformError>;
    async fn is_running(&self, app_id: &str) -> Result<bool, PlatformError>;
}

/// Network operations (interfaces, connectivity).
#[async_trait]
pub trait NetworkPort: Send + Sync {
    async fn interfaces(&self) -> Result<Vec<NetworkInterface>, PlatformError>;
    async fn is_online(&self) -> Result<bool, PlatformError>;
    async fn reachable(&self, host: &str, timeout_ms: u64) -> Result<bool, PlatformError>;
}

/// Device discovery operations.
#[async_trait]
pub trait DevicePort: Send + Sync {
    async fn devices(&self, category: Option<DeviceCategory>) -> Result<Vec<DeviceInfo>, PlatformError>;
    async fn present(&self, device_id: &str) -> Result<bool, PlatformError>;
}

/// Audio playback / capture operations.
#[async_trait]
pub trait AudioPort: Send + Sync {
    async fn play(&self, audio: &[u8], format: &str) -> Result<(), PlatformError>;
    async fn stop(&self) -> Result<(), PlatformError>;
    async fn is_playing(&self) -> Result<bool, PlatformError>;
}

/// Camera capture operations.
#[async_trait]
pub trait CameraPort: Send + Sync {
    async fn capture(&self, device_id: &str) -> Result<Vec<u8>, PlatformError>;
}

/// Display / screenshot operations.
#[async_trait]
pub trait DisplayPort: Send + Sync {
    async fn screenshot(&self) -> Result<Vec<u8>, PlatformError>;
    async fn resolution(&self) -> Result<(u32, u32), PlatformError>;
}

/// Compute (GPU / CPU offload) operations.
#[async_trait]
pub trait ComputePort: Send + Sync {
    async fn device_name(&self) -> Result<String, PlatformError>;
    async fn compute(&self, kernel: &str, data: Vec<u8>) -> Result<Vec<u8>, PlatformError>;
    async fn is_compute_available(&self) -> Result<bool, PlatformError>;
}

/// Desktop notification operations.
#[async_trait]
pub trait NotificationPort: Send + Sync {
    async fn notify(&self, request: NotificationRequest) -> Result<(), PlatformError>;
}

/// System information operations.
#[async_trait]
pub trait SystemInfoPort: Send + Sync {
    async fn system_info(&self) -> Result<SystemInfo, PlatformError>;
    async fn uptime_secs(&self) -> Result<u64, PlatformError>;
}

/// Power / battery operations.
#[async_trait]
pub trait PowerPort: Send + Sync {
    async fn status(&self) -> Result<PowerStatus, PlatformError>;
}

/// Platform security operations (keychain, credential sealing, lockdown).
#[async_trait]
pub trait PlatformSecurityPort: Send + Sync {
    async fn seal_secret(&self, name: &str, secret: &[u8]) -> Result<(), PlatformError>;
    async fn unseal_secret(&self, name: &str) -> Result<Vec<u8>, PlatformError>;
    async fn delete_secret(&self, name: &str) -> Result<(), PlatformError>;
}

/// Logical → physical data directory resolution.
#[async_trait]
pub trait DataDirectoryPort: Send + Sync {
    async fn root(&self) -> Result<PathBuf, PlatformError>;
    async fn area(&self, area: DataArea) -> Result<DataDirectory, PlatformError>;
    async fn ensure(&self, area: DataArea) -> Result<PathBuf, PlatformError>;
}

/// Feature detection: which ports each adapter actually implements.
#[async_trait]
pub trait FeatureDetectionPort: Send + Sync {
    async fn features(&self) -> Result<Vec<FeatureStatus>, PlatformError>;
    async fn descriptor(&self) -> Result<PlatformDescriptor, PlatformError>;
}

/// Aggregate: a complete platform adapter implements every port.
#[async_trait]
pub trait Platform: FileSystemPort
    + ProcessPort
    + ApplicationPort
    + NetworkPort
    + DevicePort
    + AudioPort
    + CameraPort
    + DisplayPort
    + ComputePort
    + NotificationPort
    + SystemInfoPort
    + PowerPort
    + PlatformSecurityPort
    + DataDirectoryPort
    + FeatureDetectionPort
    + Send
    + Sync
{
    fn descriptor(&self) -> PlatformDescriptor;
}