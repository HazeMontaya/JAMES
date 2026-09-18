"""Integration tests for Face (state snapshots + WebSocket/NATS serialization)."""

import json
from datetime import UTC, datetime

from james_face.state import (
    CapabilityState,
    CognitiveState,
    CognitiveStateEnum,
    EventEntry,
    GoalState,
    GoalStatus,
    MemoryStats,
    SkillState,
    SkillStatus,
)


def _sample_state() -> CognitiveState:
    state = CognitiveState(state=CognitiveStateEnum.EXECUTING)
    state.uptime_seconds = 42.5
    state.active_goals = [
        GoalState(id="g1", description="Launch", status=GoalStatus.ACTIVE, priority=1, progress=0.6)
    ]
    state.skills = [
        SkillState(name="market_research", status=SkillStatus.RUNNING, category="business", progress=0.5)
    ]
    state.memory = MemoryStats(episodic_count=10, semantic_count=5, total_size_mb=2.3)
    state.capabilities = [
        CapabilityState(name="web_search", category="web", health="healthy", backends=1, success_rate=0.95)
    ]
    state.recent_events = [
        EventEntry(id="e1", event_type="cognitive.state.changed", source="core", payload={"to": "executing"})
    ]
    state.health = {"status": "ok"}
    return state


def test_state_to_dict_roundtrip() -> None:
    state = _sample_state()
    data = state.to_dict()

    assert data["state"] == "executing"
    assert data["uptime_seconds"] == 42.5
    assert data["active_goals"][0]["id"] == "g1"
    assert data["active_goals"][0]["status"] == "active"
    assert data["skills"][0]["status"] == "running"
    assert data["memory"]["episodic_count"] == 10
    assert data["capabilities"][0]["health"] == "healthy"
    assert data["recent_events"][0]["event_type"] == "cognitive.state.changed"
    assert data["health"]["status"] == "ok"
    # Datetimes serialize as ISO strings
    assert data["active_goals"][0]["created_at"] is not None
    assert "T" in data["active_goals"][0]["created_at"]


def test_state_to_dict_json_serializable() -> None:
    data = _sample_state().to_dict()
    encoded = json.dumps(data, ensure_ascii=False)
    assert "executing" in encoded


def test_goal_state_values() -> None:
    goal = GoalState(description="x", status=GoalStatus.COMPLETED)
    assert goal.status.value == "completed"
    assert goal.progress == 0.0
    assert goal.id


def test_skill_and_memory_defaults() -> None:
    skill = SkillState()
    assert skill.status == SkillStatus.ENABLED
    memory = MemoryStats()
    assert memory.total_size_mb == 0.0


def test_updates_after_transition() -> None:
    state = CognitiveState(state=CognitiveStateEnum.PLANNING)
    state.state = CognitiveStateEnum.EXECUTING
    goal = GoalState(description="Ship it", status=GoalStatus.ACTIVE, priority=2)
    goal.status = GoalStatus.COMPLETED
    goal.completed_at = datetime.now(UTC)
    state.active_goals.append(goal)
    data = state.to_dict()
    assert data["state"] == "executing"
    assert data["active_goals"][0]["status"] == "completed"
    assert data["active_goals"][0]["completed_at"] is not None