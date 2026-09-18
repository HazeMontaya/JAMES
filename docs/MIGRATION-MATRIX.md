# JAMES Migration Matrix

Status: initial matrix based on reachable JAMES code only
Date: 2026-09-17

JARVIS and AUTOMATON repositories were not available under `S:\` during the audit. Rows referring to them remain `BLOCKED` until source repositories are provided or mounted.

| Source function | JAMES target | Current JAMES state | Migration state |
|---|---|---|---|
| JARVIS voice | `James-Voice`, `James-STT`, `James-TTS` | First-party modules exist, providers are basic | BLOCKED: source comparison unavailable |
| JARVIS chat | `James-Chat`, `James-AI` | Chat and AI modules exist; system path works | PARTIAL |
| JARVIS UI | `James-Void` | Void module and UI contracts exist | PARTIAL |
| JARVIS models | `James-Models`, `James-ModelRouter` | Modules exist | PARTIAL |
| Automaton agents | `James-Agents` | No dedicated agent module found | NOT STARTED |
| Automaton tasks | `James-Tasks` | Module and core task manager exist | PARTIAL |
| Automaton workflows | `James-Workflow` | No dedicated module found | NOT STARTED |
| Automaton scheduling | `James-Scheduler` | Module and core scheduler exist | PARTIAL |
| Automaton memory/state | `James-Memory`, Core Storage | Memory module exists; recovery incomplete | PARTIAL |
| System control | Filesystem/Applications/Processes modules | mostly not implemented as real executors | NOT STARTED |
| Adaptive Void brain | `James-Void` UI orchestration | architecture specified, renderer incomplete | NOT STARTED |
