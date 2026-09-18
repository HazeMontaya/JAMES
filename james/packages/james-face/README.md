# JAMES Face Visualizer

**Real-time Cognitive State Visualization** — Separate AGPL-3.0 process with WebSocket frontend.

## Architecture

```
┌─────────────┐     NATS + WebSocket     ┌──────────────────┐
│  Rust Core  │ ◄────────────────────► │  Python Face     │
│  (james-core)│   events.>              │  (james-face)    │
│             │   face.state            │  - FastAPI       │
└─────────────┘   face.command          │  - WebSocket     │
                face.layout             │  - WebGL/Canvas  │
                                        └──────────────────┘
```

## Features

- **Real-time Dashboard**: Cognitive state, goals, skills, memory, capabilities
- **Event Stream**: Live NATS event visualization
- **Skill Graph**: Visual skill composition and execution flow
- **Memory Explorer**: Browse episodic/semantic/procedural memory
- **WebGL Visualizer**: 3D state space rendering (optional)
- **REST + WebSocket API**: For frontend integration

## NATS Subjects

| Subject | Direction | Purpose |
|---------|-----------|---------|
| `james.face.state` | Core → Face | Full state snapshot |
| `james.face.events` | Core → Face | Live event stream |
| `james.face.command` | Face → Core | UI commands (pause, inspect) |
| `james.face.layout` | Face → Core | UI layout preferences |

## Quick Start

```bash
cd packages/james-face
uv pip install -e .
james-face
# Open http://localhost:8080
```

## API Endpoints

- `GET /api/state` — Current cognitive state
- `GET /api/events` — Recent events (SSE)
- `GET /api/memory` — Memory browser
- `GET /api/skills` — Skill registry
- `GET /api/capabilities` — Capability registry
- `WS /ws` — Live updates

## License

AGPL-3.0-only