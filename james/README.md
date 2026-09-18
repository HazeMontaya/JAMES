# JAMES - Autonomous Business Agent System

> **J**oint **A**utonomous **M**ulti-agent **E**xecution **S**ystem

An autonomous agent system designed for business automation with memory, skills, world access, and multimodal interaction.

## Architecture

```
JAMES
├── Core (Cognitive Engine)
│   ├── Model Router (vLLM, llama.cpp, OpenAI, Anthropic, Ollama)
│   ├── Reasoning (CoT, ToT, ReAct, Self-Consistency)
│   ├── Planning (Goal Decomposition, Skill Graphs)
│   ├── Reflection (Execution Analysis, Pattern Recognition)
│   ├── Goals (Lifecycle Management, Sub-goals)
│   ├── Agents (Orchestration, Delegation, Registry)
│   ├── Verification (Multi-criteria, Cross-source)
│   └── Learning (Pattern Extraction, Skill Memory)
│
├── Memory (Hybrid Storage)
│   ├── Episodic (SQLite - Events, Runs, Decisions)
│   ├── Semantic (Markdown Vault + Vector Index)
│   ├── Procedural (SQLite - Skills, Procedures)
│   ├── Business (SQLite - Opportunities, Metrics)
│   └── User (Markdown - Profile, Preferences, Context)
│
├── Skills (Operational Engine)
│   ├── Market Research
│   ├── Offer Design
│   ├── Landing Page
│   ├── Email Sequence
│   ├── Analytics Review
│   └── Extensible Plugin System
│
├── World Access (Capability Router)
│   ├── Agent-Reach Adapter (GitHub, Reddit, YouTube, X, Web)
│   ├── Browser (Playwright)
│   ├── GitHub (GraphQL)
│   ├── Web Search (Exa, Jina, DDG)
│   ├── RSS
│   └── Credential Vault (age encryption)
│
├── Voice (AGPL - Separate Process)
│   ├── STT (faster-whisper)
│   ├── TTS (Kokoro, ElevenLabs)
│   ├── Barge-in / Interrupt
│   ├── Voice Control Plane
│   └── gRPC Interface
│
├── Face (AGPL - Separate Process)
│   ├── State Mapper (Cognitive → Visual)
│   ├── 4 Renderers (Circuit, Orb, Matrix, Constellation)
│   ├── WebSocket + Three.js
│   └── Deterministic Test Mode
│
├── Hands (AGPL - Future)
│   ├── Gesture Tracking (MediaPipe)
│   ├── Intent Mapping
│   └── Media Airlock
│
└── Bootstrap
    ├── Single Config Wizard
    ├── Component Discovery
    ├── Credential Setup
    └── Update Orchestrator
```

## Quick Start

```bash
# Install
uv sync --all-extras

# Start NATS (required)
docker-compose up -d

# Run bootstrap wizard
james bootstrap

# Check health
james doctor

# Run a goal
james run "Research market for AI note-taking apps"
```

## Development

```bash
# Run tests
uv run pytest

# Type check
uv run mypy packages/

# Lint
uv run ruff check packages/

# Format
uv run ruff format packages/
```

## License

- Core packages: MIT
- Voice/Face/Hands: AGPL-3.0-only (separate processes)

## Project Structure

```
james/
├── pyproject.toml              # Workspace root
├── docker-compose.yml          # NATS JetStream
├── packages/
│   ├── james-core/             # Cognitive engine
│   ├── james-memory/           # Hybrid memory system
│   ├── james-skills/           # Skill engine + built-ins
│   ├── james-world/            # World access layer
│   ├── james-voice/            # Voice pipeline (AGPL)
│   ├── james-face/             # Visualizer (AGPL)
│   ├── james-hands/            # Gesture control (AGPL)
│   ├── james-bootstrap/        # Installation wizard
│   └── james-cli/              # Main entry point
├── skills/                     # User skills (~/.james/skills)
├── vault/                      # Memory vault (~/.james/vault)
├── tests/                      # Test suite
└── docs/                       # Documentation
```