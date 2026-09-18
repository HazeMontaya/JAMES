# JAMES Deployment Guide

This guide covers installing, configuring, and operating JAMES in production.

## Prerequisites

- Python 3.12+
- [uv](https://docs.astral.sh/uv/) package manager
- Optional: Docker + NATS JetStream (needed for the real event bus)
- Optional: model endpoints (Ollama for local, or OpenAI/Anthropic APIs)

## Installation

```bash
cd james
uv sync --all-extras
```

This creates a virtual environment and installs all packages (`james-core`, `james-memory`, `james-skills`, `james-world`, `james-cli`, and AGPL `james-voice`, `james-face`, `james-hands` with pyautogui etc.).

## Configuration

Configuration is created on first run at `~/.james/config.yaml`. Review and adjust:

```bash
james config --show          # dump effective settings
james config --set logging.level --value debug
james bootstrap              # interactive setup wizard
```

### Model endpoints

Edit `~/.james/config.yaml`:

```yaml
ollama:
  enabled: true
  base_url: "http://localhost:11434"
  model: "llama3.1:8b"
  api_key: ""
# openai:
#   enabled: true
#   model: "gpt-4o"
#   api_key: "sk-..."
```

If all endpoints are disabled, JAMES still runs skills via deterministic Python fallbacks (no LLM required), which is useful for testing and offline operation.

## Health Checks

```bash
james doctor          # human-readable
james doctor --json   # machine-readable
```

Returns up to 8 checks: Python version, venv, config, memory, audit store, skill catalogue, event bus, model router. A CI-friendly JSON `status: healthy` is produced when all pass.

## Running Goals

```bash
james run "Research the market for AI note-taking apps"
james run "Generate a follow-up email for lead X" --priority high
james run "Analyze our Q3 metrics" --skills data_analyzer,market_research
```

Output: a markdown report (success report) or a "no skills matched" message with exit code 1.

## Security & Audit

Every goal/skill run is recorded in `~/.james/memory/audit.db`. Secrets are redacted before storage.

```bash
james audit                      # last 20 entries
james audit --limit 50 --actor cli
james audit --action goal.run --actor cli
```

The audit log:

- Append-only SQLite (WAL) with SHA-256 fingerprint per entry.
- Filters: `--limit`, `--actor`, `--action` (outcome/severity filtering available via the library).

Backup the audit DB regularly; tampering is detectable (fingerprints break).

## Where Data Lives

Paths under `~/.james/` (override via `paths.*` in config):

| Path            | Contents                                  |
|-----------------|-------------------------------------------|
| `config.yaml`   | Settings and endpoint configuration       |
| `memory/`       | Episodic/business/procedural DBs, semantic + user markdown |
| `memory/audit.db`| Append-only audit log                    |
| `skills/`       | User-defined skills                       |
| `credentials/`  | Credential vault (keep encrypted/access-controlled) |
| `logs/`         | Application logs                          |
| `vault/`        | Semantic memory markdown vault            |

## NATS (optional, for distributed mode)

```bash
docker-compose up -d          # starts NATS JetStream
```

Without NATS, JAMES falls back to an in-memory event bus; skills and memory still work. NATS is required for cross-process multimodal events (voice/face/hands).

## Multimodal Processes (AGPL)

Voice, Face, and Hands run as separate processes and communicate over NATS. They are scaffolded and testable with mocked engines (no hardware required):

```bash
uv run pytest tests/integration/test_voice_pipeline.py
uv run pytest tests/integration/test_hands_pipeline.py
uv run pytest tests/integration/test_face_state.py
```

Connect real hardware/engines (faster-whisper, microphone, camera, MediaPipe) by implementing the engine adapters; the pipelines, state machines, and serialization are already wired and tested.

## Observability

- **Logging**: structlog (`james_core`), level via `logging.level`.
- **Metrics**: not yet wired; the audit log and skill durations (`duration_ms`) provide baseline timing.

## Upgrades & Backups

1. Stop the agent and producers.
2. Back up `~/.james/` (config, memory DBs, vault, audit DB, credentials).
3. `git pull` and `uv sync --all-extras`.
4. Run `james doctor` to verify health, then restart.

## Troubleshooting

| Symptom                              | Likely cause / fix                              |
|--------------------------------------|-------------------------------------------------|
| `No skills matched the goal`         | Goal wording doesn't hit skill keywords; try more domain-specific phrasing or `--skills`. |
| Models not used (fallback outputs)   | Endpoint disabled or unreachable; check `james config --show`. |
| NATS tests skipped                   | `SKIP_NATS_TESTS` truthy or Docker/NATS absent — expected when no live NATS. |
| Mypy numpy stub error                | Known upstream issue (numpy stubs require 3.12 syntax); project code itself is type-clean. |
| Secrets appear in logs/audit         | Report a bug: audit redaction must run before persistence. |