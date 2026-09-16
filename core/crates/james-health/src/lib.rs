use std::sync::Arc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::info;
use chrono::{DateTime, Utc};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use dashmap::DashMap;

use james_events::EventBus;
use james_registry::Registry;
use james_services::ServiceRegistry;
use james_tasks::{TaskManager, TaskStatus};
use james_scheduler::Scheduler;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
    pub details: HashMap<String, serde_json::Value>,
    pub checked_at: DateTime<Utc>,
    pub response_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub overall: HealthStatus,
    pub components: HashMap<String, ComponentHealth>,
    pub checked_at: DateTime<Utc>,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub name: String,
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub critical: bool,
    pub check_fn: String,
}

pub struct HealthMonitor {
    core_status: Arc<RwLock<String>>,
    event_bus: Option<Arc<EventBus>>,
    registry: Option<Arc<Registry>>,
    service_registry: Option<Arc<ServiceRegistry>>,
    task_manager: Option<Arc<TaskManager>>,
    scheduler: Option<Arc<Scheduler>>,
    checks: DashMap<String, HealthCheckConfig>,
    results: DashMap<String, ComponentHealth>,
    start_time: Instant,
    running: Arc<RwLock<bool>>,
    state_dir: PathBuf,
}

fn default_state_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("james")
        .join("state")
}

impl HealthMonitor {
    pub fn new(
        core_status: Arc<RwLock<String>>,
        event_bus: Option<Arc<EventBus>>,
        registry: Option<Arc<Registry>>,
        service_registry: Option<Arc<ServiceRegistry>>,
        task_manager: Option<Arc<TaskManager>>,
        scheduler: Option<Arc<Scheduler>>,
    ) -> Self {
        Self {
            core_status,
            event_bus,
            registry,
            service_registry,
            task_manager,
            scheduler,
            checks: DashMap::new(),
            results: DashMap::new(),
            start_time: Instant::now(),
            running: Arc::new(RwLock::new(false)),
            state_dir: default_state_dir(),
        }
    }

