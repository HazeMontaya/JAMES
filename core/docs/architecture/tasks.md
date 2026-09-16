# JAMES Task System Architecture

## Overview

The Task System provides a generic, priority-based task execution framework. It handles task lifecycle, dependencies, retries, and integrates with the Event Bus for observability.

## Task Model

### Task Structure

```rust
struct Task {
    id: Uuid,                      // UUID v7 (time-ordered)
    task_type: String,             // Category: "discovery", "automation", "sync"
    name: String,                  // Human-readable
    priority: TaskPriority,        // Low/Normal/High/Critical
    status: TaskStatus,            // Current state
    created_at: DateTime<Utc>,     // When queued
    started_at: Option<DateTime<Utc>>, // When execution began
    completed_at: Option<DateTime<Utc>>, // When finished
    source: String,                // Origin: "user", "scheduler", "workflow"
    dependencies: Vec<Uuid>,       // Task IDs that must complete first
    timeout_secs: u64,             // Max execution time
    retry_policy: RetryPolicy,     // Retry configuration
    current_retry: u32,            // Attempt number
    payload: Value,                // Input data
    result: Option<Value>,         // Output data (on success)
    error: Option<String>,         // Error message (on failure)
    assigned_worker: Option<String>, // Worker identifier
    progress: Option<f32>,         // 0.0-1.0
}
```

### Priority Levels

| Priority | Value | Use Case |
|----------|-------|----------|
| `Low` | 0 | Background maintenance |
| `Normal` | 50 | Standard operations |
| `High` | 100 | User-facing actions |
| `Critical` | 200 | System integrity |

### Status Transitions

```
QUEUED
  │
  ├─► RUNNING (worker picks up)
  │     │
  │     ├─► COMPLETED (success)
  │     │
  │     ├─► FAILED (error)
  │     │     │
  │     │     └─► RETRY (if retries left) ──► QUEUED
  │     │
  │     ├─► TIMEOUT (exceeded timeout_secs)
  │     │
  │     └─► CANCELLED (explicit cancel)
  │
  └─► CANCELLED (before start)
```

## Retry Policy

```rust
struct RetryPolicy {
    max_retries: u32,           // Default: 3
    base_delay_secs: u64,       // Default: 1
    max_delay_secs: u64,        // Default: 60
    exponential_backoff: bool,  // Default: true
    retry_on: Vec<String>,      // Error types to retry
}
```

### Delay Calculation

```
exponential_backoff = true:
  attempt 1: base_delay * 2^0 = 1s
  attempt 2: base_delay * 2^1 = 2s
  attempt 3: base_delay * 2^2 = 4s
  ...
  capped at max_delay_secs

exponential_backoff = false:
  all attempts: base_delay_secs
```

### Retry Triggers

Retry occurs when:
- Task status becomes `Failed`
- Error type matches `retry_on` list
- `current_retry < max_retries`

Default `retry_on`: `["timeout", "error", "unavailable"]`

## Dependency Management

Tasks can depend on other tasks:

```rust
Task {
    dependencies: vec![uuid1, uuid2],
    // ...
}
```

### Dependency Resolution

1. Task created with dependencies → `QUEUED`
2. Task Manager checks dependencies on each tick
3. When all dependencies are `COMPLETED`, task becomes `RUNNABLE`
4. Scheduler picks up `RUNNABLE` tasks based on priority

### Dependency Graph

```
Task A (no deps)
    │
    ├─► Task B (depends on A)
    │
    └─► Task C (depends on A)
          │
          └─► Task D (depends on B, C)
```

Cycle detection prevents circular dependencies.

## Worker Model

### Task Assignment

- Workers register with Task Manager
- Tasks assigned based on:
  1. Priority (higher first)
  2. Dependencies satisfied
  3. Worker capacity
  4. FIFO within same priority

### Worker Interface

```rust
trait TaskWorker {
    fn can_handle(&self, task_type: &str) -> bool;
    async fn execute(&self, task: Task) -> TaskResult;
    fn max_concurrent(&self) -> usize;
}
```

### Built-in Workers

| Worker | Task Types | Concurrency |
|--------|------------|-------------|
| `discovery` | `discovery.scan` | 1 |
| `automation` | `browser.*`, `desktop.*` | 4 |
| `sync` | `sync.*` | 2 |
| `generic` | Any | 8 |

## Task Templates (for Scheduler)

Scheduled tasks use templates:

```rust
struct TaskTemplate {
    task_type: String,
    name: String,
    priority: TaskPriority,
    source: String,
    timeout_secs: u64,
    retry_policy: RetryPolicy,
    payload: Value,
}
```

The Scheduler creates concrete `Task` instances from templates.

## Event Integration

All task lifecycle changes emit events:

| Event | Trigger |
|-------|---------|
| `task.created` | Task queued |
| `task.started` | Worker begins execution |
| `task.completed` | Success result stored |
| `task.failed` | Error stored |
| `task.cancelled` | Explicit cancel |

### Event Payload

```json
{
  "task_id": "uuid",
  "task_type": "discovery.scan",
  "name": "Full Discovery Scan",
  "status": "Running",
  "source": "scheduler",
  "priority": "Normal"
}
```

## Progress Reporting

Long-running tasks can report progress:

```rust
task_manager.update_progress(task_id, 0.5).await?; // 50%
```

Progress is included in `task.started` and `task.completed` events.

## Timeout Handling

- Each task has `timeout_secs`
- Timer starts when task enters `RUNNING`
- On timeout: status → `TIMEOUT`, error set, retry evaluated
- Worker should respect cancellation

## Concurrency Control

- `max_concurrent_tasks` global limit (default: 100)
- Per-worker concurrency limits
- Queue backs up when at capacity
- Priority-based preemption (future)

## Persistence

Task state is not automatically persisted. For durability:
1. Use database-backed Task Manager (future)
2. Implement custom persistence via events
3. Re-create tasks on startup from event log

## Testing

```rust
#[tokio::test]
async fn test_task_lifecycle() {
    let manager = TaskManager::new(None);
    manager.start().await.unwrap();
    
    let task = Task {
        task_type: "test".to_string(),
        name: "Test".to_string(),
        priority: TaskPriority::Normal,
        // ... other fields
    };
    
    let id = manager.create_task(task).unwrap();
    assert_eq!(manager.get_task(id).unwrap().status, TaskStatus::Queued);
    
    manager.update_status(id, TaskStatus::Running).unwrap();
    assert_eq!(manager.get_task(id).unwrap().status, TaskStatus::Running);
    assert!(manager.get_task(id).unwrap().started_at.is_some());
    
    manager.set_result(id, json!({"ok": true})).unwrap();
    assert_eq!(manager.get_task(id).unwrap().status, TaskStatus::Completed);
}
```

## Future Extensions

- **Distributed Tasks**: Worker pool across machines
- **Task Chains**: Explicit workflow definition
- **Cron Tasks**: Scheduled recurring tasks
- **Task Groups**: Atomic multi-task operations
- **Dead Letter Queue**: Failed tasks after max retries
- **Task Prioritization**: Dynamic priority adjustment
- **Resource Quotas**: CPU/memory per task