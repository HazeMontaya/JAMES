"""JAMES goal runner - E2E wiring: goal -> skills -> memory/world/model -> report"""

import time
from typing import Any

import structlog
from james_core.audit import AuditLog, AuditOutcome, AuditSeverity
from james_core.cognition.model_router import ModelRouter, build_model_router
from james_core.config import JamesConfig
from james_core.events import Event, GoalEvent, InMemoryEventBus, SkillEvent
from james_memory import (
    BusinessMemory,
    EpisodicMemory,
    MemoryRouter,
    MemoryType,
    ProceduralMemory,
    SemanticMemory,
    UserMemory,
)
from james_skills import SkillEngine
from james_skills.engine.models import SkillContext
from james_world import WorldAccess

logger = structlog.get_logger()

__all__ = ["GoalRunner", "build_runner_components", "run_goal"]

VALID_PRIORITIES = ("low", "normal", "high", "critical")


def build_runner_components(config: JamesConfig) -> dict[str, Any]:
    """Create all runtime components wired to the given config."""
    memory = MemoryRouter(
        episodic=EpisodicMemory(str(config.paths.memory / "episodic.db")),
        semantic=SemanticMemory(str(config.paths.vault)),
        procedural=ProceduralMemory(str(config.paths.memory / "procedural.db")),
        business=BusinessMemory(str(config.paths.memory / "business.db")),
        user=UserMemory(str(config.paths.vault)),
    )

    engines = SkillEngine(skill_dirs=[str(config.paths.skills)])

    router: ModelRouter | None = None
    if config.enabled_model_endpoints():
        router = build_model_router(config)

    return {
        "memory": memory,
        "world": WorldAccess(),
        "engine": engines,
        "model_router": router,
        "event_bus": InMemoryEventBus(),
        "config": config,
        "audit": AuditLog(str(config.paths.memory / "audit.db")),
    }


