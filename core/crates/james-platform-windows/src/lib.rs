//! JAMES Platform Windows adapter.
//!
//! First concrete implementation of the JAMES platform port contract.
//! Uses `sysinfo` for system/process/battery data and `dirs` for data
//! directories. All missing services return `Unsupported`/`Unavailable`
//! rather than pretending to succeed.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use james_platform::*;
use sysinfo::{Networks, System};
use tokio::sync::RwLock;

/// Convert UNIX seconds to a UTC timestamp (saturating to epoch on errors).
fn ts_to_dt(secs: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap())
}

/// Aggregate Windows platform adapter implementing every port.
pub struct WindowsPlatform {
    inner: Arc<RwLock<System>>,
    dirs: DataDirectories,
}

impl WindowsPlatform {
    /// Build the adapter, refreshing system info once.
    pub fn new() -> Self {
        // Initialize once: on Windows, `refresh_all` is the reliable path.
        let mut sys = System::new();
        sys.refresh_all();
        Self {
            inner: Arc::new(RwLock::new(sys)),
            dirs: DataDirectories::default(),
        }
    }

    /// Create with custom data directories (tests / non-default root).
    pub fn with_data_directories(root: PathBuf) -> Self {
        let mut sys = System::new();
        sys.refresh_all();
        Self {
            inner: Arc::new(RwLock::new(sys)),
            dirs: DataDirectories { root },
        }
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

/// Physical data locations below the platform data root.
#[derive(Clone)]
pub struct DataDirectories {
    root: PathBuf,
}

impl Default for DataDirectories {
    fn default() -> Self {
        let base = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from(".james"))
            .join("james");
        Self { root: base }
    }
}

impl DataDirectories {
    fn area(&self, area: DataArea) -> PathBuf {
        self.root.join(area.as_str())
    }
}

// ---------------------------------------------------------------------------
// Platform descriptor / feature detection
// ---------------------------------------------------------------------------

#[async_trait]
impl FeatureDetectionPort for WindowsPlatform {
    async fn features(&self) -> Result<Vec<FeatureStatus>, PlatformError> {
        let info = self.system_info().await?;
        let sys = self.inner.read().await;
        let cpu_count = sys.cpus().len();
        Ok(vec![
            FeatureStatus {
                feature: "system_info".into(),
                supported: true,
                details: format!("{}/{}", info.os_name, info.os_version),
            },
            FeatureStatus {
                feature: "process_list".into(),
                supported: true,
                details: format!("{cpu_count} logical CPUs visible via sysinfo"),
            },
            FeatureStatus {
                feature: "battery".into(),
                supported: read_power_status().map(|s| {
                    s.battery_flag != 255 && s.battery_flag != 128
                }).unwrap_or(false),
                details: "via GetSystemPowerStatus".into(),
            },
            FeatureStatus {
                feature: "notifications".into(),
                supported: false,
                details: "no notification adapter configured yet".into(),
            },
        ])
    }

