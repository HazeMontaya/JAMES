"""Reflection Engine for JAMES"""

from dataclasses import dataclass
from typing import Any

from .model_router import ModelRouter
from .reasoning import ReasoningEngine


@dataclass
class ReflectionResult:
    insights: list[str]
    improvements: list[str]
    patterns_recognized: list[str]
    confidence: float
    metadata: dict[str, Any] | None = None


class ReflectionEngine:
    def __init__(self, model_router: ModelRouter, reasoning_engine: ReasoningEngine):
        self._model_router = model_router
        self._reasoning = reasoning_engine

    async def reflect_on_execution(
        self,
        goal: str,
        plan: str,
        execution_result: str,
        success: bool,
        metrics: dict[str, Any] | None = None,
    ) -> ReflectionResult:
        prompt = f"""Reflect on this execution:

Goal: {goal}
Plan: {plan}
Result: {execution_result}
Success: {success}
Metrics: {metrics or {}}

Analyze:
1. What worked well?
2. What could be improved?
3. What patterns do you recognize?
4. What would you do differently next time?

Provide structured insights."""

        response = await self._model_router.generate(prompt, temperature=0.4)

        insights = self._extract_insights(response.content, "insight", "worked", "good")
        improvements = self._extract_insights(response.content, "improve", "better", "different")
        patterns = self._extract_insights(response.content, "pattern", "recur", "similar")

        return ReflectionResult(
            insights=insights,
            improvements=improvements,
            patterns_recognized=patterns,
            confidence=0.7,
            metadata={"raw_reflection": response.content},
        )

    async def reflect_on_skill_performance(
        self,
        skill_name: str,
        executions: list[dict[str, Any]],
    ) -> ReflectionResult:
        exec_summary = "\n".join([
            f"- {e.get('goal', 'Unknown')}: {'Success' if e.get('success') else 'Failed'} "
            f"(confidence: {e.get('confidence', 0):.2f}, duration: {e.get('duration', 0):.1f}s)"
            for e in executions[-10:]
        ])

        prompt = f"""Analyze skill performance:

Skill: {skill_name}
Recent Executions:
{exec_summary}

Identify:
1. Strengths of this skill
2. Weaknesses or failure modes
3. Patterns in successful vs failed executions
4. Suggested improvements to the skill itself"""

        response = await self._model_router.generate(prompt, temperature=0.4)

        return ReflectionResult(
            insights=self._extract_insights(response.content, "strength", "good", "effective"),
            improvements=self._extract_insights(response.content, "improve", "weak", "fail"),
            patterns_recognized=self._extract_insights(response.content, "pattern", "trend", "correlat"),
            confidence=0.75,
            metadata={"skill": skill_name, "executions_analyzed": len(executions)},
        )

    async def meta_reflect(
        self,
        recent_reflections: list[ReflectionResult],
    ) -> ReflectionResult:
        if not recent_reflections:
            return ReflectionResult([], [], [], 0.0)

        summary = "\n".join([
            f"- Insights: {len(r.insights)}, Improvements: {len(r.improvements)}, Patterns: {len(r.patterns_recognized)}"
            for r in recent_reflections[-20:]
        ])

        prompt = f"""Meta-reflection on recent reflections:

{summary}

Identify meta-patterns:
1. Recurring themes across reflections
2. Systemic issues vs one-off problems
3. Evolution of performance over time
4. Strategic adjustments needed"""

        response = await self._model_router.generate(prompt, temperature=0.4)

        return ReflectionResult(
            insights=self._extract_insights(response.content, "theme", "recur", "systemic"),
            improvements=self._extract_insights(response.content, "strategic", "adjust", "change"),
            patterns_recognized=self._extract_insights(response.content, "evolution", "trend", "progress"),
            confidence=0.7,
            metadata={"reflections_analyzed": len(recent_reflections)},
        )

    def _extract_insights(self, text: str, *keywords: str) -> list[str]:
        insights = []
        for line in text.split("\n"):
            line = line.strip()
            if not line:
                continue
            if any(kw in line.lower() for kw in keywords):
                clean = line.lstrip("-•*1234567890. ").strip()
                if len(clean) > 10:
                    insights.append(clean)
        return insights[:10]
