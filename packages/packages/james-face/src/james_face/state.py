"""Cognitive state models for Face visualizer"""

from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import StrEnum
from uuid import uuid4


class CognitiveStateEnum(StrEnum):
    """JAMES cognitive states."""
    BOOTING = "booting"
    STARTING = "starting"
    RUNNING = "running"
    LISTENING = "listening"
    PLANNING = "planning"
    EXECUTING = "executing"
    VERIFYING = "verifying"
    LEARNING = "learning"
    REFLECTING = "reflecting"
    PAUSED = "paused"
    STOPPING = "stopping"
    STOPPED = "stopped"
    ERROR = "error"


class GoalStatus(StrEnum):
    PENDING = "pending"
    ACTIVE = "active"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


class SkillStatus(StrEnum):
    ENABLED = "enabled"
    DISABLED = "disabled"
    EXPERIMENTAL = "experimental"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"


@dataclass
class GoalState:
    id: str = field(default_factory=lambda: str(uuid4()))
    description: str = ""
    status: GoalStatus = GoalStatus.PENDING
    priority: int = 0
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    started_at: datetime | None = None
    completed_at: datetime | None = None
    progress: float = 0.0
    sub_goals: list[str] = field(default_factory=list)
    metadata: dict = field(default_factory=dict)


@dataclass
class SkillState:
    name: str = ""
    status: SkillStatus = SkillStatus.ENABLED
    category: str = ""
    current_step: str = ""
    progress: float = 0.0
    last_run: datetime | None = None
    success_count: int = 0
    failure_count: int = 0


@dataclass
class MemoryStats:
    episodic_count: int = 0
    semantic_count: int = 0
    procedural_count: int = 0
    business_count: int = 0
    user_count: int = 0
    total_size_mb: float = 0.0


@dataclass
class CapabilityState:
    name: str = ""
    category: str = ""
    health: str = "unknown"
    backends: int = 0
    last_used: datetime | None = None
    success_rate: float = 1.0


@dataclass
class EventEntry:
    id: str = field(default_factory=lambda: str(uuid4()))
    event_type: str = ""
    source: str = ""
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))
    payload: dict = field(default_factory=dict)
    severity: str = "info"


@dataclass
class CognitiveState:
    """Complete cognitive state snapshot for visualization."""

    # Core state
    state: CognitiveStateEnum = CognitiveStateEnum.BOOTING
    instance_id: str = field(default_factory=lambda: str(uuid4()))
    version: str = "0.1.0"
    uptime_seconds: float = 0.0

    # Goals
    active_goals: list[GoalState] = field(default_factory=list)
    completed_goals: list[GoalState] = field(default_factory=list)
    failed_goals: list[GoalState] = field(default_factory=list)

    # Skills
    skills: list[SkillState] = field(default_factory=list)
    active_skill: str | None = None

    # Memory
    memory: MemoryStats = field(default_factory=MemoryStats)

    # Capabilities
    capabilities: list[CapabilityState] = field(default_factory=list)

    # Events
    recent_events: list[EventEntry] = field(default_factory=list)

    # Health
    health: dict = field(default_factory=dict)

    # Metadata
    metadata: dict = field(default_factory=dict)

    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "state": self.state.value,
            "instance_id": self.instance_id,
            "version": self.version,
            "uptime_seconds": self.uptime_seconds,
            "active_goals": [
                {
                    "id": g.id,
                    "description": g.description,
                    "status": g.status.value,
                    "priority": g.priority,
                    "progress": g.progress,
                    "created_at": g.created_at.isoformat() if g.created_at else None,
                    "started_at": g.started_at.isoformat() if g.started_at else None,
                    "completed_at": g.completed_at.isoformat() if g.completed_at else None,
                    "sub_goals": g.sub_goals,
                }
                for g in self.active_goals
            ],
            "skills": [
                {
                    "name": s.name,
                    "status": s.status.value,
                    "category": s.category,
                    "current_step": s.current_step,
                    "progress": s.progress,
                    "last_run": s.last_run.isoformat() if s.last_run else None,
                    "success_count": s.success_count,
                    "failure_count": s.failure_count,
                }
                for s in self.skills
            ],
            "memory": {
                "episodic_count": self.memory.episodic_count,
                "semantic_count": self.memory.semantic_count,
                "procedural_count": self.memory.procedural_count,
                "business_count": self.memory.business_count,
                "user_count": self.memory.user_count,
                "total_size_mb": self.memory.total_size_mb,
            },
            "capabilities": [
                {
                    "name": c.name,
                    "category": c.category,
                    "health": c.health,
                    "backends": c.backends,
                    "last_used": c.last_used.isoformat() if c.last_used else None,
                    "success_rate": c.success_rate,
                }
                for c in self.capabilities
            ],
            "recent_events": [
                {
                    "id": e.id,
                    "event_type": e.event_type,
                    "source": e.source,
                    "timestamp": e.timestamp.isoformat(),
                    "payload": e.payload,
                    "severity": e.severity,
                }
                for e in self.recent_events
            ],
            "health": self.health,
        }


@dataclass
class StateSnapshot:
    """Timestamped state snapshot for history."""

    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))
    state: CognitiveState = field(default_factory=CognitiveState)

    def to_dict(self) -> dict:
        return {
            "timestamp": self.timestamp.isoformat(),
            "state": self.state.to_dict(),
        }