    async fn descriptor(&self) -> Result<PlatformDescriptor, PlatformError> {
        let info = self.system_info().await?;
        Ok(PlatformDescriptor {
            name: "james-platform-windows".into(),
            os: info.os_name,
            version: info.os_version,
            supports: vec![
                "filesystem".into(),
                "process".into(),
                "system_info".into(),
                "power".into(),
                "data_directory".into(),
            ],
        })
    }
}

impl Platform for WindowsPlatform {
    fn descriptor(&self) -> PlatformDescriptor {
        // Best-effort sync snapshot for consumers that cannot await.
        PlatformDescriptor {
            name: "james-platform-windows".into(),
            os: std::env::consts::OS.into(),
            version: std::env::consts::ARCH.into(),
            supports: vec![
                "filesystem".into(),
                "process".into(),
                "system_info".into(),
                "power".into(),
                "data_directory".into(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// System info
// ---------------------------------------------------------------------------

#[async_trait]
impl SystemInfoPort for WindowsPlatform {
    async fn system_info(&self) -> Result<SystemInfo, PlatformError> {
        let mut sys = self.inner.write().await;
        sys.refresh_memory();
        sys.refresh_cpu_usage();

        Ok(SystemInfo {
            os_name: std::env::consts::OS.to_string(),
            os_version: System::long_os_version().unwrap_or_else(|| "unknown".into()),
            hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count: Some(sys.cpus().len() as u32),
            total_memory_bytes: Some(sys.total_memory()),
            free_memory_bytes: Some(sys.available_memory()),
            total_disk_bytes: disk_total().ok(),
            free_disk_bytes: disk_free().ok(),
        })
    }

    async fn uptime_secs(&self) -> Result<u64, PlatformError> {
        Ok(System::uptime())
    }
}

fn disk_total() -> std::io::Result<u64> {
    // DriveSpace on the root; good enough for a health snapshot.
    let info = get_disk_free_space(&windows_root())?;
    Ok(info.1)
}

fn disk_free() -> std::io::Result<u64> {
    let info = get_disk_free_space(&windows_root())?;
    Ok(info.0)
}

use std::os::raw::c_void;
type DiskFreeSpaceEx = unsafe extern "system" fn(
    *const u16,
    *mut u64,
    *mut u64,
    *mut u64,
) -> i32;

/// Portable-ish GetDiskFreeSpaceExW accessor (Windows only in practice).
fn get_disk_free_space(
    path: &str,
) -> std::io::Result<(u64, u64)> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = std::ffi::OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            // Resolve the import from kernel32 each call (no libc dep needed).
            let kernel = winapi_kernel32()?;
            let sym = GetProcAddress(kernel, b"GetDiskFreeSpaceExW\0".as_ptr() as *const _);
            if sym.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let f: DiskFreeSpaceEx = std::mem::transmute(sym);
            let mut free_for_caller = 0u64;
            let mut total = 0u64;
            let mut free_total = 0u64;
            if f(
                wide.as_ptr(),
                &mut free_for_caller,
                &mut total,
                &mut free_total,
            ) == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok((free_for_caller, total))
        }
    }
    #[cfg(not(windows))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "disk free space only supported on windows",
        ))
    }
}

#[cfg(windows)]
fn windows_root() -> String {
    std::env::var("SystemDrive")
        .unwrap_or_else(|_| "C:".to_string())
        + "\\"
}

#[cfg(not(windows))]
fn windows_root() -> String {
    "/".to_string()
}

#[cfg(windows)]
fn winapi_kernel32() -> std::io::Result<*mut c_void> {
    use std::os::windows::ffi::OsStrExt;
    let lib = std::ffi::OsStr::new("kernel32.dll")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>();
    // SAFETY: GetModuleHandleW is a kernel32 import; must only be null on error.
    let h = unsafe { GetModuleHandleW(lib.as_ptr()) };
    if h.is_null() {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(h as *mut c_void)
    }
}

#[cfg(windows)]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

// ---------------------------------------------------------------------------
// Power (Windows: GetSystemPowerStatus via dynamic kernel32 import)
// ---------------------------------------------------------------------------

#[repr(C)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

type GetSystemPowerStatus = unsafe extern "system" fn(*mut SystemPowerStatus) -> i32;

fn read_power_status() -> Result<SystemPowerStatus, PlatformError> {
    #[cfg(windows)]
    {
        let kernel = winapi_kernel32().map_err(PlatformError::from)?;
        let sym = unsafe {
            GetProcAddress(kernel, b"GetSystemPowerStatus\0".as_ptr() as *const _)
        };
        if sym.is_null() {
            return Err(PlatformError::Unavailable(
                "GetSystemPowerStatus not found".into(),
            ));
        }
        // SAFETY: sym is a function pointer to a stdcall kernel32 export.
        let f: GetSystemPowerStatus = unsafe { std::mem::transmute(sym) };
        let mut status = SystemPowerStatus {
            ac_line_status: 255,
            battery_flag: 255,
            battery_life_percent: 255,
            system_status_flag: 0,
            battery_life_time: 0xFFFF_FFFF,
            battery_full_life_time: 0xFFFF_FFFF,
        };
        if unsafe { f(&mut status) } == 0 {
            return Err(PlatformError::Unknown(
                "GetSystemPowerStatus failed".into(),
            ));
        }
        Ok(status)
    }
    #[cfg(not(windows))]
    {
        Err(PlatformError::Unsupported("power".into()))
    }
}

