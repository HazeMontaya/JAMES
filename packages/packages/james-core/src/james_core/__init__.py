"""JAMES Cognitive Core - Reasoning, Planning, Agent Orchestration"""

from .agents.orchestrator import AgentOrchestrator
from .agents.registry import AgentRegistry
from .audit import AuditLog, AuditOutcome, AuditSeverity
from .cognition.model_router import ModelRouter
from .cognition.planning import Planner
from .cognition.reasoning import ReasoningEngine
from .cognition.reflection import ReflectionEngine
from .events.bus import EventBus
from .events.models import CognitiveStateEvent, Event, ThoughtEvent
from .events.state_machine import CognitiveState, CognitiveStateMachine
from .goals.engine import GoalEngine
from .goals.models import Goal, GoalPriority, GoalStatus
from .learning.engine import LearningEngine
from .security import redact_secrets
from .verification.engine import VerificationEngine

__version__ = "0.1.0"

__all__ = [
    "AgentOrchestrator",
    "AgentRegistry",
    "AuditLog",
    "AuditOutcome",
    "AuditSeverity",
    "CognitiveState",
    "CognitiveStateEvent",
    "CognitiveStateMachine",
    "Event",
    "EventBus",
    "Goal",
    "GoalEngine",
    "GoalPriority",
    "GoalStatus",
    "LearningEngine",
    "ModelRouter",
    "Planner",
    "ReasoningEngine",
    "ReflectionEngine",
    "ThoughtEvent",
    "VerificationEngine",
    "redact_secrets",
]
