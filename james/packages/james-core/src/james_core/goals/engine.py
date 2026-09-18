"""Goals Engine for JAMES"""

from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
from typing import Any
from uuid import uuid4

from ..cognition.planning import Plan, Planner
from ..events.bus import EventBus
from ..events.models import GoalEvent


class GoalStatus(Enum):
    CREATED = "created"
    PLANNING = "planning"
    EXECUTING = "executing"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"
    PAUSED = "paused"


class GoalPriority(Enum):
    LOW = 1
    NORMAL = 2
    HIGH = 3
    CRITICAL = 4


@dataclass
class Goal:
    id: str = field(default_factory=lambda: str(uuid4()))
    description: str = ""
    status: GoalStatus = GoalStatus.CREATED
    priority: GoalPriority = GoalPriority.NORMAL
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    updated_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    started_at: datetime | None = None
    completed_at: datetime | None = None
    plan: Plan | None = None
    result: Any | None = None
    error: str | None = None
    metadata: dict[str, Any] = field(default_factory=dict)
    parent_goal_id: str | None = None
    sub_goal_ids: list[str] = field(default_factory=list)


class GoalEngine:
    def __init__(self, event_bus: EventBus, planner: Planner):
        self._event_bus = event_bus
        self._planner = planner
        self._goals: dict[str, Goal] = {}
        self._current_goal: Goal | None = None

    async def create_goal(
        self,
        description: str,
        priority: GoalPriority = GoalPriority.NORMAL,
        parent_goal_id: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> Goal:
        goal = Goal(
            description=description,
            priority=priority,
            parent_goal_id=parent_goal_id,
            metadata=metadata or {},
        )

        self._goals[goal.id] = goal

        if parent_goal_id and parent_goal_id in self._goals:
            self._goals[parent_goal_id].sub_goal_ids.append(goal.id)

        await self._event_bus.publish("goal.created", GoalEvent(goal_id=goal.id, action="created"))

        return goal

    async def plan_goal(self, goal_id: str, available_skills: list[str], context: str = "") -> Plan:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        goal.status = GoalStatus.PLANNING
        goal.updated_at = datetime.now(UTC)

        plan = await self._planner.create_plan(
            goal_description=goal.description,
            available_skills=available_skills,
            context=context,
        )
        plan.goal_id = goal_id
        goal.plan = plan

        await self._event_bus.publish("goal.planned", GoalEvent(goal_id=goal_id, action="planned"))

        return plan

    async def start_goal(self, goal_id: str) -> None:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        goal.status = GoalStatus.EXECUTING
        goal.started_at = datetime.now(UTC)
        goal.updated_at = datetime.now(UTC)
        self._current_goal = goal

        await self._event_bus.publish("goal.started", GoalEvent(goal_id=goal_id, action="started"))

    async def complete_goal(self, goal_id: str, result: Any = None) -> None:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        goal.status = GoalStatus.COMPLETED
        goal.completed_at = datetime.now(UTC)
        goal.updated_at = datetime.now(UTC)
        goal.result = result

        if self._current_goal and self._current_goal.id == goal_id:
            self._current_goal = None

        await self._event_bus.publish("goal.completed", GoalEvent(goal_id=goal_id, action="completed"))

    async def fail_goal(self, goal_id: str, error: str) -> None:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        goal.status = GoalStatus.FAILED
        goal.completed_at = datetime.now(UTC)
        goal.updated_at = datetime.now(UTC)
        goal.error = error

        if self._current_goal and self._current_goal.id == goal_id:
            self._current_goal = None

        await self._event_bus.publish("goal.failed", GoalEvent(goal_id=goal_id, action="failed"))

    async def pause_goal(self, goal_id: str) -> None:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        goal.status = GoalStatus.PAUSED
        goal.updated_at = datetime.now(UTC)

        await self._event_bus.publish("goal.paused", GoalEvent(goal_id=goal_id, action="paused"))

    async def resume_goal(self, goal_id: str) -> None:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        goal.status = GoalStatus.EXECUTING
        goal.updated_at = datetime.now(UTC)
        self._current_goal = goal

        await self._event_bus.publish("goal.resumed", GoalEvent(goal_id=goal_id, action="resumed"))

    async def cancel_goal(self, goal_id: str) -> None:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        goal.status = GoalStatus.CANCELLED
        goal.updated_at = datetime.now(UTC)

        for sub_id in goal.sub_goal_ids:
            if sub_id in self._goals:
                await self.cancel_goal(sub_id)

        if self._current_goal and self._current_goal.id == goal_id:
            self._current_goal = None

        await self._event_bus.publish("goal.cancelled", GoalEvent(goal_id=goal_id, action="cancelled"))

    def get_goal(self, goal_id: str) -> Goal | None:
        return self._goals.get(goal_id)

    def get_all_goals(self) -> list[Goal]:
        return list(self._goals.values())

    def get_active_goals(self) -> list[Goal]:
        return [g for g in self._goals.values() if g.status in (GoalStatus.PLANNING, GoalStatus.EXECUTING)]

    def get_current_goal(self) -> Goal | None:
        return self._current_goal

    async def decompose_goal(self, goal_id: str, max_depth: int = 2) -> list[Goal]:
        goal = self._goals.get(goal_id)
        if not goal:
            raise ValueError(f"Goal {goal_id} not found")

        sub_descriptions = await self._planner.decompose_goal(goal.description, max_depth)
        sub_goals = []

        for desc in sub_descriptions:
            sub_goal = await self.create_goal(
                description=desc,
                priority=GoalPriority(goal.priority.value - 1) if goal.priority.value > 1 else GoalPriority.LOW,
                parent_goal_id=goal_id,
            )
            sub_goals.append(sub_goal)

        return sub_goals
