"""Skill data models for JAMES"""

from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
from typing import Any
from uuid import uuid4


class SkillStatus(Enum):
    DISABLED = "disabled"
    ENABLED = "enabled"
    EXPERIMENTAL = "experimental"
    DEPRECATED = "deprecated"


@dataclass
class SkillInput:
    name: str = ""
    type: str = "string"
    required: bool = False
    description: str = ""
    default: Any = None
    enum: list[str] = field(default_factory=list)


@dataclass
class SkillOutput:
    name: str = ""
    type: str = "string"
    description: str = ""


@dataclass
class SkillStepSpec:
    id: str = ""
    type: str = "llm"  # llm | tool | skill | function
    name: str = ""
    description: str = ""
    skill_ref: str | None = None
    tool_ref: str | None = None
    function: str | None = None
    inputs: dict[str, Any] = field(default_factory=dict)
    inputs_from: dict[str, str] = field(default_factory=dict)
    expected_schema: dict[str, Any] | None = None


@dataclass
class VerificationSpec:
    type: str = "criteria"
    criteria: list[str] = field(default_factory=list)
    min_score: float = 0.7


@dataclass
class SkillSpec:
    name: str = ""
    version: str = "1.0.0"
    category: str = "general"
    description: str = ""
    purpose: str = ""
    status: SkillStatus = SkillStatus.ENABLED
    prerequisites: list[str] = field(default_factory=list)
    inputs: list[SkillInput] = field(default_factory=list)
    outputs: list[SkillOutput] = field(default_factory=list)
    steps: list[SkillStepSpec] = field(default_factory=list)
    metrics: list[str] = field(default_factory=list)
    verification: list[VerificationSpec] = field(default_factory=list)
    failure_modes: list[str] = field(default_factory=list)
    tags: list[str] = field(default_factory=list)
    # Runtime metadata (not from YAML necessarily)
    entry_point: str | None = None  # python module path
    async_execute: Callable[[dict[str, Any]], Awaitable[dict[str, Any]]] | None = None


# Runtime context/plan objects


@dataclass
class SkillContext:
    goal_id: str = ""
    inputs: dict[str, Any] = field(default_factory=dict)
    state: dict[str, Any] = field(default_factory=dict)
    memory: Any = None  # MemoryRouter
    world: Any = None  # CapabilityRegistry/Router
    model_router: Any = None
    event_bus: Any = None
    raw: dict[str, Any] = field(default_factory=dict)


@dataclass
class SkillStep:
    id: str = ""
    name: str = ""
    spec: SkillStepSpec = field(default_factory=SkillStepSpec)
    status: str = "pending"  # pending | running | completed | failed | skipped
    result: Any = None
    error: str | None = None
    started_at: datetime | None = None
    completed_at: datetime | None = None


@dataclass
class SkillPlan:
    skill_name: str = ""
    steps: list[SkillStep] = field(default_factory=list)
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))


@dataclass
class SkillResult:
    skill_name: str = ""
    success: bool = False
    outputs: dict[str, Any] = field(default_factory=dict)
    metrics: dict[str, float] = field(default_factory=dict)
    verification: dict[str, Any] = field(default_factory=dict)
    duration_ms: float = 0.0
    error: str | None = None
    execution_id: str = field(default_factory=lambda: str(uuid4()))
