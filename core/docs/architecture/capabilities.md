# JAMES Capability System Architecture

## Overview

Capabilities are the atomic units of authorization in JAMES. Every action that interacts with the outside world requires an explicit capability.

## Capability Model

### Definition Structure

```rust
struct CapabilityDefinition {
    id: String,                    // Unique identifier (reverse domain style)
    name: String,                  // Human-readable name
    category: CapabilityCategory,  // Classification
    version: String,               // Semantic version
    provider: String,              // Implementation provider
    description: String,           // What this capability does
    risk_level: RiskLevel,         // Low/Medium/High/Critical
    required_permissions: Vec<String>,  // Permission strings
    dependencies: Vec<String>,     // Other capability IDs required
    input_schema: Option<Value>,   // JSON Schema for input
    output_schema: Option<Value>,  // JSON Schema for output
    execution_target: ExecutionTarget, // Where it runs
    tags: Vec<String>,             // Searchable tags
    deprecated: bool,              // Deprecated flag
    experimental: bool,            // Experimental flag
}
```

### Categories

| Category | Description | Examples |
|----------|-------------|----------|
| `System` | Core system operations | files, processes, config |
| `File` | Filesystem operations | read, write, list, watch |
| `Process` | Process management | start, stop, signal |
| `Network` | Network operations | http, tcp, dns, mdns |
| `Browser` | Browser automation | navigate, click, screenshot |
| `Voice` | Audio I/O | transcribe, synthesize |
| `Ai` | AI/ML operations | inference, embed, train |
| `Device` | Hardware devices | bluetooth, usb, camera |
| `SmartHome` | Home automation | lights, climate, sensors |
| `Automation` | UI/OS automation | click, type, window |
| `Database` | Data persistence | query, migrate, backup |
| `Security` | Security operations | encrypt, sign, auth |
| `Custom(String)` | Extensible category | domain-specific |

### Risk Levels

| Level | Description | Approval Required |
|-------|-------------|-------------------|
| `Low` | Read-only, no side effects | Implicit |
| `Medium` | State changes, limited scope | User consent |
| `High` | System-wide effects, execution | Explicit approval |
| `Critical` | Destructive, irreversible | Admin + confirmation |

### Execution Targets

| Target | Description | Use Case |
|--------|-------------|----------|
| `Local` | Same process/host | Fast, trusted |
| `Remote(host)` | Network service | Distributed |
| `Container(image)` | Isolated container | Untrusted code |
| `Wasm(module)` | WebAssembly sandbox | Plugin isolation |
| `Native` | Direct syscall | Performance critical |

## Permission Model

### Permission Strings

Permissions are simple strings that map to capabilities:

```rust
// Capability requires these permissions
required_permissions: vec![
    "filesystem.read",
    "filesystem.write",
    "network.egress",
]
```

### Permission Evaluation

```
Request Capability
       │
       ▼
┌──────────────────┐
│ Capability Exists?│──No──► DENY
└────────┬─────────┘
         │ Yes
         ▼
┌──────────────────┐
│ Status = Available?│──No──► DENY
└────────┬─────────┘
         │ Yes
         ▼
┌──────────────────┐
│ Dependencies Met? │──No──► DENY
└────────┬─────────┘
         │ Yes
         ▼
┌──────────────────┐
│ Caller Has Permissions? │──No──► DENY
└────────┬─────────┘
         │ Yes
         ▼
┌──────────────────┐
│ Validate Input Schema │──Fail──► DENY
└────────┬─────────┘
         │ Pass
         ▼
      EXECUTE
         │
         ▼
┌──────────────────┐
│ Validate Output Schema │──Fail──► ERROR
└────────┬─────────┘
         │ Pass
         ▼
      RETURN RESULT
```

## Built-in Capabilities

### System Capabilities

