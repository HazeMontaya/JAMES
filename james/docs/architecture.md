# JAMES Architecture

This document describes the architecture of the JAMES autonomous business agent system as implemented in `james/`.

## Overview

JAMES is a modular agent system: a **core cognitive engine** coordinates **skills**, backed by a **hybrid memory**, with optional **multimodal interfaces** (voice, face, hands) as separate AGPL processes. All components communicate asynchronously through a **NATS JetStream** message bus, except where local SQLite storage is used.

```
┌─────────────────────────────────────────────────────────────┐
│                        james-cli                            │
│   run | doctor | config | skill | memory | audit | voice    │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│                      GoalRunner (CLI)                       │
│  build_runner_components → initialize → run → close         │
│  • event_bus (NATS)   • model_router   • audit_log          │
│  • skill_engine       • memory_router                       │
└───────────────────────────┬─────────────────────────────────┘
                            │
      ┌─────────────────────┼──────────────────────┐
      ▼                     ▼                      ▼
┌──────────┐         ┌──────────────┐       ┌──────────────┐
│ Skill    │         │   Memory     │       │   Security   │
│ Engine   │         │   Router     │       │   Layer      │
├──────────┤         ├──────────────┤       ├──────────────┤
│ composer │         │ Episodic     │       │  AuditLog    │
│ executor │         │ Semantic     │       │  (SQLite)    │
│ registry │         │ Procedural   │       │  redact_     │
│ loader   │         │ Business     │       │  secrets     │
├──────────┤         │ User         │       │  sanitize_   │
│ builtins │         └──────────────┘       │  text        │
│  (6)     │                                └──────────────┘
└──────────┘
```

## Component Overview

### james-core (`packages/james-core`)

Shared foundation used by every other package.

- **Config** (`config.py`): `JamesSettings` (Pydantic), `JamesConfig` with paths, model endpoints (ollama, openai, anthropic).
- **Events** (`events/`): `EventBus` with `Event`, `EventType`, NATS-backed transport (`NatsEventBus`) and in-memory fallback. Publish/subscribe with subject-based routing.
- **Security** (`security.py`): secret detection (`is_secret_key`, regex patterns for bearer tokens, `sk-*` keys, GitHub fine-grained tokens, AWS access keys, Slack tokens), recursive redaction (`redact_secrets`), text redaction (`redact_secrets_in_text`), and `sanitize_text` (control char stripping + length cap).
- **Audit** (`audit.py`): append-only `AuditLog` backed by SQLite (WAL mode). Every entry is recorded with actor, action, resource, outcome enum (`AuditOutcome`), severity enum (`AuditSeverity`), a SHA-256 fingerprint, and optional details. Details are **secret-redacted before storage**; sensitive values never reach disk.

### james-memory (`packages/james-memory`)

Hybrid storage, all SQLite-based in the implementation so far:

| Store      | Purpose                                  | Location             |
|------------|------------------------------------------|----------------------|
| Episodic   | Events, runs, decisions                  | `memory/episodic.db` |
| Semantic   | Concept knowledge, markdown + embeddings | `memory/semantic/`   |
| Procedural | Skills and procedures                    | `memory/procedural`  |
| Business   | Opportunities, metrics                   | `memory/business.db` |
| User       | Profile, preferences                     | `memory/user`        |

`MemoryRouter.store` / `retrieve` dispatch on `MemoryType` (episodic, semantic, procedural, business, user).

### james-skills (`packages/james-skills`)

The operational engine. A skill has a `skill.yaml` spec (name, purpose, status, inputs, outputs, steps, verification) plus an optional Python entry point.

- **Loader** discovers skills from user dirs (`~/.james/skills`) and built-ins.
- **Registry** keeps enabled skills, names, categories.
- **Composer** maps a goal to skills via keyword matching (`compose_from_keywords`).
- **Executor** runs Plan → Execute → Verify → Learn:
  - `skill` steps → nested skill execution
  - `tool` steps → world capability calls
  - `function` steps → `async_execute` Python entry point (deterministic fallback)
  - default LLM step → model router, or Python fallback if no router available
- **Verification**: criteria like `contains:...`, `min_length:...`, `has_outputs`, `no_error`.

Built-ins (6): `market_research`, `lead_generation`, `web_scraper`, `code_generator`, `data_analyzer`, `email_drafter`. Each has a deterministic Python fallback so the system works fully offline / without model endpoints. `web_scraper` iterates a `tool` step (`tool_ref: web_fetch`) so it uses the live world capability when available, and falls back to direct `urllib` fetching with built-in HTML extraction otherwise.

### james-world (`packages/james-world`)

Capability router for external access. Capabilities (`CapabilityRegistry`) route through prioritized backends with health checks, timeout, and failover.

- **`web_search`**: DuckDuckGo HTML search + Jina reader (`r.jina.ai`) with 20k-token content caps.
- **`web_fetch`**: direct HTTP(S) fetch via httpx with HTML→text extraction (`adapters/web_fetch.py`), JSON passthrough, 60k-chars cap, follow-redirects.
- **Facade methods** (`WorldAccess`): `web_search`, `web_fetch`, `http_fetch` — resolved by skill `tool` steps via `getattr(ctx.world, tool_ref)`.
- **CredentialVault**: Fernet-encrypted store for API keys.

Skills resolve `tool_ref` directly on `ctx.world`; if no world is provided the executor falls back to the skill's Python entry point (deterministic offline run) instead of failing.

