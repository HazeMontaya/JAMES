# JAMES Dependency Map

Date: 2026-09-17

## Workspace layers

```text
james-app
  -> james-app-api
  -> james-core

james-core
  -> events
  -> registry
  -> capabilities
  -> services
  -> tasks
  -> scheduler
  -> health

james-system
  -> james-assembly
  -> james-app-api
  -> james-core

james-assembly
  -> text input/output, chat, models, model router
  -> ai/local AI, memory, tasks, scheduler
  -> STT/TTS/voice, browser/web research
  -> Void/dashboard
```

## Boundary rules

- `james-core` must not depend on first-party user capability modules.
- `james-app-api` depends on transport contracts, not AI implementations.
- `james-system` may compose Core and modules.
- Modules register capabilities after Core startup.
- Capability execution must move through the Capability Broker.
- Platform-specific APIs belong behind Platform adapters.

## Current dependency risks

- Module workspaces use path dependencies and duplicate workspace dependency definitions.
- `james-system` currently composes all first-party modules statically.
- Module Host dynamic execution is not yet connected to the assembly.
- Discovery TypeScript models are not yet shared with Core domain models.
