"""Agent Orchestrator for JAMES"""

from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
from typing import Any
from uuid import uuid4

import anyio
import structlog

from ..cognition.planning import PlanStatus, PlanStep
from ..events.bus import EventBus
from ..events.models import Event

logger = structlog.get_logger()


class AgentType(Enum):
    RESEARCHER = "researcher"
    ANALYST = "analyst"
    WRITER = "writer"
    CODER = "coder"
    PLANNER = "planner"
    VERIFIER = "verifier"
    GENERAL = "general"


class AgentStatus(Enum):
    IDLE = "idle"
    RUNNING = "running"
    WAITING = "waiting"
    COMPLETED = "completed"
    FAILED = "failed"


@dataclass
class Agent:
    id: str = field(default_factory=lambda: str(uuid4()))
    name: str = ""
    agent_type: AgentType = AgentType.GENERAL
    capabilities: list[str] = field(default_factory=list)
    status: AgentStatus = AgentStatus.IDLE
    current_task: str | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class AgentTask:
    id: str = field(default_factory=lambda: str(uuid4()))
    goal_id: str = ""
    plan_step_id: str = ""
    description: str = ""
    assigned_agent_id: str | None = None
    status: AgentStatus = AgentStatus.IDLE
    result: Any = None
    error: str | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    started_at: datetime | None = None
    completed_at: datetime | None = None


class AgentOrchestrator:
    def __init__(self, event_bus: EventBus):
        self._event_bus = event_bus
        self._agents: dict[str, Agent] = {}
        self._tasks: dict[str, AgentTask] = {}
        self._task_handlers: dict[str, Callable[[AgentTask], Awaitable[Any]]] = {}
        self._lock = anyio.Lock()

    def register_agent(self, agent: Agent) -> None:
        self._agents[agent.id] = agent
        logger.info("Agent registered", agent_id=agent.id, name=agent.name, type=agent.agent_type.value)

    def unregister_agent(self, agent_id: str) -> None:
        if agent_id in self._agents:
            del self._agents[agent_id]
            logger.info("Agent unregistered", agent_id=agent_id)

    def register_handler(self, capability: str, handler: Callable[[AgentTask], Awaitable[Any]]) -> None:
        self._task_handlers[capability] = handler

    def get_agent(self, agent_id: str) -> Agent | None:
        return self._agents.get(agent_id)

    def get_available_agents(self, capability: str | None = None) -> list[Agent]:
        agents = [a for a in self._agents.values() if a.status == AgentStatus.IDLE]
        if capability:
            agents = [a for a in agents if capability in a.capabilities]
        return agents

    async def create_task(
        self,
        goal_id: str,
        plan_step_id: str,
        description: str,
        required_capability: str,
    ) -> AgentTask:
        task = AgentTask(
            goal_id=goal_id,
            plan_step_id=plan_step_id,
            description=description,
        )
        self._tasks[task.id] = task

        await self._event_bus.publish("agent.task.created", Event(
            type="agent.task.created",
            payload={"task_id": task.id, "goal_id": goal_id, "capability": required_capability}
        ))

        return task

    async def assign_task(self, task_id: str, agent_id: str) -> bool:
        task = self._tasks.get(task_id)
        agent = self._agents.get(agent_id)

        if not task or not agent:
            return False

        if agent.status != AgentStatus.IDLE:
            return False

        task.assigned_agent_id = agent_id
        task.status = AgentStatus.RUNNING
        task.started_at = datetime.now(UTC)
        agent.status = AgentStatus.RUNNING
        agent.current_task = task_id

        handler = self._task_handlers.get("default")
        if not handler:
            for cap in agent.capabilities:
                if cap in self._task_handlers:
                    handler = self._task_handlers[cap]
                    break

        if handler:
            try:
                result = await handler(task)
                await self.complete_task(task_id, result)
            except Exception as e:
                await self.fail_task(task_id, str(e))
        else:
            await self.fail_task(task_id, "No handler registered for task")

        return True

    async def complete_task(self, task_id: str, result: Any) -> None:
        task = self._tasks.get(task_id)
        if not task:
            return

        task.status = AgentStatus.COMPLETED
        task.completed_at = datetime.now(UTC)
        task.result = result

        if task.assigned_agent_id and task.assigned_agent_id in self._agents:
            agent = self._agents[task.assigned_agent_id]
            agent.status = AgentStatus.IDLE
            agent.current_task = None

        await self._event_bus.publish("agent.task.completed", Event(
            type="agent.task.completed",
            payload={"task_id": task_id, "goal_id": task.goal_id, "result": str(result)[:100]}
        ))

    async def fail_task(self, task_id: str, error: str) -> None:
        task = self._tasks.get(task_id)
        if not task:
            return

        task.status = AgentStatus.FAILED
        task.completed_at = datetime.now(UTC)
        task.error = error

        if task.assigned_agent_id and task.assigned_agent_id in self._agents:
            agent = self._agents[task.assigned_agent_id]
            agent.status = AgentStatus.IDLE
            agent.current_task = None

        await self._event_bus.publish("agent.task.failed", Event(
            type="agent.task.failed",
            payload={"task_id": task_id, "goal_id": task.goal_id, "error": error}
        ))

    async def execute_plan_steps(
        self,
        goal_id: str,
        plan_steps: list[PlanStep],
        capability_map: dict[str, str],
    ) -> dict[str, Any]:
        results: dict[str, Any] = {}

        for step in plan_steps:
            capability = capability_map.get(step.skill_name or "", "general")

            task = await self.create_task(goal_id, step.id, step.description, capability)
            agents = self.get_available_agents(capability)

            if not agents:
                step.status = PlanStatus.FAILED
                step.error = f"No available agent for capability: {capability}"
                results[step.id] = {"error": step.error}
                continue

            agent = agents[0]
            assigned = await self.assign_task(task.id, agent.id)

            if assigned:
                while task.status == AgentStatus.RUNNING:
                    await anyio.sleep(0.5)

                if task.status == AgentStatus.COMPLETED:
                    step.status = PlanStatus.COMPLETED
                    step.result = task.result
                    results[step.id] = task.result
                else:
                    step.status = PlanStatus.FAILED
                    step.error = task.error
                    results[step.id] = {"error": task.error}
            else:
                step.status = PlanStatus.FAILED
                step.error = "Failed to assign task"
                results[step.id] = {"error": step.error}

        return results
