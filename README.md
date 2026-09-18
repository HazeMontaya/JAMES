# JAMES

JAMES is a local-first agent and automation runtime.

## Repository layout

- `runtime/core/` — Rust kernel, contracts, security, lifecycle
- `runtime/modules/` — first-party capability modules and system launcher
- `runtime/python/` — Python AI runtime sidecar and bridge
- `runtime/tools/` — environment discovery and runtime tooling
- `packages/` — Python agent ecosystem: cognition, memory, skills, world, voice, UI
- `ui/` — user-facing Void assets and shared protocols
- `ops/scripts/` — provisioning/start/maintenance scripts
- `docs/` — architecture, audits, contracts, migration records
- `.james/` — local runtime state, generated and never committed

## Source-of-truth rules

- `runtime/core` owns execution contracts and security boundaries.
- `runtime/modules` owns first-party capability implementations.
- `runtime/python` owns the Python inference sidecar.
- `packages` contains the higher-level Python agent ecosystem.
- `ui` contains presentation/protocol assets only; it does not own execution state.
- `docs` contains project contracts and verification records.
- Build output belongs in root `out/` and is generated.
- Runtime state belongs in `.james/` and is never committed.

## Windows quick start

From CMD:

`cd /d S:\JAMES`

`git pull --ff-only origin main`

`powershell -NoProfile -ExecutionPolicy Bypass -File "ops\scripts\start-james.ps1"`

See `docs/JAMES-MASTER-CONCEPT.md` and `docs/PROJECT-CONTINUATION.md` before changing architectural boundaries.
