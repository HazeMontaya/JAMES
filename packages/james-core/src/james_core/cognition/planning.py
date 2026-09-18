"""Planning Engine for JAMES"""

from dataclasses import dataclass, field
from enum import Enum
from typing import Any
from uuid import uuid4

from .model_router import ModelRouter
from .reasoning import ReasoningEngine, ReasoningStrategy


class PlanStatus(Enum):
    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


@dataclass
class PlanStep:
    id: str = field(default_factory=lambda: str(uuid4()))
    name: str = ""
    description: str = ""
    skill_name: str | None = None
    inputs: dict[str, Any] = field(default_factory=dict)
    dependencies: list[str] = field(default_factory=list)
    status: PlanStatus = PlanStatus.PENDING
    result: Any | None = None
    error: str | None = None


@dataclass
class Plan:
    id: str = field(default_factory=lambda: str(uuid4()))
    goal_id: str = ""
    name: str = ""
    description: str = ""
    steps: list[PlanStep] = field(default_factory=list)
    status: PlanStatus = PlanStatus.PENDING
    metadata: dict[str, Any] = field(default_factory=dict)


class Planner:
    def __init__(self, model_router: ModelRouter, reasoning_engine: ReasoningEngine):
        self._model_router = model_router
        self._reasoning = reasoning_engine

    async def create_plan(
        self,
        goal_description: str,
        available_skills: list[str],
        context: str = "",
        constraints: list[str] | None = None,
    ) -> Plan:
        reasoning_result = await self._reasoning.reason(
            problem=f"Create a detailed execution plan for: {goal_description}",
            context=f"Available skills: {', '.join(available_skills)}\nContext: {context}\nConstraints: {constraints or []}",
            strategy=ReasoningStrategy.COT,
        )

        steps = self._parse_plan_steps(reasoning_result.conclusion, available_skills)

        plan = Plan(
            goal_id="",
            name=f"Plan for: {goal_description[:50]}",
            description=goal_description,
            steps=steps,
            metadata={
                "reasoning": reasoning_result.conclusion,
                "confidence": reasoning_result.overall_confidence,
            },
        )
        return plan

    def _parse_plan_steps(self, plan_text: str, available_skills: list[str]) -> list[PlanStep]:
        steps = []
        lines = plan_text.split("\n")
        current_step = None

        for line in lines:
            line = line.strip()
            if not line:
                continue

            if line.startswith(("Step", "step", "-", "1.", "2.", "3.", "4.", "5.")):
                if current_step:
                    steps.append(current_step)

                skill_match = None
                for skill in available_skills:
                    if skill.lower() in line.lower():
                        skill_match = skill
                        break

                current_step = PlanStep(
                    name=line[:100],
                    description=line,
                    skill_name=skill_match,
                )
            elif current_step:
                current_step.description += "\n" + line

        if current_step:
            steps.append(current_step)

        for i, step in enumerate(steps):
            if i > 0:
                step.dependencies.append(steps[i-1].id)

        return steps if steps else [
            PlanStep(name="Execute Goal", description=plan_text, skill_name=available_skills[0] if available_skills else None)
        ]

    async def decompose_goal(self, goal_description: str, max_depth: int = 3) -> list[str]:
        prompt = f"""Decompose this goal into {max_depth} levels of sub-goals:

Goal: {goal_description}

Format each level as:
Level 1: [sub-goal]
Level 2: [sub-goal]
...

Sub-goals:"""

        response = await self._model_router.generate(prompt, temperature=0.3)
        sub_goals = []

        for line in response.content.split("\n"):
            line = line.strip()
            if line.startswith(("Level", "level", "-", "1.", "2.", "3.")):
                sub_goals.append(line)

        return sub_goals

    async def estimate_effort(self, plan: Plan) -> dict[str, Any]:
        total_steps = len(plan.steps)
        skill_steps = len([s for s in plan.steps if s.skill_name])

        return {
            "total_steps": total_steps,
            "skill_steps": skill_steps,
            "estimated_duration_minutes": total_steps * 5,
            "complexity": "high" if total_steps > 10 else "medium" if total_steps > 5 else "low",
        }
