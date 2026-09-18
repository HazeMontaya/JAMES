"""Event models for JAMES Cognitive Core"""

from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any
from uuid import uuid4

from .state_machine import CognitiveState


@dataclass
class Event:
    id: str = field(default_factory=lambda: str(uuid4()))
    type: str = ""
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))
    payload: dict[str, Any] = field(default_factory=dict)
    correlation_id: str | None = None
    causation_id: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "type": self.type,
            "timestamp": self.timestamp.isoformat(),
            "payload": self.payload,
            "correlation_id": self.correlation_id,
            "causation_id": self.causation_id,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Event":
        return cls(
            id=data.get("id", str(uuid4())),
            type=data.get("type", ""),
            timestamp=datetime.fromisoformat(data["timestamp"]) if "timestamp" in data else datetime.now(UTC),
            payload=data.get("payload", {}),
            correlation_id=data.get("correlation_id"),
            causation_id=data.get("causation_id"),
        )


@dataclass
class CognitiveStateEvent(Event):
    previous_state: CognitiveState = CognitiveState.IDLE
    current_state: CognitiveState = CognitiveState.IDLE
    metadata: dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        self.type = "cognitive.state.changed"
        self.payload = {
            "previous_state": self.previous_state.value,
            "current_state": self.current_state.value,
            "metadata": self.metadata,
        }


@dataclass
class ThoughtEvent(Event):
    thought_type: str = "reasoning"
    content: str = ""
    confidence: float = 0.0
    metadata: dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        self.type = "cognitive.thought"
        self.payload = {
            "thought_type": self.thought_type,
            "content": self.content,
            "confidence": self.confidence,
            "metadata": self.metadata,
        }


@dataclass
class GoalEvent(Event):
    goal_id: str = ""
    action: str = "created"

    def __post_init__(self) -> None:
        self.type = f"goal.{self.action}"
        self.payload = {"goal_id": self.goal_id, "action": self.action}


@dataclass
class SkillEvent(Event):
    skill_name: str = ""
    step_id: str = ""
    action: str = "started"

    def __post_init__(self) -> None:
        self.type = f"skill.{self.action}"
        self.payload = {"skill_name": self.skill_name, "step_id": self.step_id, "action": self.action}


@dataclass
class MemoryEvent(Event):
    memory_type: str = ""
    operation: str = ""
    keys: list[str] = field(default_factory=list)

    def __post_init__(self) -> None:
        self.type = f"memory.{self.operation}"
        self.payload = {"memory_type": self.memory_type, "operation": self.operation, "keys": self.keys}


@dataclass
class VerificationEvent(Event):
    verification_id: str = ""
    passed: bool = False
    score: float = 0.0
    details: dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        self.type = "verification.result"
        self.payload = {
            "verification_id": self.verification_id,
            "passed": self.passed,
            "score": self.score,
            "details": self.details,
        }
