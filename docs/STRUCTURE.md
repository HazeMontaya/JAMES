# JAMES — Repository Structure

This document is the canonical placement rule for source, runtime, UI, tooling and documentation.

## Root

| Directory | Responsibility |
|---|---|
| `runtime/` | executable runtime implementation |
| `packages/` | Python agent ecosystem and user-facing service packages |
| `ui/` | Void presentation assets and shared interface protocols |
| `ops/` | provisioning, startup and maintenance automation |
| `docs/` | architecture, audits, contracts and migration records |
| `.james/` | local runtime state; generated and never committed |
| `out/` | generated build artifacts; ignored by Git |

## Runtime

- `runtime/core/`: Rust kernel and cross-cutting contracts.
- `runtime/modules/`: first-party modules, assembly and system launcher.
- `runtime/python/`: Python AI/inference sidecar and language bridge.
- `runtime/tools/`: environment discovery and machine inspection.

## Python packages

`packages/` is one Python workspace. Its direct children are independently installable packages:

- `james-core`: cognition, goals, agents, security, events and verification.
- `james-memory`: memory stores and routing.
- `james-skills`: skill registry, composition and built-ins.
- `james-world`: web/world access and credential boundary.
- `james-voice`, `james-face`, `james-hands`: optional multimodal processes.
- `james-bootstrap`: provisioning logic.
- `james-cli`: command-line control surface.

The Python workspace manifest is `packages/pyproject.toml`.

## UI and protocols

- `ui/void/`: Void static presentation.
- `ui/protocols/`: shared wire/interface definitions.
- UI does not execute privileged operations directly. Actions enter the JAMES capability boundary.

## Operations

All repository-level operational scripts belong under `ops/scripts/`. They must resolve paths from the repository root rather than assuming the caller's current directory.

## Data and generated artifacts

Mutable databases, logs, caches, models, secrets and inventory are runtime data. They are not source code and must not be committed.

Build output is generated under root `out/`.

## Placement rules

1. Do not create duplicate Core, Module, Python-runtime or UI trees.
2. Do not place generated files beside source when a generator output directory exists.
3. Do not add application logic to `docs/`, `ui/` or `ops/`.
4. New first-party capabilities belong under `runtime/modules/` and expose a declared capability contract.
5. Python ecosystem features belong under an existing package in `packages/`, or a new package only when its dependency and lifecycle boundary is materially distinct.
6. Shared protocol definitions have one canonical source under `ui/protocols/`.
7. Mutable state belongs under `.james/` and is always ignored by Git.

## Refactor policy

A file move is not complete until imports, manifests, build paths, scripts, CI, documentation and tests agree with the new location.
