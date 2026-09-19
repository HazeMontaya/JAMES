# JAMES Documentation

## Source of truth

1. `JAMES-MASTER-CONCEPT.md` — identity and non-negotiable architecture/security rules.
2. `JAMES_MASTER_ARCHITECTURE.md` — canonical runtime lifecycle and subsystem boundaries.
3. `PROJECT-CONTINUATION.md` — current implementation state and next gates.
4. `CURRENT-STATE.md` — verification snapshot.
5. `STRUCTURE.md` — canonical repository layout and anti-duplication invariants.
6. `DEPENDENCY-MAP.md` — workspace dependencies and boundaries.
7. `FUNCTIONAL-GAP-ANALYSIS.md` — capability gaps and acceptance gates.
8. `JAMES_ECOSYSTEM_INTEGRATION.md` — external ecosystem integration boundaries.
9. `AUDIT.md` — reproducible audit information.
10. `architecture/` — focused subsystem architecture notes.

## Canonical workspaces

- Rust core: `runtime/core/`
- Rust modules: `runtime/modules/`
- Python sidecar: `runtime/python/sidecar/`
- Discovery: `runtime/tools/discovery/`
- UI: `ui/`

Retired parallel package trees must not be reintroduced. When a responsibility moves, remove the old implementation and update references in the same change.

## Windows quick start

```powershell
.\\ops\\scripts\\start-james.ps1
```

Local inference bootstrap:

```powershell
.\\scripts\\bootstrap-local.ps1
```