| ID | Name | Risk | Permissions | Description |
|----|------|------|-------------|-------------|
| `system.files.read` | Read Files | Low | `filesystem.read` | Read any file in allowed paths |
| `system.files.write` | Write Files | Medium | `filesystem.write` | Write files in allowed paths |
| `system.files.list` | List Directory | Low | `filesystem.read` | List directory contents |
| `system.files.watch` | Watch Files | Low | `filesystem.read` | File change notifications |
| `system.process.start` | Start Process | High | `process.execute` | Launch subprocesses |
| `system.process.stop` | Stop Process | High | `process.execute` | Terminate processes |
| `system.config.read` | Read Config | Low | `config.read` | Read JAMES config |
| `system.config.write` | Write Config | High | `config.write` | Modify JAMES config |

### Network Capabilities

| ID | Name | Risk | Permissions | Description |
|----|------|------|-------------|-------------|
| `network.http.get` | HTTP GET | Medium | `network.egress` | Make HTTP GET requests |
| `network.http.post` | HTTP POST | Medium | `network.egress` | Make HTTP POST requests |
| `network.tcp.connect` | TCP Connect | High | `network.egress` | Raw TCP connections |
| `network.dns.resolve` | DNS Resolve | Low | `network.egress` | DNS lookups |
| `network.mdns.browse` | mDNS Browse | Low | `network.local` | Service discovery |

### Browser Capabilities

| ID | Name | Risk | Permissions | Description |
|----|------|------|-------------|-------------|
| `browser.navigate` | Navigate | Medium | `browser.control` | Go to URL |
| `browser.click` | Click Element | Medium | `browser.control` | Click on page |
| `browser.type` | Type Text | Medium | `browser.control` | Input text |
| `browser.screenshot` | Screenshot | Low | `browser.control` | Capture page |
| `browser.evaluate` | Execute JS | High | `browser.control` | Run JavaScript |
| `browser.cookies` | Manage Cookies | High | `browser.cookies` | Cookie access |

### Voice Capabilities

| ID | Name | Risk | Permissions | Dependencies |
|----|------|------|-------------|--------------|
| `voice.transcribe` | Speech-to-Text | Low | `audio.input` | `ai.local.inference` |
| `voice.synthesize` | Text-to-Speech | Low | `audio.output` | `ai.local.inference` |
| `voice.record` | Record Audio | Medium | `audio.input` | - |
| `voice.play` | Play Audio | Low | `audio.output` | - |

### AI Capabilities

| ID | Name | Risk | Permissions | Description |
|----|------|------|-------------|-------------|
| `ai.local.inference` | Local Inference | Medium | `ai.inference` | Run local LLM |
| `ai.embed` | Generate Embeddings | Low | `ai.inference` | Text embeddings |
| `ai.classify` | Classify Text | Low | `ai.inference` | Text classification |
| `ai.summarize` | Summarize Text | Low | `ai.inference` | Text summarization |
| `ai.code.generate` | Generate Code | Medium | `ai.inference` | Code generation |

### Device Capabilities

| ID | Name | Risk | Permissions | Description |
|----|------|------|-------------|-------------|
| `device.bluetooth.scan` | Scan Bluetooth | Low | `bluetooth.scan` | Discover BT devices |
| `device.bluetooth.connect` | Connect Bluetooth | Medium | `bluetooth.connect` | Pair/connect |
| `device.usb.list` | List USB | Low | `usb.read` | Enumerate USB |
| `device.camera.capture` | Camera Capture | High | `camera.access` | Take photo/video |
| `device.audio.input` | Audio Input | Medium | `audio.input` | Microphone |
| `device.audio.output` | Audio Output | Low | `audio.output` | Speakers |

### Smart Home Capabilities

| ID | Name | Risk | Permissions | Description |
|----|------|------|-------------|-------------|
| `smart_home.light.control` | Control Lights | Medium | `smart_home.control` | On/off/brightness |
| `smart_home.climate.control` | Climate Control | Medium | `smart_home.control` | Thermostat |
| `smart_home.sensor.read` | Read Sensors | Low | `smart_home.read` | Temperature, motion |
| `smart_home.scene.activate` | Activate Scene | Medium | `smart_home.control` | Run scene |
| `smart_home.home_assistant.call_service` | HA Service | Medium | `smart_home.control` | Call HA service |

## Schema Validation

### Input Validation

Every capability can define an input schema (JSON Schema Draft 7):

