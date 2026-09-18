"""Skill executor - runs Plan -> Execute -> Verify -> Learn"""

import time
from datetime import UTC, datetime
from typing import Any

import structlog

from ..engine.registry import SkillRegistry
from .models import (
    SkillContext,
    SkillPlan,
    SkillResult,
    SkillSpec,
    SkillStep,
    SkillStepSpec,
)

logger = structlog.get_logger()


class SkillExecutor:
    def __init__(self, registry: SkillRegistry):
        self._registry = registry

    async def plan(self, spec: SkillSpec, ctx: SkillContext) -> SkillPlan:
        steps = [
            SkillStep(id=step.id or f"step_{i}", name=step.name or step.id, spec=step)
            for i, step in enumerate(spec.steps)
        ]
        return SkillPlan(skill_name=spec.name, steps=steps)

    async def execute(self, spec: SkillSpec, ctx: SkillContext, plan: SkillPlan | None = None) -> SkillResult:
        start = time.monotonic()
        plan = plan or await self.plan(spec, ctx)
        outputs: dict[str, Any] = {}

        for step in plan.steps:
            step.status = "running"
            step.started_at = datetime.now(UTC)
            try:
                result = await self._run_step(spec, step.spec, ctx, outputs)
                step.result = result
                step.status = "completed"
                step.completed_at = datetime.now(UTC)
                step_name = step.spec.id or step.name
                outputs[step_name] = result
                ctx.state[step_name] = result
            except Exception as e:
                step.status = "failed"
                step.error = str(e)
                step.completed_at = datetime.now(UTC)
                logger.error("Skill step failed", skill=spec.name, step=step.spec.id, error=str(e))

                # If a python async_execute exists, try fallback
                failure_result = await self._fallback(spec, ctx, outputs, step, str(e))
                if failure_result:
                    step.status = "completed"
                    step.result = failure_result
                    outputs[step.spec.id or step.name] = failure_result
                    ctx.state[step.spec.id or step.name] = failure_result

        duration_ms = (time.monotonic() - start) * 1000

        # Verification
        verification = await self._verify(spec, outputs, ctx)

        success = all(s.status == "completed" for s in plan.steps)
        return SkillResult(
            skill_name=spec.name,
            success=success,
            outputs=outputs,
            verification=verification,
            duration_ms=duration_ms,
            error=None if success else "; ".join(s.error or "" for s in plan.steps if s.error),
        )

    async def _run_step(
        self,
        spec: SkillSpec,
        step: SkillStepSpec,
        ctx: SkillContext,
        outputs: dict[str, Any],
    ) -> Any:
        # Resolve inputs: goal-level context first, step-level overrides second
        resolved = dict(ctx.inputs)
        resolved.update(step.inputs)
        for src_name, src_key in step.inputs_from.items():
            if src_key in ctx.state:
                resolved[src_name] = ctx.state[src_key]

        if step.type == "skill":
            if not step.skill_ref:
                raise ValueError("Skill step without skill_ref")
            sub_spec = self._registry.get(step.skill_ref)
            if not sub_spec:
                raise ValueError(f"Referenced skill not found: {step.skill_ref}")
            sub_ctx = SkillContext(
                goal_id=ctx.goal_id,
                inputs=resolved,
                state={k: v for k, v in ctx.state.items()},
                memory=ctx.memory,
                world=ctx.world,
                model_router=ctx.model_router,
                event_bus=ctx.event_bus,
                raw=ctx.raw,
            )
            result = await self.execute(sub_spec, sub_ctx)
            if not result.success:
                raise RuntimeError(f"Sub-skill {step.skill_ref} failed: {result.error}")
            return result.outputs

        if step.type == "tool":
            if not ctx.world:
                # No world access: fall back to the skill's Python entry point (deterministic offline run)
                return await self._offline_entry_or_raise(spec, resolved, ctx)
            return await self._call_tool(step.tool_ref, resolved, ctx)

        if step.type == "function":
            if not spec.async_execute:
                raise ValueError(f"Skill has no callable entry point for function step: {step.function}")
            merged = {**resolved, "context": ctx}
            return await spec.async_execute(merged)

        # Default: LLM step
        if not ctx.model_router:
            # Fall back to the skill's Python entry point (local deterministic run)
            return await self._offline_entry_or_mock(spec, resolved, ctx)
        return await self._call_llm(spec, step, resolved, ctx)

    async def _offline_entry_or_raise(self, spec: SkillSpec, resolved: dict[str, Any], ctx: SkillContext) -> Any:
        if spec.async_execute is not None:
            return await spec.async_execute({**resolved, "context": ctx})
        raise ValueError("World access not available for tool step")

    async def _offline_entry_or_mock(self, spec: SkillSpec, resolved: dict[str, Any], ctx: SkillContext) -> dict[str, Any]:
        if spec.async_execute is not None:
            return await spec.async_execute({**resolved, "context": ctx})
        return {"text": "", "note": "no model router available; mock result"}

    async def _call_tool(self, tool_ref: str | None, inputs: dict[str, Any], ctx: SkillContext) -> Any:
        if not tool_ref:
            raise ValueError("Tool step without tool_ref")
        try:
            fn = getattr(ctx.world, tool_ref)
            return await fn(**inputs)
        except AttributeError:
            raise ValueError(f"Tool not available on world: {tool_ref}")

    async def _call_llm(
        self,
        spec: SkillSpec,
        step: SkillStepSpec,
        inputs: dict[str, Any],
        ctx: SkillContext,
    ) -> dict[str, Any]:
        instruction = f"""Skill: {spec.name}
Step: {step.name or step.id}
Description: {step.description or ''}

Context inputs:
{self._format_inputs(inputs)}

Produce the requested output as structured content."""
        response = await ctx.model_router.generate(instruction, temperature=0.3)
        return {"text": response.content, "model": response.model}

    def _format_inputs(self, inputs: dict[str, Any]) -> str:
        parts = []
        for k, v in inputs.items():
            try:
                import json

                parts.append(f"{k}: {json.dumps(v, indent=2)[:2000]}")
            except Exception:
                parts.append(f"{k}: {v}")
        return "\n".join(parts)

    async def _verify(self, spec: SkillSpec, outputs: dict[str, Any], ctx: SkillContext) -> dict[str, Any]:
        results = []
        for v in spec.verification:
            score = 0.0
            checks = 0
            for criterion in v.criteria:
                checks += 1
                if self._check_criterion(criterion, outputs):
                    score += 1.0
            passed = (score / max(checks, 1)) >= v.min_score
            results.append({"type": v.type, "passed": passed, "score": score / max(checks, 1)})
        return {"satisfied": all(r["passed"] for r in results), "checks": results}

    def _check_criterion(self, criterion: str, outputs: dict[str, Any]) -> bool:
        crit = criterion.lower()
        if not outputs:
            return False
        text = " ".join(str(v)[:5000] for v in outputs.values()).lower()
        if crit.startswith("contains:"):
            return crit.split(":", 1)[1].strip() in text
        if crit.startswith("min_length:"):
            try:
                n = int(crit.split(":", 1)[1].strip())
                return len(text) >= n
            except ValueError:
                return True
        if crit in ("has_outputs", "output_present", "non_empty"):
            return bool(text.strip())
        if crit in ("no_error", "no_exception"):
            return True
        # default: keyword presence
        words = crit.replace("_", " ").split()
        return all(w in text for w in words)

    async def _fallback(
        self,
        spec: SkillSpec,
        ctx: SkillContext,
        outputs: dict[str, Any],
        step: SkillStep,
        error: str,
    ) -> Any | None:
        return None
