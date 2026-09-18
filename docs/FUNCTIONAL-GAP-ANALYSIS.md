# JAMES Functional Gap Analysis

Status: current assessment
Date: 2026-09-17
Scope: reachable `S:\JAMES` workspace

## Executive result

JAMES is not yet a 100% usable product. The verified usable slice is:

```text
Windows source workspace
-> Core lifecycle
-> First-party static assembly
-> Local Ollama-compatible AI provider
-> Chat via stdin and localhost API
-> JSON memory and task persistence
-> memory.read through Capability Broker
-> event-based audit path
```

The next work is implementation, not more concept documents.

## Verification status

| Area | Status | Evidence / blocker |
|---|---|---|
| Core start/stop | VERIFIED | Rust lifecycle tests and runtime smoke test |
| Zero-module boot | VERIFIED | Core starts with zero concrete capabilities |
| Static first-party assembly | VERIFIED | Module workspace tests and smoke test |
| Chat transport | VERIFIED | stdin and localhost handler path |
| Local AI | PARTIAL | Ollama provider exists; availability/model health is not managed as a runtime state |
| Memory read | VERIFIED | Assembly broker executor and integration test |
| Tasks | PARTIAL | JSON persistence and lifecycle exist; durable recovery and full workflow execution do not |
| Capability Broker | PARTIAL | Real executor path exists for memory.read; most module actions bypass it |
| Capability Resolver | MISSING | No general resolver/provider selection API exists |
| Agent runtime | MISSING | No dedicated James-Agents module or structured plan executor |
| Workflow engine | MISSING | No James-Workflow module |
| Dynamic module loading | MISSING | Native loader returns mock handle; process/WASM loaders are unimplemented |
| Authentication | MISSING | Local API is loopback-only but unauthenticated |
| Confirmation flow | MISSING | ASK exists as a broker error, no persistent user confirmation endpoint/state |
| Audit persistence | PARTIAL | Audit/event structures exist; durable restart-safe audit integration is incomplete |
| Identity enforcement | PARTIAL | Identity registry exists; broker does not require verified Identity for every request |
| Windows platform layer | PARTIAL | Discovery uses Windows adapters; Core platform ports are not implemented as a dedicated crate |
| Discovery integration | PARTIAL | TypeScript tool works separately; Core does not consume an Environment Model |
| Browser automation | NOT USABLE | `navigate` performs HTTP GET; screenshot/extract capabilities have no real executors |
| Web research | PARTIAL | DuckDuckGo/fetch methods exist; no broker path, timeout enforcement or robust source trust model |
| Voice/STT/TTS | NOT VERIFIED | Modules start, but real device/provider execution is not proven |
| Void renderer | IMPLEMENTED | Black/gold renderer (`interfaces/void`) with Canvas Brain, chat, dashboard tabs, drawer, API and WebSocket wiring; 18-brain-state fabric; consumes orchestrator `ui.intent` over `/ws/v1/events`; security seal reflects live ALLOW/ASK/DENY decisions; telemetry-raised activity respects the "resources never claim a cognitive state" contract |
| UI orchestrator | IMPLEMENTED | `core/crates/james-ui` projects real bus state (task/security/ai/plan/telemetry) into declarative `UiIntent`; publishes `ui.intent` and serves `GET /api/v1/ui/intent`; 8 unit tests + 2 API endpoint tests pass; live smoke test returns a real idle snapshot |
| 2-way Void socket | IMPLEMENTED | `/ws/v1/void` routes `void.action` through CapabilityBroker (PlatformCapabilityExecutor with 27 capabilities); `void.action_result`/`void.action_error` events flow back; broker wired with PlatformCapabilityExecutor; audit via existing broker event publishing |
| Semantic motion profiles | IMPLEMENTED | 16 state-specific flows (LISTENING input→core, THINKING core↔reason, EXECUTING core→cap→browser→output, VERIFYING output→verify→core, SECURITY_LOCK ring, ERROR isolation, RECOVERING reconnection); state-specific core pulse; active-zone filtering; prefers-reduced-motion respected |
| Context priority motion | IMPLEMENTED | BACKGROUND→NORMAL→IMPORTANT→URGENT→CRITICAL with position/size/duration semantics; pulseGlow for unresolved, fadeOut for momentary |
| API authentication | IMPLEMENTED | Bearer token (`Authorization: Bearer <token>` / `?token=`); EnvTokenStore + WindowsTokenStore; dev mode allows loopback with visible warning; correlation IDs; 4 auth tests + 14 app-api tests green |
| Dashboard | PARTIAL | Snapshot/module API exists; it is not yet the adaptive Void presentation system |
| Discovery tests | PARTIAL | Build passes, 30 tests pass; 22 live/hardware tests are skipped |
| npm dependencies | WARNING | `npm audit` reports six high-severity dependency vulnerabilities |
| JARVIS/AUTOMATON migration | BLOCKED | Repositories are not present or accessible under `S:\` |

## P0: required for first genuinely usable PC product

### P0.1 Reproducible toolchain

The current fresh Rust rebuild is blocked by missing `dlltool.exe` after build artifacts were cleaned. Fix the Windows Rust linker/toolchain, then verify from a clean workspace:

```text
core: cargo test --workspace
modules: cargo test --workspace
tools/discovery: npm test
```

Acceptance: a new checkout can build and test without relying on old `out/` artifacts.

### P0.2 Secure local API

Implement:

- bearer/session authentication
- token from a supported secret backend, never source or config plaintext
- authentication for REST and WebSocket routes
- explicit development mode with visible warning
- loopback default retained
- request identity and correlation IDs

Acceptance: unauthenticated chat, WebSocket and control requests are rejected; authenticated requests work.

### P0.3 Agent/plan/tool execution

Implement a structured request model:

```text
UserIntent -> Plan -> PlanStep -> CapabilityRequest
-> Resolver -> Broker -> Executor -> Verification -> Result
```

The model must not emit executable shell text as an authority. Start with `memory.read`, then add one read-only web capability.

Acceptance: an AI/tool plan produces a typed CapabilityRequest, the broker can deny it, execute it through a registered executor, validate the result and emit audit/events.

### P0.4 Real module capability executors

Every advertised capability must be either:

- backed by a tested executor,
- marked unavailable/degraded,
- or removed from the advertised registration.

Currently advertised-but-not-backed examples include browser screenshot/extract and several voice/device capabilities.

Acceptance: no registered capability silently points to a missing implementation.

### P0.5 Usable Void surface

The first renderer is implemented under `interfaces/void` with:

- chat input/output
- Core/module/capability health projections
- task and memory dashboard projections
- security/context drawer foundation
- Live Brain state display foundation
- black/gold responsive visual language
- Canvas Brain, semantic display states and reduced-motion support
- chat form, dashboard tabs, context drawer and live event client

Remaining work is server integration verification and real event-driven Brain transitions.

Acceptance: `james-system` opens a working local Void surface and a user can chat, inspect state and approve/deny a safe action.

## P1: required for reliable daily use

- persistent Storage abstraction used by Identity, Registry, Audit, Tasks and Memory
- restart recovery for queued/running tasks
- real audit chain persistence and redaction
- identity verification enforced by broker
- policy and confirmation persistence
- provider health, model discovery and offline/error states
- real Windows platform ports for filesystem, processes, applications, audio and devices
- broker executors for read-only filesystem, process inspection and controlled browser navigation
- request timeouts, cancellation and rate limits for all network calls
- integration of TypeScript Discovery into a versioned Core Environment Snapshot
- replace skipped hardware/live discovery tests with Windows CI or controlled fixtures

## P2: required for full JAMES target

- dynamic process/WASM module loading with trust/signature/hash verification
- Module Resolver and reconciliation loop
- module install/recommend/detect policy modes
- multi-agent identity, budgets, workspaces and lifecycle
- workflow engine
- distributed JAMES instance discovery and remote execution
- full UI Orchestrator, adaptive panels/windows/tabs and renderer variants
- real voice/STT/TTS providers
- SelfMade simulation, then controlled autonomy
- economics, SelfModify and Replication only after security gates

## Advertised capability audit

Before each release, generate a table from the registry:

```text
capability_id
provider
status
executor
input_schema
output_schema
risk
required_permissions
health
last_verified
```

A capability must not be shown as available solely because a module registered its metadata.

## Definition of 100% usable

For the first PC release, 100% means:

1. Clean Windows build and tests pass.
2. Core starts with zero modules.
3. Static modules can be activated deterministically.
4. API is authenticated and loopback-safe.
5. Chat works with a configured provider and reports provider failure honestly.
6. At least one AI-created typed plan reaches a broker executor.
7. Every advertised capability has a real executor or explicit unavailable status.
8. Memory and tasks survive restart.
9. Risky actions require confirmation and are audited.
10. Void displays real state and remains optional; CLI/API still work without it.
11. Discovery produces a usable Windows capability snapshot.
12. No credentials or untrusted web content bypass the policy boundary.

This does not mean every future platform, economic module or SelfMade feature is complete. It means the Windows reference instance is genuinely usable, secure by default and honest about unavailable features.

## Immediate implementation order

```text
1. Fix clean Windows Rust toolchain / dlltool
2. Add API authentication
3. Add typed Agent/Plan/CapabilityRequest path
4. Add resolver and executor registry
5. Remove or mark unimplemented advertised capabilities
6. Add persistent audit/identity/task recovery
7. Implement first Void renderer
8. Add filesystem/process read-only executors
9. Add browser/web executor with timeouts and policy
10. Re-run complete acceptance gate
```
