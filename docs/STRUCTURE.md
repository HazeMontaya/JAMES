# JAMES — Canonical Repository Structure

This file is the structural source of truth. The repository has one implementation path per runtime responsibility.

## Top level

| Path | Responsibility | Status |
|---|---|---|
| `.github/` | CI/CD | canonical |
| `docs/` | architecture, state, audits, migration and operational documentation | canonical |
| `ops/` | operational startup/preflight scripts | canonical |
| `runtime/core/` | Rust core workspace | canonical |
| `runtime/modules/` | first-party Rust module workspace | canonical |
| `runtime/python/sidecar/` | Python runtime/AI sidecar | canonical |
| `runtime/tools/discovery/` | TypeScript environment discovery | canonical |
| `ui/` | protocol + Void UI | canonical |
| `scripts/` | bootstrap/development scripts | canonical |
| `.james/` | generated local state | runtime-only, gitignored |
| `out/` | generated build artifacts | build-only, gitignored |

## Removed parallel implementations

The old `packages/` workspace duplicated cognition, memory, skills, world, voice, face, hands, bootstrap and CLI responsibilities already represented by the current runtime/module architecture. It is retired rather than maintained as a second implementation.

The old root `james/` package and root `pyproject.toml` were only a minimal status CLI/runtime stub and are also retired. The executable runtime is under `runtime/python/sidecar/`, while Rust owns system orchestration.

## Invariants

1. Rust core code lives under `runtime/core/` only.
2. First-party Rust capabilities live under `runtime/modules/` only.
3. Python runtime code lives under `runtime/python/sidecar/` only.
4. Environment discovery lives under `runtime/tools/discovery/` only.
5. UI source lives under `ui/` only.
6. Generated state never becomes source code.
7. Documentation describes the current tree; it must not advertise retired paths.
8. A capability must have one authoritative implementation and one registration path.

## Build artifacts

Rust targets are redirected to `out/core/` and `out/modules/`. Discovery's generated dependencies and build output remain local and ignored.

## Runtime state

`.james/` contains local configuration, durable event/mission state, model files, bridge credentials and other instance data. Secrets and instance state never belong in source control.
