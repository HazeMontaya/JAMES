# JAMES Master Architecture Contract

Status: implementation contract
Scope: runtime/core, runtime/modules, runtime/python/sidecar, discovery

## 1. System boundary

JAMES is a local-first agent runtime. The canonical flow is:

request -> task -> plan -> capability resolution -> policy/broker -> execution -> observation -> evaluation -> memory -> result

Autonomy adds:

heartbeat -> observe -> prioritize -> act -> observe -> learn -> schedule

Self-development adds:

self-observe -> gap detection -> proposal -> isolated worktree -> patch validation -> verification -> behavioral evaluation -> promotion gate -> rollback

## 2. Non-negotiable boundaries

- Models propose; runtime policy authorizes.
- Capability registry is the identity source for executable capabilities.
- Capability resolver selects an executor; the broker enforces authorization.
- Python capabilities execute through the bridge; they do not bypass the broker for privileged operations.
- SelfMade never mutates the canonical checkout during candidate development.
- Every generated patch is path-validated before git applies it.
- Verification failure must roll the isolated candidate back.
- Promotion is a separate operation from development and verification.
- Security roots, audit integrity, kill controls, and promotion policy are not model-editable.
- Events are the cross-subsystem observability contract.

## 3. Canonical lifecycle

Every autonomous action should be representable as:

1. Observe
2. Identify task/goal
3. Retrieve relevant memory
4. Generate candidate plan
5. Resolve required capabilities
6. Authorize through policy/broker
7. Execute with timeout/cancellation
8. Observe result
9. Validate/evaluate
10. Persist event and learning
11. Schedule follow-up

No subsystem should invent a parallel lifecycle when the same operation can use this contract.

## 4. Inference layer

Model routing, AutoTune, feedback, STM, liquid/race selection and provider fallback belong to inference optimization. They are implementation mechanisms below the agent/runtime layer, not the system architecture itself.

## 5. Memory fabric

The target memory model is:

- working: current reasoning/task context
- episodic: completed interactions/events
- semantic: durable facts
- procedural: reusable skills
- project: repository/project knowledge
- learning: outcomes of experiments and evolution cycles
- system: runtime state and configuration

Transient caches may use SQLite or in-memory structures, but durable memory must have a recoverable representation.

## 6. Capability lifecycle

A capability is healthy only when all of the following are true:

registered + schema-valid + resolvable + authorized + executable + observable

Python/Rust synchronization must converge on this contract. Registration alone must not be treated as proof that execution works.

## 7. SelfMade lifecycle

SelfMade is a controlled software-evolution subsystem:

observe -> assess -> propose -> develop -> verify -> evaluate -> promote OR rollback

A passing build/test is necessary but not sufficient evidence for promotion. Behavioral and regression evaluation must be added before autonomous promotion is enabled.

## 8. Autonomy

Heartbeat is scheduling infrastructure, not the decision engine. A future autonomous loop must be able to:

- restore durable state
- inspect unfinished work
- inspect health
- detect stale/failed tasks
- retrieve relevant memory
- select a bounded action
- execute through the normal capability path
- persist the result
- detect repeated loops
- back off when resources or dependencies are unavailable

## 9. Verification tiers

Candidate changes should progress through:

1. formatting/static checks
2. compilation
3. unit tests
4. integration tests
5. capability contract tests
6. security/path-policy tests
7. regression tests
8. behavioral evaluation
9. resource/timeout checks
10. promotion policy

## 10. Anti-loop rule

When a failure appears, repair the lowest common architectural contract that explains it. Do not repeatedly patch downstream symptoms without re-checking the shared contract.

## 11. Current implementation gaps

The repository currently has substantial foundations for all major layers, but the following remain integration work:

- unified end-to-end lifecycle
- capability health/convergence semantics
- unified memory fabric
- autonomous decision loop above heartbeat
- candidate behavioral evaluation
- promotion workflow (implementation exists behind explicit `JAMES_SELFMADE_ALLOW_PROMOTION=1` policy gate; behavioral evaluation remains a prerequisite for autonomous use)
- repository self-model/index
- end-to-end contract/regression suite

These gaps are intentional work items, not reasons to discard the existing architecture.

## 12. Definition of done

JAMES is not considered complete merely because individual workspaces compile. Completion requires:

- clean reproducible bootstrap
- core build and tests
- modules build and tests
- Python sidecar tests
- discovery build/tests
- runtime integration test
- capability bridge contract test
- autonomous loop test
- SelfMade sandbox/verify/rollback test
- candidate evaluation test
- promotion/rollback test
- clean startup/shutdown
- documented recovery procedure
