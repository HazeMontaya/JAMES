"""Agent Delegation for JAMES"""

from dataclasses import dataclass
from typing import Any

from ..agents.orchestrator import AgentOrchestrator
from ..cognition.planning import Plan
from ..goals.engine import Goal


@dataclass
class DelegationPlan:
    goal_id: str
    task_assignments: dict[str, str]  # task_id -> agent_id
    parallel_groups: list[list[str]]  # task_ids that can run in parallel


class DelegationEngine:
    def __init__(self, orchestrator: AgentOrchestrator):
        self._orchestrator = orchestrator

    async def create_delegation_plan(
        self,
        goal: Goal,
        plan: Plan,
    ) -> DelegationPlan:
        task_assignments = {}
        parallel_groups = []
        current_group = []

        for step in plan.steps:
            capability = step.skill_name or "general"
            agents = self._orchestrator.get_available_agents(capability)

            if not agents:
                continue

            task = await self._orchestrator.create_task(
                goal_id=goal.id,
                plan_step_id=step.id,
                description=step.description,
                required_capability=capability,
            )

            agent = agents[0]
            task_assignments[task.id] = agent.id
            current_group.append(task.id)

            if len(current_group) >= 3:
                parallel_groups.append(current_group)
                current_group = []

        if current_group:
            parallel_groups.append(current_group)

        return DelegationPlan(
            goal_id=goal.id,
            task_assignments=task_assignments,
            parallel_groups=parallel_groups,
        )

    async def execute_delegation(self, delegation: DelegationPlan) -> dict[str, Any]:
        results = {}

        for group in delegation.parallel_groups:
            tasks = []
            for task_id in group:
                agent_id = delegation.task_assignments.get(task_id)
                if agent_id:
                    tasks.append(self._orchestrator.assign_task(task_id, agent_id))

            if tasks:
                await asyncio.gather(*tasks)

            for task_id in group:
                task = self._orchestrator._tasks.get(task_id)
                if task:
                    results[task_id] = task.result if task.status.value == "completed" else {"error": task.error}

        return results

import asyncio