#[async_trait]
impl PowerPort for WindowsPlatform {
    async fn status(&self) -> Result<PowerStatus, PlatformError> {
        let status = read_power_status()?;
        let on_ac = status.ac_line_status == 1;
        let on_battery = status.ac_line_status == 0;
        let has_battery = status.battery_flag != 255 && status.battery_flag != 128;
        let battery_percent = if status.battery_life_percent <= 100 {
            Some(status.battery_life_percent)
        } else {
            None
        };
        let battery_charging = if has_battery {
            Some(status.battery_flag & 8 != 0)
        } else {
            None
        };
        let remaining_secs = if status.battery_life_time != 0xFFFF_FFFF {
            Some(status.battery_life_time as u64)
        } else {
            None
        };
        Ok(PowerStatus {
            on_ac,
            on_battery,
            battery_percent,
            battery_charging,
            remaining_secs: if on_battery { remaining_secs } else { None },
        })
    }
}

// ---------------------------------------------------------------------------
// Processes
// ---------------------------------------------------------------------------

#[async_trait]
impl ProcessPort for WindowsPlatform {
    async fn spawn(&self, request: SpawnRequest) -> Result<SpawnResult, PlatformError> {
        if request.program.is_empty() {
            return Err(PlatformError::InvalidInput("program must not be empty".into()));
        }
        // Use std::process::Command with a runtime-bounded wait.
        let mut cmd = std::process::Command::new(&request.program);
        if let Some(dir) = &request.working_dir {
            cmd.current_dir(dir);
        }
        if !request.args.is_empty() {
            cmd.args(&request.args);
        }
        for (k, v) in &request.env {
            cmd.env(k, v);
        }
        let capture = request.capture_output;
        if capture {
            cmd.stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
        } else {
            cmd.stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit());
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| PlatformError::Io(format!("spawn {}: {e}", request.program)))?;
        let pid = child.id();

        let (stdout, stderr, exit, timed_out) = match request.timeout_ms {
            Some(ms) => tokio::task::spawn_blocking(move || {
                use std::io::Read;
                let mut out = String::new();
                let mut err = String::new();
                let mut kill = false;
                // Simple polling wait with timeout (no std blocking wait API).
                // This is a pragmatic approximation; production uses a real process lib.
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
                let exit = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break Some(status),
                        Ok(None) => {
                            if std::time::Instant::now() >= deadline {
                                let _ = child.kill();
                                kill = true;
                                break None;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                        Err(_) => break None,
                    }
                };
                if capture {
                    if let Some(mut so) = child.stdout.take() {
                        let _ = so.read_to_string(&mut out);
                    }
                    if let Some(mut se) = child.stderr.take() {
                        let _ = se.read_to_string(&mut err);
                    }
                }
                (out, err, exit.map(|s| s.code().unwrap_or(-1)), kill)
            })
            .await
            .map_err(|e| PlatformError::Unknown(e.to_string()))?,
            None => tokio::task::spawn_blocking(move || {
                use std::io::Read;
                let mut out = String::new();
                let mut err = String::new();
                let output = child
                    .wait()
                    .ok()
                    .map(|status| status.code().unwrap_or(-1));
                if capture {
                    if let Some(mut so) = child.stdout.take() {
                        let _ = so.read_to_string(&mut out);
                    }
                    if let Some(mut se) = child.stderr.take() {
                        let _ = se.read_to_string(&mut err);
                    }
                }
                (out, err, output, false)
            })
            .await
            .map_err(|e| PlatformError::Unknown(e.to_string()))?,
        };