### james-voice / james-face / james-hands (AGPL)

Separate multimodal processes communicating over NATS.

- **Voice** (`james-voice`): STT (faster-whisper), TTS (Kokoro/ElevenLabs), barge-in, control plane. Audio payloads are base64-encoded in events; STT accepts base64 strings or raw bytes. `VoiceStatus` and `HandsStatus` use `field(default_factory=...)` for mutable default safety and serialize numpy values via `float()`.
- **Face** (`james-face`): cognitive state → visual mapping via deterministically-testable `CognitiveState` (enum, lowercase values, serializable via `to_dict`/`from_dict`).
- **Hands** (`james-hands`): MediaPipe gesture tracking with intent mapping. `ActionExecutor.execute_action_type(action_type, params)` bypasses gesture mapping for direct action invocation; gesture recognition has deterministic landmark logic testable without hardware or MediaPipe.

### james-cli (`packages/james-cli`)

Entry point. `GoalRunner` wires the components together:

```
GoalRunner(config)
├── build_runner_components(config)
│   ├── event_bus : NatsEventBus (NATS) or InMemoryEventBus   [NATS disabled → memory fallback when unavailable]
│   ├── model_router : from config endpoints (all disabled → None)
│   ├── skill_engine : SkillEngine()
│   ├── memory : MemoryRouter(paths.memory)
│   └── audit : AuditLog(paths.memory / "audit.db")
├── initialize() : opens memory + audit + event bus
└── run(goal) :
    │  1. compose goal → skills
    │  2. per skill: emit event, run skill, emit event
    │  3. audit record goal.run + skill.run (with duration)
    │  4. build final report (markdown) with skill outputs
    └── returns {"success", "skills", "report", "message"}
```

The CLI exposes: `run`, `doctor` (health checks, `--json`), `config` (`--show`/`--set`), `skill` (`--list`/`--info`), `memory`, `audit` (`--limit/--actor/--action`), `voice`, `bootstrap`, `version`.

## Configuration

Configuration lives in `~/.james/config.yaml` (or overridden via `JamesConfig` in tests). Key settings:

- `paths.*` – root, config, vault, memory, skills, credentials, tools, runtimes, models, cache, logs, browser, sandboxes
- `ollama` / `openai` / `anthropic` – model endpoints (`enabled`, `base_url`, `model`, `api_key`)
- `logging.level`

## Data Flow: Goal → Report

```
CLI: james run "Research the market for AI note apps"
  │
  ├─ GoalRunner.run(goal)
  │    ├─ skill_engine.compose(goal)      → SkillComposition (matching skills)
  │    ├─ (if none) ──► audit goal.run=failure; return {success: False}
  │    ├─ for each skill (parallel):
  │    │    ├─ event_bus.publish(goal.run, skill=<name>)          [subject: james.goal.run]
  │    │    ├─ skill_engine.run_skill(name, context)              → SkillResult
  │    │    ├─ event_bus.publish(goal.skill, status="completed")  [subject: james.skill.<name>]
  │    │    └─ audit.record(skill.run, outcome=success/failure, duration_ms)
  │    └─ audit.record(goal.run, outcome=..., duration_ms=total)
  │    └─ render report (markdown) from skill outputs
  └── return {success, skills, report}
```

## Security Model

- **Audit trail**: every goal/skill run is permanently logged (append-only, WAL). The `james audit` command queries it.
- **Secret redaction**: any dict/text passing through `redact_secrets` / `redact_secrets_in_text` strips credentials to `[REDACTED]` before storage or logging. Audit details are redacted before write.
- **Sanitization**: `sanitize_text` caps length and strips control characters before writing to logs/storage.
- **Note**: never store API keys outside the credential vault (`~/.james/credentials`).

## Testing Strategy

| Layer            | Location                     | Notes                                         |
|------------------|------------------------------|-----------------------------------------------|
| Unit             | `tests/unit/`                | config, memory, event models, security/audit, skill engine (17) |
| Integration      | `tests/integration/`         | voice/hands/face pipelines (mocked NATS engines), world+skills (mocked HTTP) |
| E2E              | `tests/e2e/`                 | goal runner, event bus (NATS skipped unless available) |
| Performance      | `tests/performance/`         | latency regression thresholds for hot paths   |
| Health checks    | `james doctor`               | 8 checks: python, venv, config, memory, audit, skills, event bus, model |

NATS-dependent tests are auto-skipped when `SKIP_NATS_TESTS` is unset/truthy (default) since a live NATS requires Docker.

Run everything:

```bash
uv run pytest
uv run ruff check packages/ tests/
uv run mypy packages/
```

## Environment Variables

- `SKIP_NATS_TESTS` – skip NATS-dependent tests (default truthy).
- `JAMES_CONFIG` – override config file path.

## CI/CD

GitHub Actions workflows live in `.github/workflows/`:

- **`ci.yml`** – on push/PR: matrix (ubuntu py3.11 + py3.12, windows py3.12) running ruff lint/format check, mypy, full pytest, performance gates, then a CLI smoke test (`doctor --json`, `run`, `audit`). A separate `minimal` job proves the fully-offline happy path (no NATS/model/Docker).
- **`release.yml`** – on tag `v*`: gates again, then drafts a GitHub Release with auto-generated notes.

All gates mirror the local commands documented in this file, so CI and local behave identically.