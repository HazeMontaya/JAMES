# JAMES Core Architecture

## Overview

JAMES Core is the central orchestration layer of the JAMES platform. It provides the foundational infrastructure that all other components (plugins, AI models, devices, UIs) build upon.

## Core Principles

1. **No Hard Dependencies on Concrete Implementations** - Core interfaces are abstract
2. **Event-Driven Communication** - Modules communicate via the Event Bus
3. **Capability-Based Security** - All actions require explicit capabilities
4. **Observable State** - All state changes emit events
5. **Platform Agnostic** - Runs on Windows, Linux, macOS

## Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                      JAMES CORE                             │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  Event   │  │ Registry │  │Capability│  │ Service  │   │
│  │   Bus    │◄─┤          │◄─┤ Registry │  │ Registry │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
│       │             │             │             │          │
│       ▼             ▼             ▼             ▼          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Core Orchestration                       │  │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐  │  │
│  │  │  Task    │ │Scheduler │ │  Health  │ │Diagnost. │  │  │
│  │  │ Manager  │ │          │ │ Monitor  │ │  ics     │  │  │
│  │  └──────────┘ └──────────┘ └──────────┘ └────────┘  │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Lifecycle

```
STARTING → RUNNING → STOPPING → STOPPED
              │
              └─► FAILED (on critical error)
```

### Startup Sequence

1. Load configuration
2. Initialize Event Bus
3. Initialize Registry
4. Initialize Capability Registry (register built-ins)
5. Initialize Service Registry
6. Initialize Task Manager
7. Initialize Scheduler
8. Initialize Health Monitor
8. Initialize Diagnostics
9. Emit `system.started` event
10. Spawn background tasks (health checks, scheduler tick)

### Shutdown Sequence

1. Emit `system.stopping` event
2. Cancel background tasks
3. Stop Scheduler
4. Stop Task Manager (cancel running tasks)
5. Stop Service Registry (stop all services)
6. Stop Capability Registry
7. Stop Registry
8. Stop Event Bus
9. Stop Health Monitor
10. Stop Diagnostics
11. Emit `system.stopped` event

## Configuration

Configuration is loaded from:
1. Default values (compile-time)
2. `james.toml` in config directory
3. Environment variables (`JAMES_*` prefix)

### Key Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `instance_name` | "james-core" | Unique instance identifier |
| `data_dir` | `~/.local/share/james/data` | Persistent data directory |
| `config_dir` | `~/.config/james` | Configuration directory |
| `log_level` | "info" | Logging level |
| `event_bus_buffer_size` | 1000 | Event bus channel buffer |
| `health_check_interval_secs` | 30 | Health check frequency |
| `scheduler_tick_interval_secs` | 1 | Scheduler tick frequency |
| `task_timeout_secs` | 300 | Default task timeout |
| `max_concurrent_tasks` | 100 | Max parallel tasks |

## Event Bus

The Event Bus is the central nervous system of JAMES Core.

### Event Structure

```rust
struct Event {
    event_id: Uuid,           // Unique event identifier (v7 = time-ordered)
    event_type: String,       // e.g., "system.started", "device.discovered"
    timestamp: DateTime<Utc>, // When the event occurred
    source: String,           // Component that emitted the event
    target: Option<String>,   // Optional specific target
    payload: serde_json::Value, // Event data
    correlation_id: Option<Uuid>, // For request/response correlation
    causation_id: Option<Uuid>,   // For event chaining
    severity: EventSeverity,  // Trace/Debug/Info/Warn/Error/Critical
}
```

### Built-in Event Types

| Event | Description |
|-------|-------------|
| `system.started` | Core finished startup |
| `system.stopping` | Core beginning shutdown |
| `system.stopped` | Core fully stopped |
| `device.discovered` | New device found |
| `device.changed` | Device properties changed |
| `device.removed` | Device no longer available |
| `capability.registered` | New capability available |
| `capability.removed` | Capability no longer available |
| `plugin.loaded` | Plugin successfully loaded |
| `plugin.unloaded` | Plugin unloaded |
| `task.created` | New task queued |
| `task.started` | Task began execution |
| `task.completed` | Task finished successfully |
| `task.failed` | Task failed |
| `workflow.started` | Workflow began |
| `workflow.completed` | Workflow finished |
| `service.started` | Service started |
| `service.stopped` | Service stopped |
| `service.health_changed` | Service health status changed |
| `error.occurred` | Error in any component |
| `discovery.scan_started` | Discovery scan began |
| `discovery.scan_completed` | Discovery scan finished |
| `discovery.changes_detected` | Environment changes found |

