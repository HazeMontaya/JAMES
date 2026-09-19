# JAMES Architecture

This document is a compact index. The detailed architecture is maintained in `JAMES_MASTER_ARCHITECTURE.md` and the focused notes under `docs/architecture/`.

## Canonical lifecycle

```
request -> task -> plan -> capability resolution -> policy/broker -> execution -> observation -> evaluation -> memory -> result
```

## Autonomy lifecycle

```
heartbeat -> observe -> prioritize -> mission -> act -> observe -> evaluate -> memory -> schedule
```

## Self-improvement lifecycle

```
self-observe -> gap detection -> proposal -> isolated workspace -> patch validation -> verification -> behavioral evaluation -> promotion gate -> rollback
```

## Boundary rule

Models are reasoning/proposal providers. Runtime policy, the capability broker, isolation, verification and promotion gates remain deterministic control boundaries.

## Repository rule

There is one canonical implementation tree: `runtime/core`, `runtime/modules`, `runtime/python/sidecar`, `runtime/tools/discovery`, and `ui`. Parallel legacy package trees are not part of the build or runtime.