    /// Override where `health.json` is persisted. Production keeps the
    /// default; tests isolate each monitor in a temp dir so parallel runs
    /// never share one state file (previously a flaky cross-test race).
    pub fn with_state_dir(mut self, dir: PathBuf) -> Self {
        self.state_dir = dir;
        self
    }

    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        self.register_default_checks().await;
        info!("Health Monitor started");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.write().await = false;
        info!("Health Monitor stopped");
        Ok(())
    }

    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    async fn register_default_checks(&self) {
        self.checks.insert("core".to_string(), HealthCheckConfig {
            name: "core".to_string(),
            interval_secs: 30,
            timeout_secs: 5,
            critical: true,
            check_fn: "check_core".to_string(),
        });

        self.checks.insert("event-bus".to_string(), HealthCheckConfig {
            name: "event-bus".to_string(),
            interval_secs: 30,
            timeout_secs: 5,
            critical: true,
            check_fn: "check_event_bus".to_string(),
        });

        self.checks.insert("registry".to_string(), HealthCheckConfig {
            name: "registry".to_string(),
            interval_secs: 30,
            timeout_secs: 5,
            critical: true,
            check_fn: "check_registry".to_string(),
        });

        self.checks.insert("services".to_string(), HealthCheckConfig {
            name: "services".to_string(),
            interval_secs: 30,
            timeout_secs: 10,
            critical: false,
            check_fn: "check_services".to_string(),
        });

        self.checks.insert("tasks".to_string(), HealthCheckConfig {
            name: "tasks".to_string(),
            interval_secs: 30,
            timeout_secs: 5,
            critical: false,
            check_fn: "check_tasks".to_string(),
        });

        self.checks.insert("scheduler".to_string(), HealthCheckConfig {
            name: "scheduler".to_string(),
            interval_secs: 30,
            timeout_secs: 5,
            critical: false,
            check_fn: "check_scheduler".to_string(),
        });

        self.checks.insert("disk-space".to_string(), HealthCheckConfig {
            name: "disk-space".to_string(),
            interval_secs: 300,
            timeout_secs: 10,
            critical: true,
            check_fn: "check_disk_space".to_string(),
        });

        self.checks.insert("memory".to_string(), HealthCheckConfig {
            name: "memory".to_string(),
            interval_secs: 60,
            timeout_secs: 5,
            critical: true,
            check_fn: "check_memory".to_string(),
        });
    }

    pub async fn check_all(&self) -> Result<SystemHealth> {
        let mut components = HashMap::new();
        let mut overall = HealthStatus::Healthy;

        for entry in self.checks.iter() {
            let check = entry.value();
            let start = Instant::now();
            
            let result = tokio::time::timeout(
                Duration::from_secs(check.timeout_secs),
                self.run_check(&check.name),
            ).await;

            let response_time = start.elapsed().as_millis() as u64;

            let component = match result {
                Ok(Ok(health)) => health,
                Ok(Err(e)) => ComponentHealth {
                    name: check.name.clone(),
                    status: HealthStatus::Unhealthy,
                    message: Some(e.to_string()),
                    details: HashMap::new(),
                    checked_at: Utc::now(),
                    response_time_ms: response_time,
                },
                Err(_) => ComponentHealth {
                    name: check.name.clone(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Health check timeout".to_string()),
                    details: HashMap::new(),
                    checked_at: Utc::now(),
                    response_time_ms: response_time,
                },
            };

            if check.critical && component.status != HealthStatus::Healthy {
                overall = HealthStatus::Unhealthy;
            } else if overall == HealthStatus::Healthy && component.status == HealthStatus::Degraded {
                overall = HealthStatus::Degraded;
            }

            self.results.insert(check.name.clone(), component.clone());
            components.insert(check.name.clone(), component);
        }

        let system_health = SystemHealth {
            overall,
            components,
            checked_at: Utc::now(),
            uptime_secs: self.start_time.elapsed().as_secs(),
        };

        self.save_health_state(&system_health).await?;

        Ok(system_health)
    }

    async fn run_check(&self, name: &str) -> Result<ComponentHealth> {
        let result = match name {
            "core" => self.check_core().await,
            "event-bus" => self.check_event_bus().await,
            "registry" => self.check_registry().await,
            "services" => self.check_services().await,
            "tasks" => self.check_tasks().await,
            "scheduler" => self.check_scheduler().await,
            "disk-space" => self.check_disk_space().await,
            "memory" => self.check_memory().await,
            _ => Err(anyhow::anyhow!("Unknown check: {}", name)),
        };

        result
    }

    async fn check_core(&self) -> Result<ComponentHealth> {
        let status_str = self.core_status.read().await;
        let status = if *status_str == "Running" {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        };

        Ok(ComponentHealth {
            name: "core".to_string(),
            status,
            message: Some(format!("Core status: {}", status_str)),
            details: [("status".to_string(), serde_json::json!(*status_str))].into(),
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn check_event_bus(&self) -> Result<ComponentHealth> {
        Ok(ComponentHealth {
            name: "event-bus".to_string(),
            status: HealthStatus::Healthy,
            message: Some("Event bus operational".to_string()),
            details: HashMap::new(),
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn check_registry(&self) -> Result<ComponentHealth> {
        let count = self.registry.as_ref().map(|r| r.count()).unwrap_or(0);
        
        Ok(ComponentHealth {
            name: "registry".to_string(),
            status: HealthStatus::Healthy,
            message: Some(format!("Registry has {} entries", count)),
            details: [("entry_count".to_string(), serde_json::json!(count))].into(),
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn check_services(&self) -> Result<ComponentHealth> {
        let services = self.service_registry.as_ref().map(|r| r.list_services()).unwrap_or_default();
        let running = services.iter().filter(|s| s.status == james_services::ServiceStatus::Running).count();
        let failed = services.iter().filter(|s| s.status == james_services::ServiceStatus::Failed).count();

        let status = if failed > 0 {
            HealthStatus::Unhealthy
        } else if running < services.len() && !services.is_empty() {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        Ok(ComponentHealth {
            name: "services".to_string(),
            status,
            message: Some(format!("{} running, {} failed out of {} services", running, failed, services.len())),
            details: [
                ("total".to_string(), serde_json::json!(services.len())),
                ("running".to_string(), serde_json::json!(running)),
                ("failed".to_string(), serde_json::json!(failed)),
            ].into(),
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn check_tasks(&self) -> Result<ComponentHealth> {
        let queued = self.task_manager.as_ref().map(|m| m.count_by_status(TaskStatus::Queued)).unwrap_or(0);
        let running = self.task_manager.as_ref().map(|m| m.count_by_status(TaskStatus::Running)).unwrap_or(0);
        let failed = self.task_manager.as_ref().map(|m| m.count_by_status(TaskStatus::Failed)).unwrap_or(0);

        let status = if failed > running * 2 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        Ok(ComponentHealth {
            name: "tasks".to_string(),
            status,
            message: Some(format!("{} queued, {} running, {} failed", queued, running, failed)),
            details: [
                ("queued".to_string(), serde_json::json!(queued)),
                ("running".to_string(), serde_json::json!(running)),
                ("failed".to_string(), serde_json::json!(failed)),
            ].into(),
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn check_scheduler(&self) -> Result<ComponentHealth> {
        let scheduled = self.scheduler.as_ref().map(|s| s.list_enabled().len()).unwrap_or(0);
        
        Ok(ComponentHealth {
            name: "scheduler".to_string(),
            status: HealthStatus::Healthy,
            message: Some(format!("{} scheduled tasks enabled", scheduled)),
            details: [("scheduled_tasks".to_string(), serde_json::json!(scheduled))].into(),
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn check_disk_space(&self) -> Result<ComponentHealth> {
        let mut status = HealthStatus::Healthy;
        let mut message = String::new();
        let mut details = HashMap::new();

        #[cfg(windows)]
        {
            use std::ffi::OsStr;
            use std::os::windows::ffi::OsStrExt;
            use winapi::um::fileapi::GetDiskFreeSpaceExW;
            use winapi::um::winnt::ULARGE_INTEGER;

            let path = std::env::current_dir()?;
            let path_wide: Vec<u16> = OsStr::new(&path).encode_wide().chain(Some(0)).collect();
            
            let mut free: ULARGE_INTEGER = unsafe { std::mem::zeroed() };
            let mut total: ULARGE_INTEGER = unsafe { std::mem::zeroed() };
            let mut total_free: ULARGE_INTEGER = unsafe { std::mem::zeroed() };

            unsafe {
                GetDiskFreeSpaceExW(
                    path_wide.as_ptr(),
                    &mut free,
                    &mut total,
                    &mut total_free,
                );
            }

            let free_bytes = unsafe { *free.QuadPart() };
            let total_bytes = unsafe { *total.QuadPart() };
            let used_bytes = total_bytes.saturating_sub(free_bytes);
            let usage_percent = if total_bytes > 0 { (used_bytes as f64 / total_bytes as f64) * 100.0 } else { 0.0 };

            if usage_percent > 95.0 {
                status = HealthStatus::Unhealthy;
            } else if usage_percent > 85.0 {
                status = HealthStatus::Degraded;
            }

            message = format!("Disk usage: {:.1}% ({:.1} GB free of {:.1} GB)", usage_percent, free_bytes as f64 / 1e9, total_bytes as f64 / 1e9);
            details.insert("free_bytes".to_string(), serde_json::json!(free_bytes));
            details.insert("total_bytes".to_string(), serde_json::json!(total_bytes));
            details.insert("usage_percent".to_string(), serde_json::json!(usage_percent));
        }

        #[cfg(not(windows))]
        {
            // Portable fallback via sysinfo (verified against sysinfo 0.30.13:
            // Disks::new_with_refreshed_list + Disk::{total_space, available_space}).
            // NOTE: this branch is not compiled on Windows; validated by source
            // inspection, full check happens on non-Windows CI (see H9-04).
            let disks = sysinfo::Disks::new_with_refreshed_list();
            let mut free: u64 = 0;
            let mut total: u64 = 0;
            for disk in disks.list() {
                free = free.saturating_add(disk.available_space());
                total = total.saturating_add(disk.total_space());
            }
            let used = total.saturating_sub(free);
            let usage = if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 };

            if usage > 95.0 { status = HealthStatus::Unhealthy; }
            else if usage > 85.0 { status = HealthStatus::Degraded; }

            message = format!("Disk usage: {:.1}%", usage);
            details.insert("free_bytes".to_string(), serde_json::json!(free));
            details.insert("total_bytes".to_string(), serde_json::json!(total));
            details.insert("usage_percent".to_string(), serde_json::json!(usage));
        }

        Ok(ComponentHealth {
            name: "disk-space".to_string(),
            status,
            message: Some(message),
            details,
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn check_memory(&self) -> Result<ComponentHealth> {
        let mut status = HealthStatus::Healthy;
        let mut message = String::new();
        let mut details = HashMap::new();

#[cfg(target_os = "windows")]
        {
            use winapi::um::psapi::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
            
            let mut perf_info: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
        perf_info.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
            unsafe { GetPerformanceInfo(&mut perf_info, std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32); }
            
            let page_size = perf_info.PageSize as u64;
            let physical_total = perf_info.PhysicalTotal as u64 * page_size;
            let physical_available = perf_info.PhysicalAvailable as u64 * page_size;
            let usage_percent = if physical_total > 0 { ((physical_total - physical_available) as f64 / physical_total as f64) * 100.0 } else { 0.0 };
            
            if usage_percent > 95.0 { status = HealthStatus::Unhealthy; }
            else if usage_percent > 85.0 { status = HealthStatus::Degraded; }
            
            message = format!("Memory usage: {:.1}% ({:.1} GB free of {:.1} GB)", usage_percent, physical_available as f64 / 1e9, physical_total as f64 / 1e9);
            details.insert("total_bytes".to_string(), serde_json::json!(physical_total));
            details.insert("available_bytes".to_string(), serde_json::json!(physical_available));
            details.insert("usage_percent".to_string(), serde_json::json!(usage_percent));
        }

        #[cfg(not(target_os = "windows"))]
        {
            let info = sysinfo::System::new_all();
            let total = info.total_memory();
            let available = info.available_memory();
            let usage = if total > 0 { ((total - available) as f64 / total as f64) * 100.0 } else { 0.0 };

            if usage > 95.0 { status = HealthStatus::Unhealthy; }
            else if usage > 85.0 { status = HealthStatus::Degraded; }

            message = format!("Memory usage: {:.1}%", usage);
            details.insert("total_bytes".to_string(), serde_json::json!(total));
            details.insert("available_bytes".to_string(), serde_json::json!(available));
            details.insert("usage_percent".to_string(), serde_json::json!(usage));
        }

        Ok(ComponentHealth {
            name: "memory".to_string(),
            status,
            message: Some(message),
            details,
            checked_at: Utc::now(),
            response_time_ms: 0,
        })
    }

    async fn save_health_state(&self, health: &SystemHealth) -> Result<()> {
        std::fs::create_dir_all(&self.state_dir)?;
        let path = self.state_dir.join("health.json");
        
        let json = serde_json::to_string_pretty(health)?;
        // Atomic write: temp file + rename, so readers never see a torn file.
        let tmp = self.state_dir.join("health.json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        
        Ok(())
    }

    pub fn get_last_check(&self, name: &str) -> Option<ComponentHealth> {
        self.results.get(name).map(|c| c.clone())
    }

    pub fn get_all_results(&self) -> HashMap<String, ComponentHealth> {
        self.results.iter().map(|e| (e.key().clone(), e.value().clone())).collect()
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new(
            Arc::new(RwLock::new("Starting".to_string())),
            None,
            None,
            None,
            None,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Isolated monitor per test: unique temp state dir, so parallel test
    /// threads never share one health.json (previously flaky cross-talk).
    /// Also keeps tests out of the real user profile data dir.
    fn test_monitor(name: &str) -> HealthMonitor {
        let state = Arc::new(RwLock::new("Running".to_string()));
        let dir = std::env::temp_dir().join(format!("james-health-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        HealthMonitor::new(state, None, None, None, None, None).with_state_dir(dir)
    }

    fn test_state_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("james-health-test-{name}"))
    }

    #[tokio::test]
    async fn test_health_monitor_check_core() {
        let monitor = test_monitor("core");
        monitor.start().await.unwrap();

        let health = monitor.check_all().await.unwrap();
        assert_eq!(health.overall, HealthStatus::Healthy);
        assert!(health.components.contains_key("core"));
        assert_eq!(health.components["core"].status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_health_monitor_disk_space() {
        let monitor = test_monitor("disk");
        monitor.start().await.unwrap();

        let health = monitor.check_all().await.unwrap();
        assert!(health.components.contains_key("disk-space"));
        assert!(health.components["disk-space"].checked_at > DateTime::UNIX_EPOCH);
    }

    #[tokio::test]
    async fn test_health_monitor_memory() {
        let monitor = test_monitor("memory");
        monitor.start().await.unwrap();

        let health = monitor.check_all().await.unwrap();
        assert!(health.components.contains_key("memory"));
    }

    #[tokio::test]
    async fn test_health_state_persistence() {
        let monitor = test_monitor("persistence");
        monitor.start().await.unwrap();

        let _ = monitor.check_all().await.unwrap();
        
        let path = test_state_dir("persistence").join("health.json");
        
        assert!(path.exists());
        let content = std::fs::read_to_string(path).unwrap();
        let health: SystemHealth = serde_json::from_str(&content).unwrap();
        assert!(health.checked_at > DateTime::UNIX_EPOCH);
    }
}