# JAMES Registry Architecture

## Overview

The Registry is the central directory of all entities in the JAMES system. It provides a unified interface for registering, discovering, and managing services, devices, capabilities, and other components.

## Entry Types

| Type | Description | Examples |
|------|-------------|----------|
| `Service` | Long-running background processes | discovery, database, voice, llm |
| `Module` | Core system modules | core, events, registry |
| `Plugin` | Dynamically loaded extensions | automation, browser, smart-home |
| `Device` | Physical or virtual devices | bluetooth-speaker, camera, sensor |
| `Capability` | Atomic permissions | system.files.read, ai.local.inference |
| `Workflow` | Defined automation sequences | morning-routine, backup-job |
| `Agent` | Autonomous actors | chat-agent, monitor-agent |
| `Model` | AI models | llama3.2, whisper-base, nomic-embed |

## Registration Process

```
Component                    Registry
    │                          │
    ├─► register(entry) ─────►│
    │                          ├─► Validate uniqueness (name)
    │                          ├─► Assign UUID v7 if needed
    │                          ├─► Set timestamps
    │                          ├─► Store in indexes
    │                          ├─► Emit registry.changed event
    │                          │
    │◄──────── id ─────────────┤
    │                          │
```

### Required Fields

| Field | Description | Validation |
|-------|-------------|------------|
| `id` | UUID v7 (auto-generated if nil) | Unique |
| `entry_type` | One of 8 types | Required |
| `name` | Human-readable identifier | Unique per type |
| `version` | Semantic version string | Required |
| `provider` | Origin identifier | Required |
| `status` | Active/Inactive/Starting/Stopping/Failed/Degraded/Unknown | Default: Active |
| `capabilities` | List of capability IDs | Optional |
| `dependencies` | List of registry entry UUIDs | Optional |
| `metadata` | Arbitrary JSON | Optional |

### Duplicate Handling

Registration fails if an entry with the same `name` already exists (regardless of type). This ensures global name uniqueness.

## Indexes

The registry maintains multiple indexes for efficient querying:

| Index | Key | Value |
|-------|-----|-------|
| Primary | `Uuid` | `RegistryEntry` |
| By Name | `String` | `Uuid` |
| By Type | `RegistryEntryType` | `Vec<Uuid>` |

## Query API

### Basic Queries

```rust
// Get by ID
registry.get(id)

// Get by name (exact match)
registry.get_by_name("my-service")

// List all
registry.list_all()

// List by type
registry.list_by_type(RegistryEntryType::Service)
```

### Flexible Queries

```rust
registry.query(RegistryQuery {
    entry_type: Some(RegistryEntryType::Service),
    name: Some("api"),           // partial match
    status: Some(RegistryStatus::Active),
    capability: Some("ai.local.inference"),
    provider: Some("james-core"),
})
```

## Event Integration

Every registry modification emits a `registry.changed` event:

```json
{
  "event_type": "registry.changed",
  "source": "registry",
  "payload": {
    "action": "registered|unregistered|modified",
    "entry_id": "uuid",
    "entry_type": "Service",
    "name": "my-service"
  }
}
```

Subscribers can react to registry changes in real-time.

## Heartbeat Mechanism

Long-lived entries can register a heartbeat interval:

```rust
entry.heartbeat_interval_secs = Some(30);
```

The registry doesn't actively monitor heartbeats, but:
- `update_heartbeat(id)` updates `last_seen`
- `cleanup_stale(max_age)` removes entries older than threshold
- Components should call heartbeat periodically

## Status Transitions

```
         ┌─────────────┐
         │  Unknown    │
         └──────┬──────┘
                │ register()
                ▼
         ┌─────────────┐
         │   Active    │◄──────────────┐
         └──────┬──────┘               │
                │                       │
      ┌─────────┼─────────┐            │
      ▼         ▼         ▼            │
┌──────────┐ ┌────────┐ ┌────────┐     │
│ Starting │ │Failed  │ │Degraded│     │
└────┬─────┘ └────┬───┘ └────┬───┘     │
     │            │            │        │
     ▼            ▼            ▼        │
┌──────────┐ ┌────────┐ ┌────────┐     │
│ Active   │ │ Stopped│ │ Active │     │
└──────────┘ └────┬───┘ └────┬───┘     │
                  │          │          │
         unregister/        heartbeat/
         stop()             update_heartbeat()
                  │          │          │
                  ▼          ▼          │
            ┌────────────────────┐      │
            │    Stopped/Failed  │──────┘
            └────────────────────┘
```

## Persistence

Registry state is not automatically persisted. Components should:
1. Re-register on startup
2. Use the Event Bus to sync state
3. Implement their own persistence if needed

Future: Optional Redis/PostgreSQL backend for distributed deployments.

## Best Practices

### Naming Conventions

- Services: `kebab-case` (e.g., `voice-service`)
- Devices: `type-location` (e.g., `bluetooth-living-room`)
- Capabilities: `category.action` (e.g., `system.files.read`)
- Modules: `james-*` prefix for core modules

### Versioning

Use semantic versioning: `MAJOR.MINOR.PATCH`

### Metadata Guidelines

Include useful operational metadata:
```json
{
  "endpoint": "http://localhost:8080",
  "health_check": "/health",
  "config": { "key": "value" },
  "tags": ["production", "critical"]
}
```

### Dependency Declaration

Declare dependencies explicitly:
```rust
dependencies: vec![
    uuid::parse("other-service-uuid").unwrap()
]
```

This enables:
- Topological startup ordering
- Impact analysis for changes
- Automated health checks

## Error Handling

| Error | Cause | Resolution |
|-------|-------|------------|
| `DuplicateName` | Name already registered | Use unique name or unregister first |
| `NotFound` | Entry doesn't exist | Check ID/name spelling |
| `DependencyMissing` | Referenced UUID not in registry | Register dependency first |
| `InvalidTransition` | Illegal status change | Follow state machine |

## Testing

```rust
#[test]
fn test_registry_basic() {
    let registry = Registry::new();
    
    let entry = RegistryEntry {
        name: "test-service".to_string(),
        // ... other fields
    };
    
    let id = registry.register(entry).unwrap();
    assert!(registry.get(id).is_some());
    assert!(registry.get_by_name("test-service").is_some());
    
    registry.unregister(id).unwrap();
    assert!(registry.get(id).is_none());
}
```

## Future Extensions

- **Distributed Registry**: Redis/etcd backend for multi-node
- **Watch API**: Real-time change notifications
- **Transactions**: Batch register/unregister
- **ACL**: Per-entry access control
- **Audit Log**: Immutable registration history