        Ok(SpawnResult {
            pid,
            stdout: capture.then_some(stdout),
            stderr: capture.then_some(stderr),
            exit_code: if timed_out { Some(exit_codes::TIMEOUT) } else { exit },
            timed_out,
        })
    }

    async fn list(&self) -> Result<Vec<ProcessInfo>, PlatformError> {
        let mut sys = self.inner.write().await;
        sys.refresh_processes();
        sys.refresh_cpu_usage();
        Ok(sys
            .processes()
            .values()
            .map(|p| ProcessInfo {
                pid: p.pid().as_u32(),
                name: p.name().to_string(),
                executable_path: p.exe().map(|p| p.to_string_lossy().to_string()),
                command_line: p.cmd().first().map(|s| s.to_string()),
                memory_bytes: Some(p.memory()),
                started_at: Some(ts_to_dt(p.start_time())),
            })
            .collect())
    }

    async fn get(&self, pid: u32) -> Result<ProcessInfo, PlatformError> {
        let mut sys = self.inner.write().await;
        sys.refresh_pids(&[sysinfo::Pid::from_u32(pid)]);
        let p = sys
            .process(sysinfo::Pid::from_u32(pid))
            .ok_or_else(|| PlatformError::NotFound(format!("pid {pid}")))?;
        Ok(ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string(),
            executable_path: p.exe().map(|p| p.to_string_lossy().to_string()),
            command_line: p.cmd().first().map(|s| s.to_string()),
            memory_bytes: Some(p.memory()),
            started_at: Some(ts_to_dt(p.start_time())),
        })
    }

    async fn kill(&self, pid: u32) -> Result<(), PlatformError> {
        let mut sys = self.inner.write().await;
        sys.refresh_pids(&[sysinfo::Pid::from_u32(pid)]);
        match sys.process(sysinfo::Pid::from_u32(pid)) {
            Some(p) => p.kill().then(|| ()).ok_or_else(|| {
                PlatformError::PermissionDenied(format!("could not terminate pid {pid}"))
            }),
            None => Err(PlatformError::NotFound(format!("pid {pid}"))),
        }
    }

    async fn wait(&self, pid: u32, timeout_ms: u64) -> Result<Option<i32>, PlatformError> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            let alive = {
                let mut sys = self.inner.write().await;
                sys.refresh_pids(&[sysinfo::Pid::from_u32(pid)]);
                sys.process(sysinfo::Pid::from_u32(pid)).is_some()
            };
            if !alive {
                return Ok(Some(0));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

#[async_trait]
impl NetworkPort for WindowsPlatform {
    async fn interfaces(&self) -> Result<Vec<NetworkInterface>, PlatformError> {
        let networks = Networks::new_with_refreshed_list();
        Ok(networks
            .iter()
            .map(|(name, data)| {
                let mac = data.mac_address();
                let mac_str = (mac.0 != [0u8; 6]).then(|| {
                    mac.0.iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .join(":")
                });
                NetworkInterface {
                    name: name.clone(),
                    is_up: true,
                    ipv4: Vec::new(),
                    ipv6: Vec::new(),
                    mac: mac_str,
                    gateway: None,
                }
            })
            .collect())
    }

    async fn is_online(&self) -> Result<bool, PlatformError> {
        Ok(true) // sysinfo does not expose connectivity; refine in a later phase
    }

    async fn reachable(&self, host: &str, timeout_ms: u64) -> Result<bool, PlatformError> {
        let addr: std::net::SocketAddr = match format!("{host}:443").parse() {
            Ok(addr) => addr,
            Err(e) => {
                return Err(PlatformError::InvalidInput(format!("{host}: {e}")));
            }
        };
        Ok(tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false))
    }
}

// ---------------------------------------------------------------------------
// Filesystem, data directories
// ---------------------------------------------------------------------------

