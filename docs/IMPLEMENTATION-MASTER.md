# JAMES Implementation Master

Status: canonical execution ledger
Date: 2026-09-19

This document is the implementation ledger for the JAMES repository. It reconciles the master concept, current-state, audit, continuation notes, and the implemented runtime architecture. It is not a second architecture: `docs/JAMES-MASTER-CONCEPT.md` remains the conceptual contract; this file records executable completion state.

## 1. Non-negotiable architecture

JAMES is a portable-first runtime. The dependency direction is:

```
Interface/UI/CLI
 -> Application/API
 -> Agent/Task orchestration
 -> Capability Resolver
 -> Permission/Policy
 -> Capability Broker
 -> Executor
 -> Platform/Module implementation
 -> Verification
 -> State/Event/Audit
```

AI proposes reasoning and plans. It is never the authorization boundary.

SelfMade is an orchestration layer above the normal execution spine. It may observe, propose, isolate, modify, verify and roll back candidates. Canonical promotion is a separate policy-controlled operation.

## 2. Current repository domains

- `runtime/core/`: portable Rust core
- `runtime/modules/`: first-party Rust modules and system assembly
- `runtime/python/`: Python runtime/sidecars
- `runtime/tools/`: discovery and operational tooling
- `interfaces/void/`: current Void renderer/assets
- `packages/`: Python ecosystem packages
- `ops/`: setup/start/deployment
- `docs/`: contracts and verification evidence
- `.james/`: runtime-local state; never architecture authority

## 3. Implemented foundations

- Core lifecycle, event bus, registries, health and services
- Capability registry, resolver, broker and typed request support
- Permission/policy enforcement with fail-closed conditional handling
- Agent/plan/resolver path and shared executor registry
- Assembly-level AI planning with heuristic fallback
- Memory foundation, sessions and skills
- Model registry/router and G0DM0D3-inspired inference steering primitives
- Heartbeat with durable state
- SelfMade observation, assessment, proposal, isolated git worktree, patch path validation, verification and rollback
- Void UI orchestration, event projection and two-way action routing
- Local API authentication/correlation support
- Discovery tooling and Windows-first environment detection
- CI definitions for Rust, Python and Discovery

## 4. Known incomplete contracts

### P0 integration blockers

1. Clean Windows end-to-end build/test must be re-established from the actual rustup MSVC toolchain.
2. Python bridge and capability-sync code must pass a clean Rust compile after recent contract changes.
3. Every advertised capability must have a real executor or an explicit unavailable/degraded state.
4. The complete Agent -> CapabilityRequest -> Resolver -> Broker -> Executor -> Verification -> Audit path needs end-to-end acceptance tests.
5. Runtime state, tasks, identity and audit need restart persistence rather than isolated/file-local persistence.
6. Discovery output must become a versioned Core Environment Snapshot.
7. Void must be verified against the current full assembly, not only unit/API tests.

### P1 runtime completeness

- Platform ports and real Windows adapters for filesystem/process/application/audio/device operations
- brokered read-only filesystem/process capabilities
- controlled browser/web research with timeout, cancellation and trust handling
- persistent task recovery and resumable execution
- durable audit chain and identity state
- provider/model discovery and health reconciliation
- module resolver/reconciliation and real dynamic loading
- confirmation lifecycle persistence
- complete UI action capability contracts

### P2 target capabilities

- multi-agent budgets/workspaces/lifecycle
- workflow engine
- remote JAMES instances and remote capabilities
- voice/STT/TTS with real providers
- full SelfMade evaluation and policy-gated promotion
- economics, SelfModify and replication only after security gates
- additional platforms

## 5. SelfMade completion contract

Current:

```
observe -> assess -> proposal -> isolated patch -> verify -> rollback/retain candidate
```

Required for controlled autonomous improvement:

```
observe
 -> understand repository/environment
 -> identify measurable gap
 -> research/reuse
 -> propose
 -> risk/resource admission
 -> isolated implementation
 -> deterministic verification
 -> behavioral evaluation against baseline
 -> regression/security evaluation
 -> policy gate
 -> promotion
 -> post-promotion health check
 -> durable learning
 -> recovery/rollback if required
```

A green `cargo test` is not sufficient evidence of improvement.

## 6. Capability truth model

A capability is usable only when all of these are true:

```
registered
+ schema-valid
+ resolver-visible
+ authorized
+ executable
+ healthy
+ verified
```

Use explicit states:

`DISCOVERED`, `AVAILABLE`, `DEGRADED`, `UNAVAILABLE`, `SELECTED`, `LOADED`, `HEALTHY`.

No UI, planner or model may infer availability from registration metadata alone.

## 7. Memory truth model

The repository contains several memory implementations. They must converge behind one logical Memory Fabric:

- working memory
- episodic memory
- semantic memory
- procedural memory
- relationship/user/project memory
- durable learning
- session context

Backends may differ. The application contract must not.

## 8. Autonomous loop

Heartbeat is scheduling/infrastructure. The autonomous cognitive loop must be:

```
wake
 -> observe
 -> retrieve relevant memory/goals
 -> detect work/anomaly
 -> reason
 -> select typed action
 -> policy/admission
 -> execute
 -> observe result
 -> verify
 -> persist event/memory
 -> detect loops/failure
 -> schedule next wake
```

No direct model-to-shell path is permitted.

## 9. Verification ladder

Every implementation block uses:

1. source inspection
2. compile/type check
3. focused unit tests
4. integration/contract tests
5. security boundary tests
6. restart/recovery tests where stateful
7. end-to-end acceptance
8. documentation/state update

`IMPLEMENTED` and `VERIFIED` remain separate states.

## 10. Stop conditions for error-loop prevention

Do not repeatedly fix one compiler error without inspecting its contract boundary.

When a failure appears:

```
failure
 -> identify layer
 -> inspect caller/callee contract
 -> inspect all implementations
 -> patch the boundary
 -> compile affected workspace
 -> run regression tests
 -> continue downstream
```

Environment/toolchain errors are separated from repository code errors.

## 11. Release gates

### PC reference release

Must prove:

- clean Windows checkout builds
- one-command setup
- discovery snapshot
- Core boot without optional modules
- authenticated local API
- real local/remote model routing when configured
- typed agent plan reaches a broker executor
- memory/tasks survive restart
- risky actions require policy/confirmation
- audit records the action
- Void is a projection of real state
- unavailable features are reported honestly
- complete acceptance suite passes

### Full target

Adds dynamic modules, broad platform adapters, advanced autonomy, evaluation/promotion, voice/devices, multi-agent and distributed capabilities.

## 12. Development rule

Do not add another major feature by copying an existing subsystem. First identify the canonical contract and make the new implementation a provider/module of that contract.

Do not delete legacy code merely because a newer implementation exists. Remove it only after references are migrated, tests cover the replacement, and the old path is demonstrably unreachable or redundant.

## 13. Immediate execution order

1. clean Rust compile/test across Core and Modules
2. inspect and repair Python bridge/capability sync contract
3. capability registry/executor availability audit
4. end-to-end broker contract suite
5. durable state/recovery consolidation
6. discovery -> Environment Snapshot
7. platform executor coverage
8. browser/web controlled execution
9. Void/full-assembly acceptance
10. SelfMade evaluation engine
11. policy-gated promotion
12. autonomous decision loop
13. remaining target capabilities

This order is deliberately dependency-driven. It prevents the project from accumulating additional disconnected features while core contracts are unstable.