### Subscriptions

Components can subscribe to:
- Specific event types: `bus.subscribe("device.discovered")`
- All events: `bus.subscribe_all()`

## Registry

The Registry is the single source of truth for all registered entities.

### Entry Types

| Type | Description |
|------|-------------|
| `Service` | Long-running background services |
| `Module` | Core modules |
| `Plugin` | Dynamically loaded plugins |
| `Device` | Discovered devices |
| `Capability` | Available capabilities |
| `Workflow` | Defined workflows |
| `Agent` | Autonomous agents |
| `Model` | AI models |

### Entry Structure

```rust
struct RegistryEntry {
    id: Uuid,
    entry_type: RegistryEntryType,
    name: String,           // Human-readable name
    version: String,        // Semantic version
    provider: String,       // Who provides this
    status: RegistryStatus, // Active/Inactive/Starting/Stopping/Failed/Degraded
    capabilities: Vec<String>, // Capability IDs this entry provides
    dependencies: Vec<Uuid>,   // Other registry entries this depends on
    metadata: Value,       // Arbitrary additional data
    registered_at: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    heartbeat_interval_secs: Option<u64>,
}
```

### Queries

The registry supports flexible queries:
- By type
- By name (partial match)
- By status
- By capability
- By provider

## Capability Registry

Capabilities are the atomic units of permission in JAMES.

### Capability Definition

```rust
struct CapabilityDefinition {
    id: String,                    // e.g., "system.files.read"
    name: String,                  // Human-readable
    category: CapabilityCategory,  // System/File/Process/Network/Browser/Voice/AI/Device/SmartHome/Automation/Database/Security/Custom
    version: String,               // Semantic version
    provider: String,              // Who implements this
    description: String,
    risk_level: RiskLevel,         // Low/Medium/High/Critical
    required_permissions: Vec<String>, // Permission strings required
    dependencies: Vec<String>,     // Other capability IDs required
    input_schema: Option<Value>,   // JSON Schema for input validation
    output_schema: Option<Value>,  // JSON Schema for output validation
    execution_target: ExecutionTarget, // Local/Remote/Container/Wasm/Native
    tags: Vec<String>,
    deprecated: bool,
    experimental: bool,
}
```

### Built-in Capabilities

| ID | Category | Risk | Description |
|----|----------|------|-------------|
| `system.files.read` | File | Low | Read local files |
| `system.files.write` | File | Medium | Write local files |
| `system.process.start` | Process | High | Execute local processes |
| `browser.navigate` | Browser | Medium | Control browser navigation |
| `voice.transcribe` | Voice | Low | Speech-to-text |
| `voice.synthesize` | Voice | Low | Text-to-speech |
| `ai.local.inference` | AI | Medium | Local LLM inference |
| `device.bluetooth.scan` | Device | Low | Scan for BT devices |
| `smart_home.home_assistant.call_service` | SmartHome | Medium | Call HA services |

### Capability Resolution

When a component requests a capability:
1. Check if capability exists in registry
2. Verify status is `Available`
3. Validate input against `input_schema`
4. Check required permissions against caller's permissions
5. Execute via `execution_target`
6. Validate output against `output_schema`
7. Record usage metrics

## Service Registry

Manages the lifecycle of long-running services.

### Service Statuses

```
STARTING → RUNNING ↔ DEGRADED
    │           │
    ▼           ▼
STOPPING → STOPPED
    │
    ▼
FAILED
```

### Restart Policies

| Policy | Behavior |
|--------|----------|
| `Never` | Don't restart |
| `OnFailure` | Restart only on failure |
| `Always` | Always restart |
| `UnlessStopped` | Restart unless explicitly stopped |

### Health Checks

Services can expose a health check endpoint. The Health Monitor polls this periodically.

## Task Manager

Generic task execution infrastructure.

### Task Lifecycle

```
QUEUED → RUNNING → COMPLETED
    │         │
    │         └─► FAILED → (retry) → QUEUED
    │
    └─► CANCELLED
    │
    └─► TIMEOUT
```