class GoalRunner:
    """Runs the goal-to-report pipeline without voice."""

    def __init__(self, config: JamesConfig | None = None):
        self.config = config or JamesConfig()
        self.components = build_runner_components(self.config)

    async def initialize(self) -> None:
        await self.components["world"].on_start()
        await self.components["audit"].initialize()

    async def run(self, goal: str, priority: str = "normal", skills: list[str] | None = None) -> dict[str, Any]:
        """Compose skills for the goal, execute them, persist and return a report."""
        if priority not in VALID_PRIORITIES:
            raise ValueError(f"invalid priority '{priority}', expected one of {', '.join(VALID_PRIORITIES)}")

        engine: SkillEngine = self.components["engine"]
        memory: MemoryRouter = self.components["memory"]
        world: WorldAccess = self.components["world"]
        model_router: ModelRouter | None = self.components["model_router"]
        event_bus: InMemoryEventBus = self.components["event_bus"]
        audit: AuditLog = self.components["audit"]
        start = time.monotonic()

        await event_bus.connect()
        await event_bus.publish(
            "james.goal.started",
            GoalEvent(goal_id=goal, action="started"),
        )

        await audit.record(
            actor="cli",
            action="goal.run",
            resource=goal,
            details={"actor_target": "cli", "goal": goal, "priority": priority},
        )

        composition = engine.compose(goal, available=skills) if skills else engine.compose(goal)

        if not composition.skills:
            await event_bus.publish(
                "james.goal.finished",
                Event(type="goal.finished", payload={"goal": goal, "success": False, "reason": "no skills matched goal"}),
            )
            await audit.record(
                actor="cli",
                action="goal.run",
                resource=goal,
                outcome=AuditOutcome.FAILURE,
                severity=AuditSeverity.WARNING,
                details={"goal": goal, "priority": priority, "reason": "no skills matched goal"},
            )
            return {"goal": goal, "skills": [], "report": "", "success": False, "message": "no skills matched goal"}

        reports: list[str] = []
        skill_names: list[str] = []

        for step in composition.skills:
            spec = engine.get_skill(step.skill_name)
            if spec is None:
                continue
            skill_names.append(step.skill_name)

            await event_bus.publish(
                "james.skill.started",
                SkillEvent(skill_name=step.skill_name, action="started"),
            )

            inputs = dict(step.inputs)
            if not any(k in inputs for k in ("topic", "goal", "subject", "problem", "instructions")):
                inputs["topic"] = goal

            ctx = SkillContext(
                goal_id=goal,
                inputs=inputs,
                memory=memory,
                world=world,
                model_router=model_router,
                event_bus=event_bus,
            )

            logger.info("Executing skill", skill=step.skill_name)
            skill_started = time.monotonic()
            try:
                result = await engine.execute(spec, ctx)
            except Exception as e:  # pragma: no cover - defensive
                logger.error("Skill execution failed", skill=step.skill_name, error=str(e))
                await audit.record(
                    actor="cli",
                    action="skill.run",
                    resource=f"{spec.name}",
                    outcome=AuditOutcome.FAILURE,
                    severity=AuditSeverity.ERROR,
                    details={"skill": spec.name, "goal": goal, "priority": priority, "error": str(e)},
                    duration_ms=(time.monotonic() - skill_started) * 1000,
                )
                await event_bus.publish(
                    "james.skill.finished",
                    SkillEvent(skill_name=step.skill_name, action="failed"),
                )
                result = None

            if result is not None:
                await audit.record(
                    actor="cli",
                    action="skill.run",
                    resource=step.skill_name,
                    outcome=AuditOutcome.SUCCESS if result.success else AuditOutcome.FAILURE,
                    severity=AuditSeverity.WARNING if not result.success else AuditSeverity.INFO,
                    details={"skill": step.skill_name, "goal": goal, "priority": priority, "success": result.success},
                    duration_ms=result.duration_ms,
                )
                await event_bus.publish(
                    "james.skill.finished",
                    SkillEvent(skill_name=step.skill_name, action="finished", step_id=step.skill_name),
                )
                report_text = self._collect_report(result.outputs)
                reports.append(report_text)
                await memory.store(
                    content=f"{spec.name} run for goal '{goal}' completed success={result.success}",
                    memory_type=MemoryType.EPISODIC,
                    metadata={"skill": spec.name, "goal": goal, "priority": priority, "success": result.success},
                    tags=[spec.name, "run"],
                    source="GoalRunner",
                )

        report = self._assemble_reports(goal, skill_names, reports, priority=priority)

        await memory.store(
            content=report,
            memory_type=MemoryType.BUSINESS,
            metadata={"goal": goal, "skills": skill_names, "priority": priority},
            tags=["goal", "report"],
            source="GoalRunner",
        )

        success = bool(reports)
        await audit.record(
            actor="cli",
            action="goal.run",
            resource=goal,
            outcome=AuditOutcome.SUCCESS if success else AuditOutcome.FAILURE,
            severity=AuditSeverity.WARNING if not success else AuditSeverity.INFO,
            details={"goal": goal, "skills": skill_names, "priority": priority, "success": success},
            duration_ms=(time.monotonic() - start) * 1000,
        )
        await event_bus.publish(
            "james.goal.finished",
            GoalEvent(goal_id=goal, action="finished"),
        )

        return {
            "goal": goal,
            "skills": skill_names,
            "report": report,
            "success": success,
        }

    @staticmethod
    def _collect_report(outputs: dict[str, Any]) -> str:
        parts: list[str] = []
        for value in outputs.values():
            if isinstance(value, dict):
                text = value.get("text", "")
                if text:
                    parts.append(str(text))
                else:
                    import json

                    parts.append(json.dumps(value, ensure_ascii=False, indent=2, default=str))
            else:
                parts.append(str(value))
        return "\n".join(parts).strip()

    @staticmethod
    def _assemble_reports(goal: str, skill_names: list[str], reports: list[str], priority: str = "normal") -> str:
        header = f"# JAMES Report\n\n**Goal:** {goal}\n**Priority:** {priority}\n**Skills:** {', '.join(skill_names) or 'none'}\n"
        body = "\n\n---\n\n".join(reports) if reports else "_No skills produced output._"
        return f"{header}\n\n{body}"


async def run_goal(
    goal: str,
    config: JamesConfig | None = None,
    priority: str = "normal",
    skills: list[str] | None = None,
) -> dict[str, Any]:
    """Convenience wrapper - builds and runs the pipeline for a single goal."""
    runner = GoalRunner(config)
    await runner.initialize()
    try:
        return await runner.run(goal, priority=priority, skills=skills)
    finally:
        await runner.components["memory"].close()
        await runner.components["audit"].close()
        await runner.components["event_bus"].close()
        router: ModelRouter | None = runner.components["model_router"]
        if router is not None:
            for backend in router.get_healthy_backends():
                await backend.close()