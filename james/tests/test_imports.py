"""Test core imports and basic functionality"""

import pytest


def test_core_imports():
    """Test that core modules can be imported"""
    from james_core import (
        AgentOrchestrator,
        CognitiveState,
        CognitiveStateMachine,
        EventBus,
        GoalEngine,
        LearningEngine,
        ModelRouter,
        Planner,
        ReasoningEngine,
        ReflectionEngine,
        VerificationEngine,
    )
    assert ModelRouter is not None
    assert ReasoningEngine is not None
    assert Planner is not None
    assert ReflectionEngine is not None
    assert GoalEngine is not None
    assert AgentOrchestrator is not None
    assert EventBus is not None
    assert CognitiveStateMachine is not None
    assert CognitiveState is not None
    assert VerificationEngine is not None
    assert LearningEngine is not None


def test_memory_imports():
    """Test that memory modules can be imported"""
    from james_memory import (
        BusinessMemory,
        EpisodicMemory,
        MemoryRouter,
        ProceduralMemory,
        SemanticMemory,
        UserMemory,
        VaultManager,
    )
    assert EpisodicMemory is not None
    assert SemanticMemory is not None
    assert ProceduralMemory is not None
    assert BusinessMemory is not None
    assert UserMemory is not None
    assert MemoryRouter is not None
    assert VaultManager is not None


def test_skills_imports():
    """Test that skills modules can be imported"""
    import james_skills
    assert james_skills.__version__ == "0.1.0"


def test_world_imports():
    """Test that world modules can be imported"""
    import james_world
    assert james_world.__version__ == "0.1.0"


def test_cognitive_state_machine():
    """Test cognitive state machine transitions"""
    from james_core.events.state_machine import CognitiveState, CognitiveStateMachine

    machine = CognitiveStateMachine()

    assert machine.state == CognitiveState.IDLE
    assert machine.can_transition(CognitiveState.LISTENING)
    assert not machine.can_transition(CognitiveState.EXECUTING)

    import anyio
    async def test_transition():
        result = await machine.transition(CognitiveState.LISTENING)
        assert result is True
        assert machine.state == CognitiveState.LISTENING

        result = await machine.transition(CognitiveState.UNDERSTANDING)
        assert result is True
        assert machine.state == CognitiveState.UNDERSTANDING

        # Invalid transition
        result = await machine.transition(CognitiveState.EXECUTING)
        assert result is False

    anyio.run(test_transition)


def test_event_models():
    """Test event model creation"""

    from james_core.events.models import (
        CognitiveStateEvent,
        Event,
        ThoughtEvent,
    )
    from james_core.events.state_machine import CognitiveState

    event = Event(type="test", payload={"key": "value"})
    assert event.id is not None
    assert event.type == "test"
    assert event.payload["key"] == "value"

    state_event = CognitiveStateEvent(
        previous_state=CognitiveState.IDLE,
        current_state=CognitiveState.LISTENING,
    )
    assert state_event.type == "cognitive.state.changed"
    assert state_event.payload["previous_state"] == "idle"
    assert state_event.payload["current_state"] == "listening"

    thought_event = ThoughtEvent(
        thought_type="reasoning",
        content="Test thought",
        confidence=0.8,
    )
    assert thought_event.type == "cognitive.thought"
    assert thought_event.payload["thought_type"] == "reasoning"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
