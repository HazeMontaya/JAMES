# JAMES Security Boundaries

## Overview

This document defines the security boundaries and trust model for JAMES Core. Security is implemented in layers, with Phase 3 establishing the foundation and Phase 4+ adding enforcement mechanisms.

## Threat Model

### Assets to Protect

| Asset | Sensitivity | Impact of Compromise |
|-------|-------------|---------------------|
| User data (files, credentials) | Critical | Privacy violation, identity theft |
| System configuration | High | Unauthorized changes, persistence |
| AI model access | Medium | Cost, misuse, bias |
| Device control | High | Physical safety, privacy |
| Network access | Medium | Lateral movement, data exfiltration |
| Automation capabilities | High | Unauthorized actions |

### Threat Actors

| Actor | Capability | Motivation |
|-------|------------|------------|
| Malicious plugin | Code execution in JAMES | Data theft, system control |
| Compromised dependency | Supply chain | Persistence, lateral movement |
| Malicious user | Local access | Privilege escalation |
| Network attacker | Remote access | Initial foothold |
| Insider | Authorized access | Data exfiltration |

## Security Layers

### Layer 1: Process Isolation (Phase 3+)

```
┌─────────────────────────────────────────┐
│            JAMES CORE PROCESS           │
│  ┌──────────┐ ┌──────────┐ ┌────────┐  │
│  │  Core    │ │ Plugins  │ │ Worker │  │
│  │ Threads  │ │ (same    │ │ Threads│  │
│  │          │ │ process) │ │        │  │
│  └──────────┘ └──────────┘ └────────┘  │
└─────────────────────────────────────────┘
```

**Current State**: All components run in single process
- Plugins share memory space with core
- No memory isolation between components
- Crash in plugin = crash in core

**Risk**: High - Plugin compromise = full core compromise

### Layer 2: Capability-Based Access Control (Phase 4)

```
┌─────────────────────────────────────────┐
│           CAPABILITY BROKER             │
│  ┌──────────────────────────────────┐  │
│  │ Permission Engine                │  │
│  │ - Policy Evaluation (OPA/Rego)   │  │
│  │ - Capability Registry            │  │
│  │ - Audit Logging                  │  │
│  └──────────────────────────────────┘  │
└─────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│         EXECUTION SANDBOXES             │
│  ┌──────────┐ ┌──────────┐ ┌────────┐  │
│  │  WASM    │ │Container │ │ Native │  │
│  │ Sandbox  │ │ Runtime  │ │ (trusted)│
│  └──────────┘ └──────────┘ └────────┘  │
└─────────────────────────────────────────┘
```

### Layer 3: Network Boundaries (Phase 4+)

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   JAMES     │────►│  EGRESS     │────►│  INTERNET   │
│   CORE      │     │  PROXY      │     │             │
└─────────────┘     │  (allowlist)│     └─────────────┘
                    └─────────────┘
                          │
                    ┌─────▼─────┐
                    │  LOCAL    │
                    │  NETWORK  │
                    └───────────┘
```

### Layer 4: Filesystem Boundaries (Phase 4+)

```
┌─────────────────────────────────────────┐
│           FILESYSTEM NAMESPACES         │
│  ┌─────────────┐ ┌─────────────────┐   │
│  │  Core R/W   │ │  Plugin R/O     │   │
│  │  .james/    │ │  (config only)  │   │
│  └─────────────┘ └─────────────────┘   │
│  ┌─────────────┐ ┌─────────────────┐   │
│  │  Data R/W   │ │  System R/O     │   │
│  │  (allowed)  │ │  (denied)       │   │
│  └─────────────┘ └─────────────────┘   │
└─────────────────────────────────────────┘
```

## Current Security Controls (Phase 3)

### Implemented

| Control | Implementation | Coverage |
|---------|---------------|----------|
| Input Validation | JSON Schema on capabilities | All capability inputs |
| Output Validation | JSON Schema on capabilities | All capability outputs |
| Audit Logging | Event Bus events | All capability executions |
| No Credentials in Logs | Config `security.redact_sensitive` | All logging |
| No Hardcoded Secrets | Config file + env vars | Configuration |
| Dependency Verification | Cargo.lock + `cargo audit` | Build pipeline |

### Not Yet Implemented

| Control | Planned Phase | Notes |
|---------|---------------|-------|
| Process Isolation | 4+ | WASM/container sandbox |
| Capability Enforcement | 4 | Permission Engine |
| Network Egress Control | 4+ | Egress proxy |
| Filesystem Namespaces | 4+ | Per-plugin views |
| Secret Management | 4+ | Vault/keyring integration |
| Attestation | 5+ | Binary verification |

## Capability Security Model

### Capability Definition Security

```rust
CapabilityDefinition {
    // Security-relevant fields
    risk_level: RiskLevel,           // Enforced by Permission Engine
    required_permissions: Vec<String>, // Checked at execution time
    dependencies: Vec<String>,       // Transitive closure validated
    input_schema: Option<Value>,     // Validated before execution
    output_schema: Option<Value>,    // Validated after execution
    execution_target: ExecutionTarget, // Determines isolation level
}
```

### Execution Target Security

| Target | Isolation | Trust Level | Use Case |
|--------|-----------|-------------|----------|
| `Native` | None | Full | Core, trusted plugins |
| `Local` | Process | High | Trusted services |
| `Container(image)` | Linux namespaces | Medium | Untrusted services |
| `Wasm(module)` | WASM sandbox | Low | Plugins, user code |
| `Remote(host)` | Network | Variable | External services |

## Data Flow Security

### Configuration

```
User Input → Config Validation → TOML Parse → Structured Config
                    │
                    └─► Schema validation (config-rs)
                    │
                    └─► No secrets in config file
                    │
                    └─► Secrets via env vars / keyring
