# JAMES Architecture

## Runtime layers

1. Interface — CLI, API, desktop/web/mobile clients.
2. Orchestrator — task lifecycle, planning, routing and execution.
3. Model layer — provider adapters, capability registry and model routing.
4. Agent layer — planner, researcher, coder, reviewer, tester and executor roles.
5. Tool layer — filesystem, process execution, Git, browser, search and other controlled tools.
6. Memory layer — conversation, task, project, semantic and system state.
7. Evolution layer — repository inspection, change proposals, sandboxed validation, rollback and promotion.

## Core rule

No model is the runtime. Models are replaceable reasoning providers behind a stable interface.

## Execution lifecycle

request -> task -> plan -> capability/model selection -> tool execution -> observation -> validation -> result

## Self-improvement lifecycle

inspect repository -> identify change -> create proposal -> isolate workspace -> modify -> test -> build -> validate -> promote or rollback

The model proposes changes; deterministic tooling and tests decide whether a change is valid.

## Immediate implementation order

- establish package/runtime boundaries
- define model provider protocol
- define tool protocol and permission model
- add task state machine
- add persistent memory interface
- add repository inspector
- add isolated evolution runner
- add API/UI adapters
- add voice adapters