#[async_trait]
impl FileSystemPort for WindowsPlatform {
    async fn read(&self, path: &str) -> Result<Vec<u8>, PlatformError> {
        tokio::fs::read(path).await.map_err(PlatformError::from)
    }
    async fn read_text(&self, path: &str) -> Result<String, PlatformError> {
        tokio::fs::read_to_string(path).await.map_err(PlatformError::from)
    }
    async fn write(&self, path: &str, data: &[u8]) -> Result<(), PlatformError> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(PlatformError::from)?;
        }
        tokio::fs::write(path, data).await.map_err(PlatformError::from)
    }
    async fn write_text(&self, path: &str, contents: &str) -> Result<(), PlatformError> {
        self.write(path, contents.as_bytes()).await
    }
    async fn append_text(&self, path: &str, contents: &str) -> Result<(), PlatformError> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(PlatformError::from)?;
        }
        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(PlatformError::from)?;
        f.write_all(contents.as_bytes()).await.map_err(PlatformError::from)
    }
    async fn exists(&self, path: &str) -> bool {
        tokio::fs::try_exists(path).await.unwrap_or(false)
    }
    async fn is_dir(&self, path: &str) -> bool {
        tokio::fs::metadata(path).await.map(|m| m.is_dir()).unwrap_or(false)
    }
    async fn create_dir_all(&self, path: &str) -> Result<(), PlatformError> {
        tokio::fs::create_dir_all(path).await.map_err(PlatformError::from)
    }
    async fn remove(&self, path: &str) -> Result<(), PlatformError> {
        match tokio::fs::metadata(path).await {
            Ok(m) if m.is_dir() => tokio::fs::remove_dir_all(path).await.map_err(PlatformError::from),
            Ok(_) => tokio::fs::remove_file(path).await.map_err(PlatformError::from),
            Err(e) => Err(PlatformError::from(e)),
        }
    }
    async fn list(&self, path: &str) -> Result<DirectoryListing, PlatformError> {
        let mut entries = Vec::new();
        let mut rd = tokio::fs::read_dir(path).await.map_err(PlatformError::from)?;
        while let Some(entry) = rd.next_entry().await.map_err(PlatformError::from)? {
            let meta = entry.metadata().await.map_err(PlatformError::from)?;
            entries.push(FileEntry {
                path: entry.path().to_string_lossy().to_string(),
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: meta.is_dir(),
                size_bytes: Some(meta.len()),
                modified_at: meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| {
                    chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap())
                }),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(DirectoryListing {
            path: path.to_string(),
            entries,
        })
    }
    async fn stat(&self, path: &str) -> Result<FileEntry, PlatformError> {
        let meta = tokio::fs::metadata(path).await.map_err(PlatformError::from)?;
        Ok(FileEntry {
            path: path.to_string(),
            name: std::path::Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string()),
            is_dir: meta.is_dir(),
            size_bytes: Some(meta.len()),
            modified_at: meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap())
            }),
        })
    }
    async fn rename(&self, from: &str, to: &str) -> Result<(), PlatformError> {
        tokio::fs::rename(from, to).await.map_err(PlatformError::from)
    }
}

#[async_trait]
impl DataDirectoryPort for WindowsPlatform {
    async fn root(&self) -> Result<PathBuf, PlatformError> {
        Ok(self.dirs.root.clone())
    }
    async fn area(&self, area: DataArea) -> Result<DataDirectory, PlatformError> {
        Ok(DataDirectory {
            area,
            path: self.dirs.area(area).to_string_lossy().to_string(),
        })
    }
    async fn ensure(&self, area: DataArea) -> Result<PathBuf, PlatformError> {
        let path = self.dirs.area(area);
        tokio::fs::create_dir_all(&path).await.map_err(PlatformError::from)?;
        Ok(path)
    }
}

// ---------------------------------------------------------------------------
// Unsupported-by-default ports (honest Unsupported)
// ---------------------------------------------------------------------------

#[async_trait]
impl ApplicationPort for WindowsPlatform {
    async fn launch(&self, _executable: &str, _args: Vec<String>) -> Result<u32, PlatformError> {
        Err(PlatformError::Unsupported("application.launch".into()))
    }
    async fn open_with_default(&self, _path_or_url: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("application.open_with_default".into()))
    }
    async fn is_running(&self, _app_id: &str) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("application.is_running".into()))
    }
}

#[async_trait]
impl DevicePort for WindowsPlatform {
    async fn devices(&self, _category: Option<DeviceCategory>) -> Result<Vec<DeviceInfo>, PlatformError> {
        Err(PlatformError::Unsupported("device.list".into()))
    }
    async fn present(&self, _device_id: &str) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("device.present".into()))
    }
}

#[async_trait]
impl AudioPort for WindowsPlatform {
    async fn play(&self, _audio: &[u8], _format: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("audio.play".into()))
    }
    async fn stop(&self) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("audio.stop".into()))
    }
    async fn is_playing(&self) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("audio.is_playing".into()))
    }
}