```

### Event Flow

```
Component → Event Creation → Event Bus → Subscribers
                    │
                    └─► No PII in events
                    │
                    └─► No credentials in payload
                    │
                    └─► Correlation IDs for tracing
```

### Capability Execution

```
Request → Capability Registry → Permission Engine → Executor
                    │                    │              │
                    │                    │              └─► Sandbox
                    │                    └─► Policy check
                    └─► Schema validation
```

## Trust Boundaries

### Trusted Components (Full Access)

- JAMES Core binaries (signed, verified)
- Core crates: `james-core`, `james-events`, `james-registry`, etc.
- System services registered at startup

### Semi-Trusted Components (Capability-Limited)

- Official plugins (signed, reviewed)
- User-installed plugins (explicit approval)
- Local AI models (via capability)

### Untrusted Components (Sandboxed)

- Third-party plugins (WASM sandbox)
- User scripts (WASM sandbox)
- Remote services (network boundary)

## Configuration Security

### Sensitive Configuration

```toml
# james.toml - NO SECRETS HERE
[core]
instance_name = "james-core"
data_dir = "~/.local/share/james/data"
log_level = "info"

[security]
no_credentials = true
no_passwords = true
no_cookies = true
no_external_scan = true
redact_sensitive = true

# Secrets via environment:
# JAMES_DB_PASSWORD=xxx
# JAMES_API_KEY=xxx
```

### Environment Variables

```bash
# Only these env vars are read:
JAMES_*           # Core config overrides
JAMES_DB_PASSWORD # Database password
JAMES_API_KEY     # External API keys
JAMES_SECRET_KEY  # Signing/encryption keys
```

## Audit Requirements

### Mandatory Audit Events

| Event | Fields |
|-------|--------|
| `capability.executed` | capability_id, caller, input_hash, duration_ms, success |
| `capability.failed` | capability_id, caller, error, duration_ms |
| `config.changed` | key, old_value_hash, new_value_hash |
| `registry.changed` | action, entry_id, entry_type |
| `plugin.loaded` | plugin_id, version, signature |
| `plugin.unloaded` | plugin_id, reason |
| `system.started/stopped` | version, instance_id |
| `error.occurred` | component, error, context_hash |

### Log Integrity

- Structured JSON logging
- Correlation IDs for request tracing
- Log rotation with retention
- Future: Signed log entries

## Incident Response

### Detection

- Health Monitor alerts on anomalies
- Event Bus pattern matching
- Capability execution rate limiting

### Containment

- Disable capability: `capability_registry.update_status(id, Disabled)`
- Stop service: `service_registry.stop_service(id)`
- Cancel tasks: `task_manager.cancel_task(id)`
- Emergency shutdown: `core.stop()`

### Recovery

- Restart from clean state
- Replay events from log
- Verify registry integrity
- Validate capability schemas

## Compliance Considerations

### Data Protection

- No personal data in core events
- Capability schemas can mark fields as PII
- Data retention configurable per capability
- Right to deletion via capability `system.files.delete`

### Access Control

- Role-based access planned for Phase 4
- Current: Capability = permission
- Future: User/role → capability mapping

### Audit Trail

- Immutable event log (append-only)
- Future: Cryptographic chaining
- Export for compliance reporting

## Security Checklist for Contributors

### Code Review

- [ ] No hardcoded secrets
- [ ] Input validation on all public APIs
- [ ] Output validation on all capability returns
- [ ] No direct filesystem access (use capabilities)
- [ ] No direct network access (use capabilities)
- [ ] No direct process execution (use capabilities)
- [ ] Errors don't leak sensitive info
- [ ] Dependencies pinned and audited

### Testing

- [ ] Unit tests for validation logic
- [ ] Integration tests for capability flow
- [ ] Fuzzing for schema parsers
- [ ] Penetration testing for sandbox escapes

### Deployment

- [ ] Binary signed and verified
- [ ] Config file permissions 600
- [ ] Secrets in environment/keyring
- [ ] Network egress restricted
- [ ] Monitoring/alerting configured