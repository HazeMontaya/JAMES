# Semantic Ownership Audit

Status: implementation baseline
Date: 2026-09-19
Branch: semantic-ownership-cleanup

## Canonical ownership decisions

| Domain | Canonical owner | Adapter/facade | Duplicate authority | Decision |
|---|---|---|---|---|
| Capability contract/registration | james-capabilities::CapabilityRegistry | module register_capabilities(), Python CapabilitySync | Core Registry previously auto-registered module capabilities | Removed automatic registration from james-registry |
| Capability execution policy | james-capability-broker::CapabilityBroker | CapabilityExecutor implementations | Python NATS bridge executes only after broker dispatch | Keep broker as sole enforcement point |
| General registry metadata | james-registry::Registry | discovery/module-host integrations | Capability registry must not be mutated as a side effect | Registry is metadata-only |
| Event transport | Rust james-events::EventBus | Python event adapters/bridges | Python RuntimeEvent is a sidecar domain representation | Keep one runtime transport; bridge at boundary |
| Task state | Core james-tasks::TaskManager | module task API is currently a parallel implementation | james-tasks-module::TasksModule duplicates Task/Status/persistence semantics | Migration target: module facade over core task state |
| Scheduling | Core james-scheduler::Scheduler | module scheduler currently owns another scheduler loop | james-scheduler-module::SchedulerModule is a second execution scheduler | Migration target: one scheduler authority |
| Model metadata | Rust james-models::ModelsModule in assembly | ModelRouterModule consumes injected registry | Router previously instantiated its own registry | Fixed: registry is injected and lifecycle-owned by assembly |
| Model routing | Rust ModelRouterModule | Python router is a sidecar adapter/runtime implementation | Python ModelRegistry is independently authoritative inside sidecar | Target: Python registry becomes transport/cache representation |
| Persistent memory | Rust james-memory::MemoryModule | Python MemoryVault is a sidecar persistence implementation | JSON memory + SQLite/Markdown memory represent separate stores | Target: define explicit persistence boundary before merging |

## Findings

### 1. Duplicate functions

The consequential duplicates are parallel domain objects, not merely same-named files:

- core TaskManager vs module TasksModule
- core Scheduler vs module SchedulerModule
- Rust ModelsModule vs Python ModelRegistry
- Rust EventBus vs Python RuntimeEvent event model
- Rust persistent memory vs Python MemoryVault

These require staged migration, not deletion.

### 2. Productive path

The full-system assembly constructs ModelsModule, ModelRouterModule, MemoryModule, TasksModule, and SchedulerModule. Agent execution also constructs and uses the core TaskManager.

Therefore the repository currently has two task state domains. This is a real semantic split and must be resolved before deleting either implementation.

### 3. Adapter/facade classification

PythonExecutor is an adapter: it implements the broker CapabilityExecutor and forwards to NATS.

CapabilitySync is a synchronization adapter: it imports Python capability metadata into the Rust capability contract registry. It must never replace an existing non-Python owner.

ModelRouterModule is a routing service, not a model registry. It now receives the canonical ModelsModule instead of constructing one.

### 4. Incorrect dependency direction fixed

james-registry previously depended on james-capabilities and automatically created capability definitions as a side effect of registering a module.

That inverted ownership: generic registry metadata registration could silently mutate the authoritative capability contract registry.

The dependency and side effect have been removed. Module capability registration is now explicit through CapabilityRegistry.

### 5. Cyclic responsibility

The task/scheduler split currently has both:

SchedulerModule -> TasksModule -> persistence/event bus

and:

Scheduler -> TaskManager

Both paths can create queued work. This is the main remaining execution-plane duplication.

The intended invariant is:

Scheduler -> TaskManager -> Broker/Executor

The scheduler must never become an independent executor.

### 6. Registry authority

There must be no generic registry that implicitly owns capabilities, models, or execution policy.

- CapabilityRegistry owns capability contracts.
- ModelsModule owns model metadata in the Rust runtime.
- ModelRouter selects from model metadata but does not own it.
- Broker owns execution authorization.
- Registry owns generic discoverable entity metadata only.

### 7. Event unification

Rust EventBus is the canonical in-process event transport and already provides correlation/causation fields, bounded subscribers, wildcard subscriptions, lifecycle enforcement, and secret scrubbing.

Python RuntimeEvent should therefore be treated as a sidecar representation/serialization boundary, not a second authoritative event transport.

### 8. Memory classification

Current Rust memory is JSON-backed and has explicit Working/Episodic/Semantic/Procedural/Relationship types.

Python MemoryVault is Markdown-first persistent storage with a SQLite retrieval index.

Neither should be deleted yet. They are semantically different stores. The next migration must establish durable canonical memory, derived retrieval index, session/working memory, and cache before consolidation.

## Required invariants

1. No capability is registered by generic module metadata registration.
2. No capability executes outside CapabilityBroker.
3. One authoritative owner exists for each contract type.
4. Adapters may translate; they may not create a second source of truth.
5. Scheduler creates/enqueues jobs; it does not independently execute them.
6. Task execution state has one canonical owner.
7. Model routing consumes model metadata; it does not own model registration.
8. Python model/memory/event registries cannot silently become authoritative over Rust runtime state.
9. Persistent state and caches are explicitly distinguished.
10. Any compatibility layer must be named and bounded.

## Next migration block

1. Convert module TasksModule into a facade over core TaskManager.
2. Convert module SchedulerModule into a facade over core Scheduler.
3. Migrate assembly/dashboard/Void callers to canonical task/scheduler types.
4. Remove duplicate module persistence after all callers migrate.
5. Define the Rust/Python model-registry synchronization contract.
6. Convert Python model registry to a sidecar cache/adapter.
7. Define memory authority and migrate retrieval indexes without losing persistent data.
8. Add architecture regression tests for ownership and dependency direction.
