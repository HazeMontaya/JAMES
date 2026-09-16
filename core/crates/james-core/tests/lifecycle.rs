//! F1-05: L3 integration — full lifecycle through the public facade:
//! new -> subscribe -> start -> SYSTEM_STARTED observed -> stop.
//! Uses tight intervals so the test finishes in milliseconds and never
//! touches the real user profile more than health's default path does.

use james_core::{CoreConfig, CoreStatus, JamesCore};
use james_events::builtin_events;
use james_tasks::{TaskManager, Task, TaskStatus, TaskPriority, RetryPolicy};
use james_services::{ServiceRegistry, ServiceDefinition, ServiceStatus, RestartPolicy};
use james_capabilities::CapabilityRegistry;
use james_registry::{Registry, RegistryEntryType, RegistryStatus};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_core_startup_event_shutdown() {
    let mut config = CoreConfig::default();
    config.health_check_interval_secs = 3600;
    config.scheduler_tick_interval_secs = 3600;

    let mut core = JamesCore::new(config).await.unwrap();
    assert_eq!(core.status().await, CoreStatus::Starting);

    let mut rx = core
        .event_bus()
        .subscribe(builtin_events::SYSTEM_STARTED);

    core.start().await.unwrap();
    assert_eq!(core.status().await, CoreStatus::Running);

    let envelope = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .expect("SYSTEM_STARTED timeout")
        .expect("event channel closed");
    assert_eq!(envelope.event.event_type, builtin_events::SYSTEM_STARTED);

    core.stop().await.unwrap();
    assert_eq!(core.status().await, CoreStatus::Stopped);
}

#[tokio::test]
async fn test_core_task_lifecycle_integration() {
    let mut config = CoreConfig::default();
    config.health_check_interval_secs = 3600;
    config.scheduler_tick_interval_secs = 3600;

    let mut core = JamesCore::new(config).await.unwrap();
    core.start().await.unwrap();

    let task_manager = core.task_manager();
    let event_bus = core.event_bus();

    // Subscribe to task events
    let mut rx_created = event_bus.subscribe("task.created");
    let mut rx_started = event_bus.subscribe("task.started");
    let mut rx_completed = event_bus.subscribe("task.completed");

    // Create a task
    let task = Task {
        id: Uuid::nil(),
        task_type: "integration-test".to_string(),
        name: "Integration Test Task".to_string(),
        priority: TaskPriority::Normal,
        status: TaskStatus::Queued,
        created_at: DateTime::UNIX_EPOCH,
        started_at: None,
        completed_at: None,
        source: "test".to_string(),
        dependencies: vec![],
        timeout_secs: 60,
        retry_policy: RetryPolicy::default(),
        current_retry: 0,
        payload: json!({"input": "test"}),
        result: None,
        error: None,
        assigned_worker: None,
        progress: None,
    };

    let id = task_manager.create_task(task).await.unwrap();
    
    // Verify TASK_CREATED event
    let created = tokio::time::timeout(Duration::from_secs(5), rx_created.recv())
        .await
        .expect("TASK_CREATED timeout")
        .expect("event channel closed");
    assert_eq!(created.event.payload["task_id"], id.to_string());

    // Update status to Running
    task_manager.update_status(id, TaskStatus::Running).await.unwrap();
    
    // Verify TASK_STARTED event
    let started = tokio::time::timeout(Duration::from_secs(5), rx_started.recv())
        .await
        .expect("TASK_STARTED timeout")
        .expect("event channel closed");
    assert_eq!(started.event.event_type, "task.started");

    // Complete the task
    task_manager.set_result(id, json!({"output": "success"})).await.unwrap();
    
    // Verify TASK_COMPLETED event
    let completed = tokio::time::timeout(Duration::from_secs(5), rx_completed.recv())
        .await
        .expect("TASK_COMPLETED timeout")
        .expect("event channel closed");
    assert_eq!(completed.event.event_type, "task.completed");

    core.stop().await.unwrap();
}

#[tokio::test]
async fn test_core_service_lifecycle_integration() {
    let mut config = CoreConfig::default();
    config.health_check_interval_secs = 3600;
    config.scheduler_tick_interval_secs = 3600;

    let mut core = JamesCore::new(config).await.unwrap();
    core.start().await.unwrap();

    let service_registry = core.service_registry();
    let event_bus = core.event_bus();

    let mut rx = event_bus.subscribe("service.started");

    let def = ServiceDefinition {
        id: "integration-service".to_string(),
        name: "Integration Service".to_string(),
        version: "1.0.0".to_string(),
        provider: "test".to_string(),
        description: "Integration test service".to_string(),
        capabilities: vec![],
        dependencies: vec![],
        config_schema: None,
        default_config: None,
        health_check_endpoint: None,
        restart_policy: RestartPolicy::Never,
    };

    service_registry.register_definition(def).unwrap();
    let instance = service_registry.start_service("integration-service", None).await.unwrap();
    assert_eq!(instance.status, ServiceStatus::Running);

    // Verify SERVICE_STARTED event
    let envelope = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("SERVICE_STARTED timeout")
        .expect("event channel closed");
    assert_eq!(envelope.event.event_type, "service.started");
    assert_eq!(envelope.event.payload["service_id"], "integration-service");

    let stopped = service_registry.stop_service("integration-service").await.unwrap();
    assert!(stopped);

    core.stop().await.unwrap();
}

#[tokio::test]
async fn test_core_registry_capability_integration() {
    let mut config = CoreConfig::default();
    config.health_check_interval_secs = 3600;
    config.scheduler_tick_interval_secs = 3600;

    let mut core = JamesCore::new(config).await.unwrap();
    core.start().await.unwrap();

    let registry = core.registry();
    let capability_registry = core.capability_registry();

    // Register a module with capabilities
    use james_registry::{RegistryEntry, RegistryEntryType, RegistryStatus};
    use chrono::Utc;
    
    let entry = RegistryEntry {
        id: Uuid::nil(),
        entry_type: RegistryEntryType::Module,
        name: "test-module".to_string(),
        version: "1.0.0".to_string(),
        provider: "test".to_string(),
        status: RegistryStatus::Active,
        capabilities: vec!["custom.test.capability".to_string()],
        dependencies: vec![],
        metadata: serde_json::json!({}),
        registered_at: Utc::now(),
        last_seen: Utc::now(),
        heartbeat_interval_secs: Some(30),
    };

    let id = registry.register(entry).await.unwrap();
    
    // Verify capability is registered
    let caps = capability_registry.list_by_provider("test");
    assert!(caps.iter().any(|c| c.definition.id == "custom.test.capability"));

    // Unregister module
    registry.unregister(id).await.unwrap();
    
    // Capability should still exist (capabilities are independent)
    let caps = capability_registry.list_by_provider("test");
    assert!(caps.iter().any(|c| c.definition.id == "custom.test.capability"));

    core.stop().await.unwrap();
}
