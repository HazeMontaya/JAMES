# JAMES Event System Architecture

## Event Bus Design

The Event Bus is a publish/subscribe messaging system that enables loose coupling between components.

### Properties

- **In-memory** - No external dependencies
- **Ordered** - Events delivered in publish order per subscription
- **At-least-once** - Best effort delivery
- **Async** - Non-blocking publish
- **Typed** - Events carry structured JSON payloads

### Event Format

```json
{
  "event_id": "0192f0c0-0000-7f00-8000-000000000000",
  "event_type": "device.discovered",
  "timestamp": "2026-09-16T10:30:00.123456Z",
  "source": "discovery-adapter",
  "target": null,
  "payload": {
    "device_id": "abc123",
    "device_type": "bluetooth",
    "name": "Living Room Speaker"
  },
  "correlation_id": null,
  "causation_id": null,
  "severity": "Info"
}
```

### Severity Levels

| Level | Value | Use Case |
|-------|-------|----------|
| Trace | 0 | Detailed flow tracing |
| Debug | 1 | Debugging information |
| Info | 2 | General operational info |
| Warn | 3 | Potential issues |
| Error | 4 | Operation failed |
| Critical | 5 | System integrity at risk |

### Event Categories

#### System Events
- `system.started` - Core initialization complete
- `system.stopping` - Graceful shutdown initiated
- `system.stopped` - Shutdown complete
- `error.occurred` - Error in any component

#### Device Events
- `device.discovered` - New device found
- `device.changed` - Device properties updated
- `device.removed` - Device no longer available

#### Capability Events
- `capability.registered` - New capability available
- `capability.removed` - Capability no longer available
- `capability.changed` - Capability definition updated

#### Plugin Events
- `plugin.loaded` - Plugin loaded successfully
- `plugin.unloaded` - Plugin unloaded
- `plugin.error` - Plugin error

#### Task Events
- `task.created` - Task queued
- `task.started` - Task execution began
- `task.completed` - Task finished successfully
- `task.failed` - Task failed
- `task.cancelled` - Task cancelled

#### Workflow Events
- `workflow.started` - Workflow execution began
- `workflow.completed` - Workflow finished
- `workflow.failed` - Workflow failed

#### Service Events
- `service.started` - Service started
- `service.stopped` - Service stopped
- `service.health_changed` - Service health status changed

#### Discovery Events
- `discovery.scan_started` - Discovery scan initiated
- `discovery.scan_completed` - Discovery scan finished
- `discovery.changes_detected` - Changes found in environment

#### Configuration Events
- `config.changed` - Configuration updated
- `registry.changed` - Registry entry added/removed/modified

### Correlation and Causation

Events can be linked for tracing:

```
User Request (correlation_id: abc)
  │
  ├─► Task Created (correlation_id: abc, causation_id: abc)
  │     │
  │     ├─► Service Called (correlation_id: abc, causation_id: task_created)
  │     │     │
  │     │     └─► Device Command (correlation_id: abc, causation_id: service_called)
  │     │
  │     └─► Task Completed (correlation_id: abc, causation_id: task_created)
  │
  └─► Response Sent (correlation_id: abc)
```

### Subscription Patterns

#### Exact Match
```rust
let mut rx = bus.subscribe("device.discovered");
```

#### Wildcard (All Events)
```rust
let mut rx = bus.subscribe_all();
```

#### Multiple Subscriptions
```rust
let mut rx1 = bus.subscribe("device.discovered");
let mut rx2 = bus.subscribe("device.changed");
let mut rx3 = bus.subscribe("device.removed");
// Handle all device events
```

### Delivery Guarantees

- **In-process**: Direct channel send, no loss under normal conditions
- **Backpressure**: Unbounded channels, but slow consumers may cause memory growth
- **Ordering**: Per-subscription FIFO ordering guaranteed
- **Duplicates**: Possible during restart/recovery scenarios

### Performance Considerations

- Event creation allocates: use `Event::new()` builder pattern
- Payload serialization: use `serde_json::json!` macro
- High-frequency events: consider batching or sampling
- Subscriber count: each subscriber gets a clone of the event

### Testing Events

```rust
#[tokio::test]
async fn test_event_flow() {
    let bus = EventBus::new(100);
    bus.start().await.unwrap();
    
    let mut rx = bus.subscribe("test.event");
    
    bus.publish(Event::new("test.event", "test")
        .with_payload(json!({"key": "value"}))).await.unwrap();
    
    let received = rx.recv().await.unwrap();
    assert_eq!(received.event.event_type, "test.event");
    assert_eq!(received.event.payload["key"], "value");
}
```

### Event Schema Evolution

Events are versioned implicitly through their `event_type` string:

- `device.discovered.v1` - Original format
- `device.discovered.v2` - Added new fields

Consumers should:
1. Handle unknown fields gracefully (serde default)
2. Check for required fields
3. Log warnings for deprecated event types

### Security

- Events contain no credentials, tokens, or secrets
- `security.redact_sensitive` config option scrubs known patterns
- External event ingestion requires explicit adapter
- Event bus not exposed externally by default