//! JAMES Platform — portable platform port traits (JAMES Architekturvertrag).
//!
//! The portable core may only use the abstractions defined here. OS-specific
//! behavior lives behind adapters (first implementation: Windows). Every port
//! returns serializable, platform-neutral domain types and a rich error type
//! so the core never derives success from a platform failure.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod doubles;
pub mod ports;

pub use doubles::*;
pub use ports::*;

/// Platform error contract (platform-abstraction.md §Fehlervertrag).
/// The core must treat any platform error as failure, never as success.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize, JsonSchema)]
pub enum PlatformError {
    #[error("platform service is not supported on this adapter: {0}")]
    Unsupported(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("platform service unavailable: {0}")]
    Unavailable(String),
    #[error("platform operation timed out: {0}")]
    Timeout(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("unknown platform error: {0}")]
    Unknown(String),
}

impl From<std::io::Error> for PlatformError {
    fn from(e: std::io::Error) -> Self {
        PlatformError::Io(e.to_string())
    }
}

/// A file system entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<DateTime<Utc>>,
}

/// A directory listing result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DirectoryListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
}

/// A running process / process info snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub executable_path: Option<String>,
    pub command_line: Option<String>,
    pub memory_bytes: Option<u64>,
    pub started_at: Option<DateTime<Utc>>,
}

/// Process spawn request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SpawnRequest {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub env: Vec<(String, String)>,
    pub capture_output: bool,
    pub timeout_ms: Option<u64>,
}

/// Result of spawning a process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SpawnResult {
    pub pid: u32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

/// System information snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub hostname: String,
    pub arch: String,
    pub cpu_count: Option<u32>,
    pub total_memory_bytes: Option<u64>,
    pub free_memory_bytes: Option<u64>,
    pub total_disk_bytes: Option<u64>,
    pub free_disk_bytes: Option<u64>,
}

/// Power status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PowerStatus {
    pub on_ac: bool,
    pub on_battery: bool,
    pub battery_percent: Option<u8>,
    pub battery_charging: Option<bool>,
    pub remaining_secs: Option<u64>,
}

/// A network interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NetworkInterface {
    pub name: String,
    pub is_up: bool,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub mac: Option<String>,
    pub gateway: Option<String>,
}

/// A desktop notification request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationRequest {
    pub title: String,
    pub body: String,
    pub category: Option<String>,
    pub urgency: NotificationUrgency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

/// Logical data areas (platform-abstraction.md §Datenregeln).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataArea {
    Identity,
    Configuration,
    Memory,
    Tasks,
    Agents,
    Modules,
    Events,
    State,
    Audit,
    Inventory,
}

impl DataArea {
    pub fn as_str(&self) -> &'static str {
        match self {
            DataArea::Identity => "identity",
            DataArea::Configuration => "configuration",
            DataArea::Memory => "memory",
            DataArea::Tasks => "tasks",
            DataArea::Agents => "agents",
            DataArea::Modules => "modules",
            DataArea::Events => "events",
            DataArea::State => "state",
            DataArea::Audit => "audit",
            DataArea::Inventory => "inventory",
        }
    }
}

/// Resolved physical location for a logical data area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DataDirectory {
    pub area: DataArea,
    pub path: String,
}

/// Capability / feature detection result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureStatus {
    pub feature: String,
    pub supported: bool,
    pub details: String,
}

/// A platform adapter self-identifies with this summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlatformDescriptor {
    pub name: String,
    pub os: String,
    pub version: String,
    pub supports: Vec<String>,
}

/// Device category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceCategory {
    Input,
    Audio,
    Camera,
    Display,
    Storage,
    Network,
    Battery,
    Other,
}

/// A discovered device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub category: DeviceCategory,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub status: String,
}

/// Exit codes used for process spawn handling across adapters.
pub mod exit_codes {
    /// Conventional success exit code.
    pub const SUCCESS: i32 = 0;
    /// Exit code that means the process timed out on this adapter.
    pub const TIMEOUT: i32 = 124;
    /// Process was terminated/cancelled.
    pub const TERMINATED: i32 = 143;
}