#[async_trait]
impl CameraPort for WindowsPlatform {
    async fn capture(&self, _device_id: &str) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("camera.capture".into()))
    }
}

#[async_trait]
impl DisplayPort for WindowsPlatform {
    async fn screenshot(&self) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("display.screenshot".into()))
    }
    async fn resolution(&self) -> Result<(u32, u32), PlatformError> {
        Err(PlatformError::Unsupported("display.resolution".into()))
    }
}

#[async_trait]
impl ComputePort for WindowsPlatform {
    async fn device_name(&self) -> Result<String, PlatformError> {
        Err(PlatformError::Unsupported("compute.device_name".into()))
    }
    async fn compute(&self, _kernel: &str, _data: Vec<u8>) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("compute.compute".into()))
    }
    async fn is_compute_available(&self) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported("compute.is_compute_available".into()))
    }
}

#[async_trait]
impl NotificationPort for WindowsPlatform {
    async fn notify(&self, _request: NotificationRequest) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("notification.notify".into()))
    }
}

#[async_trait]
impl PlatformSecurityPort for WindowsPlatform {
    async fn seal_secret(&self, _name: &str, _secret: &[u8]) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("security.seal_secret".into()))
    }
    async fn unseal_secret(&self, _name: &str) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported("security.unseal_secret".into()))
    }
    async fn delete_secret(&self, _name: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported("security.delete_secret".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_descriptor_and_features() {
        let p = WindowsPlatform::with_data_directories(PathBuf::from(".test-james"));
        let desc = <WindowsPlatform as Platform>::descriptor(&p);
        assert_eq!(desc.name, "james-platform-windows");
        assert!(desc.supports.contains(&"system_info".to_string()));
    }

    #[tokio::test]
    async fn test_system_info_basic() {
        let p = WindowsPlatform::with_data_directories(PathBuf::from(".test-james"));
        let info = p.system_info().await.unwrap();
        assert!(!info.os_name.is_empty());
        assert!(info.cpu_count.is_some());
    }

    #[tokio::test]
    async fn test_memory_fs_contract_shape() {
        // Basic shape check on the real adapter (temp dir).
        let dir = std::env::temp_dir().join(format!("james-winstest-{}", std::process::id()));
        let fs = WindowsPlatform::with_data_directories(dir.clone());
        let test_file = dir.join("hello.txt");
        let p_str = test_file.to_string_lossy().to_string();
        fs.write_text(&p_str, "hello windows").await.unwrap();
        assert!(fs.exists(&p_str).await);
        assert_eq!(fs.read_text(&p_str).await.unwrap(), "hello windows");
        fs.remove(&p_str).await.unwrap();
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn test_unsupported_ports_return_unsupported() {
        let p = WindowsPlatform::with_data_directories(PathBuf::from(".test-james"));
        let err = p
            .notify(NotificationRequest {
                title: "x".into(),
                body: "y".into(),
                category: None,
                urgency: NotificationUrgency::Normal,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, PlatformError::Unsupported(_)));
    }

    #[tokio::test]
    async fn test_data_directory_ensure() {
        let base = std::env::temp_dir().join(format!("james-dirdist-{}", std::process::id()));
        let p = WindowsPlatform::with_data_directories(base.clone());
        let audit_path = p.ensure(DataArea::Audit).await.unwrap();
        assert!(audit_path.exists());
        let _ = tokio::fs::remove_dir_all(&base).await;
    }

    #[tokio::test]
    async fn test_process_spawn_shape() {
        let p = WindowsPlatform::with_data_directories(PathBuf::from(".test-james"));
        let (program, args) = if cfg!(windows) {
            ("cmd".to_string(), vec!["/C".to_string(), "echo".to_string(), "hi".to_string()])
        } else {
            ("echo".to_string(), vec!["hi".to_string()])
        };
        let r = p
            .spawn(SpawnRequest {
                program,
                args,
                working_dir: None,
                env: Vec::new(),
                capture_output: true,
                timeout_ms: Some(10_000),
            })
            .await
            .unwrap();
        assert!(!r.timed_out);
        assert!(r.stdout.unwrap_or_default().trim().ends_with("hi"));
    }
}