### Task Properties

- Unique ID (UUID v7)
- Type and name
- Priority (Low/Normal/High/Critical)
- Timeout
- Retry policy (exponential backoff)
- Dependencies (other task IDs)
- Progress reporting
- Result/Error payloads

## Scheduler

Time-based task scheduling.

### Schedule Types

| Type | Description |
|------|-------------|
| `Immediate` | Run once ASAP |
| `Delayed` | Run once after delay |
| `Interval` | Run repeatedly at fixed interval |
| `Cron` | Run on cron expression |

### Tick Loop

The scheduler runs a tick loop (default 1 second) that:
1. Checks for due tasks
2. Creates Task instances from templates
3. Submits to Task Manager
4. Updates next_run timestamps

## Health Monitor

Continuous health assessment of all core components.

### Checks

| Check | Interval | Critical | Description |
|-------|----------|----------|-------------|
| Core status | 30s | Yes | Core state is Running |
| Event Bus | 30s | Yes | Event bus responsive |
| Registry | 30s | Yes | Registry accessible |
| Services | 30s | No | All critical services running |
| Tasks | 30s | No | No task queue buildup |
| Scheduler | 30s | No | Scheduler ticking |
| Disk Space | 5min | Yes | >5% free space |
| Memory | 60s | Yes | >5% available |

### Output

Health state persisted to `.james/state/health.json`:

```json
{
  "overall": "Healthy",
  "components": {
    "core": { "status": "Healthy", "message": "Core status: Running", "response_time_ms": 1 },
    "disk-space": { "status": "Healthy", "message": "Disk usage: 45.2%", "response_time_ms": 15 }
  },
  "checked_at": "2026-09-16T...",
  "uptime_secs": 3600
}
```

## Diagnostics

CLI and programmatic inspection interface.

### Commands

| Command | Description |
|---------|-------------|
| `james-core status` | Overall status summary |
| `james-core version` | Version info |
| `james-core registry [--type TYPE]` | List registry entries |
| `james-core capabilities [--category CAT]` | List capabilities |
| `james-core events [--limit N]` | Recent events |
| `james-core health` | Health report |
| `james-core export [--format json|yaml]` | Export state |

### Output Formats

- Human-readable tables (default)
- JSON (`--format json`)
- YAML (`--format yaml`)

## Security Boundaries

```
┌─────────────────────────────────────┐
│          JAMES CORE                 │
│  ┌─────────────────────────────┐   │
│  │ Capability Registry         │   │
│  │  - Definitions              │   │
│  │  - Permissions              │   │
│  │  - Schemas                  │   │
│  └─────────────┬────────────────┘   │
│                │                    │
│  ┌─────────────▼────────────────┐   │
│  │ Permission Engine (Phase 4)  │   │
│  │  - Policy Evaluation        │   │
│  │  - Capability Broker        │   │
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
```

### Current Boundaries (Phase 3)

- No arbitrary shell execution
- No credential access
- No network egress without capability
- No filesystem access outside configured directories
- All actions audited via events

### Future Boundaries (Phase 4+)

- WASM sandbox for untrusted code
- Capability-based filesystem namespaces
- Network policy enforcement
- Audit log signing

## Versioning

| Component | Versioning |
|-----------|------------|
| JAMES Core | Semantic (0.1.0) |
| Event Schema | Separate (v1) |
| Registry Schema | Separate (v1) |
| Capability Schema | Per-capability |
| API | Semantic per crate |

## Testing Strategy

| Level | Scope |
|-------|-------|
| Unit | Individual functions, structs |
| Integration | Cross-crate interactions |
| Lifecycle | Startup/shutdown sequences |
| Event Flow | Publish/subscribe patterns |
| Health | Check implementations |
| Scheduler | Timing accuracy |
| Persistence | State save/load |

## Deployment

### Directory Structure

```
.james/
├── config/
│   └── james.toml
├── state/
│   ├── health.json
│   └── core-status.json
├── logs/
│   └── james-core.log
├── secrets/          (Phase 4+)
└── inventory/        (from Phase 2)
```

### Process Model

Single process with async Tokio runtime. All components run as tasks within the same process.

Future: Multi-process with IPC for isolation.