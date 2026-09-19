# JAMES

JAMES is a modular, model-agnostic AI runtime with a Rust execution core, Python sidecar, first-party capability modules, controlled autonomy, persistent memory, repository-aware self-improvement, and the Void UI.

## Canonical repository layout

- `runtime/core/` — Rust core: identity, registry, capabilities, broker, policy boundaries, agents, platform, API, Python bridge.
- `runtime/modules/` — first-party modules and `james-system` full assembly.
- `runtime/python/sidecar/` — Python inference, memory, tools, integrations, autonomy and self-improvement runtime.
- `runtime/tools/discovery/` — TypeScript environment discovery tool.
- `ui/` — UI protocol and Void client.
- `ops/` — operational startup/preflight scripts.
- `scripts/` — development/bootstrap scripts.
- `docs/` — authoritative architecture and project-state documentation.
- `.james/` — local runtime state, models, logs and secrets; never source code.
- `out/` — generated build artifacts; never source code.

There is deliberately **one runtime implementation**. The former parallel Python package tree is retired; new capabilities belong in the canonical runtime or an explicit first-party module.

## Execution model

```
request -> task -> plan -> capability resolution -> policy/broker -> execution -> observation -> evaluation -> memory -> result
```

Models propose and reason; deterministic runtime policy authorizes execution.

## Start on Windows

```powershell
.\\ops\\scripts\\start-james.ps1
```

For local inference bootstrap:

```powershell
.\\scripts\\bootstrap-local.ps1
```

See `docs/README.md` for the authoritative documentation index and `docs/STRUCTURE.md` for repository invariants.