```json
{
  "type": "object",
  "properties": {
    "path": { "type": "string", "minLength": 1 },
    "encoding": { "type": "string", "enum": ["utf-8", "base64"], "default": "utf-8" }
  },
  "required": ["path"],
  "additionalProperties": false
}
```

### Output Validation

Output schemas ensure consistent responses:

```json
{
  "type": "object",
  "properties": {
    "content": { "type": "string" },
    "size": { "type": "integer", "minimum": 0 },
    "modified_at": { "type": "string", "format": "date-time" }
  },
  "required": ["content"],
  "additionalProperties": false
}
```

### Validation Behavior

- **Input invalid**: Request rejected before execution
- **Output invalid**: Execution error, result not returned
- **No schema**: Validation skipped (backward compatible)

## Capability Discovery

### Registration

Capabilities are registered at startup:

```rust
capability_registry.register(CapabilityDefinition {
    id: "my.custom.capability".to_string(),
    // ...
}, "my-plugin").await?;
```

### Querying

```rust
// All capabilities
registry.list_all()

// By category
registry.list_by_category(CapabilityCategory::File)

// By provider
registry.list_by_provider("james-core")

// Available only
registry.list_available()

// Get definition
registry.get_definition("system.files.read")
```

### Schema Introspection

```rust
// Get JSON Schema for capability
registry.get_input_schema("system.files.read")
registry.get_output_schema("system.files.read")

// Validate without executing
registry.validate_input("system.files.read", &input_json)
```

## Capability Composition

### Dependency Chains

Capabilities can depend on other capabilities:

```
voice.transcribe
    │
    ├─► ai.local.inference (required)
    │       │
    │       └─► (no dependencies)
    │
    └─► audio.input (permission)
```

### Aggregated Capabilities

Higher-level capabilities can compose lower-level ones:

```
smart_home.morning_routine
    ├─► smart_home.light.control
    ├─► smart_home.climate.control
    └─► voice.synthesize (announcement)
```

## Lifecycle

```
REGISTERED → AVAILABLE
    │
    ├─► DEPRECATED (still works, warning logged)
    │
    ├─► DISABLED (explicitly turned off)
    │
    └─► UNREGISTERED (removed)
```

## Auditing

All capability executions are logged via events:

```json
{
  "event_type": "capability.executed",
  "source": "capability-broker",
  "payload": {
    "capability_id": "system.files.read",
    "caller": "plugin:automation",
    "input_hash": "sha256:...",
    "duration_ms": 15,
    "success": true
  },
  "severity": "Info"
}
```

Failed executions:
```json
{
  "event_type": "capability.failed",
  "source": "capability-broker",
  "payload": {
    "capability_id": "system.process.start",
    "caller": "plugin:automation",
    "error": "Permission denied: process.execute",
    "duration_ms": 2
  },
  "severity": "Error"
}
```

## Best Practices

### Designing Capabilities

1. **Single Responsibility** - One capability = one logical action
2. **Least Privilege** - Minimal required permissions
3. **Explicit Schemas** - Define input/output schemas
3. **Clear Risk Level** - Honest risk assessment
4. **Versioning** - Semantic version for breaking changes
5. **Documentation** - Clear description and examples

### Capability Naming

```
{category}.{resource}.{action}
```

Examples:
- `system.files.read`
- `network.http.post`
- `browser.dom.click`
- `voice.audio.transcribe`
- `device.bluetooth.scan`

### Testing Capabilities

```rust
#[tokio::test]
async fn test_file_read_capability() {
    let registry = CapabilityRegistry::new();
    registry.start().await.unwrap();
    
    // Valid input
    let valid = registry.validate_input("system.files.read", &json!({
        "path": "/allowed/file.txt"
    }));
    assert!(valid.is_ok());
    
    // Invalid input (missing required field)
    let invalid = registry.validate_input("system.files.read", &json!({}));
    assert!(invalid.is_err());
}
```

## Future Extensions

- **Capability Marketplace** - Share/load capabilities
- **Dynamic Loading** - WASM-based capability plugins
- **Policy Engine** - OPA/Rego for complex rules
- **Capability Delegation** - Temporary grants
- **Rate Limiting** - Per-capability quotas
- **Cost Tracking** - Resource usage per capability