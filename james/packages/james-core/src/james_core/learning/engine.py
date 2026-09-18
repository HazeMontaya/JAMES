"""Learning Engine for JAMES"""

from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any
from uuid import uuid4

import structlog

from ..cognition.reflection import ReflectionEngine
from ..events.bus import EventBus
from ..events.models import Event

logger = structlog.get_logger()


@dataclass
class LearnedPattern:
    id: str = field(default_factory=lambda: str(uuid4()))
    pattern_type: str = ""
    description: str = ""
    context: dict[str, Any] = field(default_factory=dict)
    conditions: list[str] = field(default_factory=list)
    actions: list[str] = field(default_factory=list)
    confidence: float = 0.0
    usage_count: int = 0
    success_rate: float = 0.0
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    updated_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class SkillMemory:
    skill_name: str
    executions: list[dict[str, Any]] = field(default_factory=list)
    learned_patterns: list[LearnedPattern] = field(default_factory=list)
    performance_metrics: dict[str, float] = field(default_factory=dict)
    last_updated: datetime = field(default_factory=lambda: datetime.now(UTC))


class LearningEngine:
    def __init__(self, event_bus: EventBus, reflection_engine: ReflectionEngine):
        self._event_bus = event_bus
        self._reflection = reflection_engine
        self._skill_memories: dict[str, SkillMemory] = {}
        self._global_patterns: list[LearnedPattern] = []

    def get_skill_memory(self, skill_name: str) -> SkillMemory:
        if skill_name not in self._skill_memories:
            self._skill_memories[skill_name] = SkillMemory(skill_name=skill_name)
        return self._skill_memories[skill_name]

    async def record_execution(
        self,
        skill_name: str,
        goal: str,
        plan: str,
        result: Any,
        success: bool,
        confidence: float,
        duration: float,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        memory = self.get_skill_memory(skill_name)

        execution = {
            "goal": goal,
            "plan": plan,
            "result": str(result)[:500],
            "success": success,
            "confidence": confidence,
            "duration": duration,
            "timestamp": datetime.now(UTC).isoformat(),
            "metadata": metadata or {},
        }

        memory.executions.append(execution)
        if len(memory.executions) > 100:
            memory.executions = memory.executions[-100:]

        memory.last_updated = datetime.now(UTC)
        self._update_performance_metrics(memory)

        await self._event_bus.publish("learning.execution_recorded", Event(
            type="learning.execution_recorded",
            payload={"skill": skill_name, "success": success, "confidence": confidence}
        ))

    def _update_performance_metrics(self, memory: SkillMemory) -> None:
        if not memory.executions:
            return

        total = len(memory.executions)
        successful = sum(1 for e in memory.executions if e["success"])
        avg_confidence = sum(e["confidence"] for e in memory.executions) / total
        avg_duration = sum(e["duration"] for e in memory.executions) / total

        memory.performance_metrics = {
            "total_executions": total,
            "success_rate": successful / total,
            "avg_confidence": avg_confidence,
            "avg_duration": avg_duration,
        }

    async def extract_patterns(self, skill_name: str) -> list[LearnedPattern]:
        memory = self.get_skill_memory(skill_name)

        if len(memory.executions) < 5:
            return []

        reflection = await self._reflection.reflect_on_skill_performance(
            skill_name=skill_name,
            executions=memory.executions[-20:],
        )

        patterns = []
        for insight in reflection.insights:
            pattern = LearnedPattern(
                pattern_type="insight",
                description=insight,
                context={"skill": skill_name},
                confidence=reflection.confidence,
                metadata={"source": "reflection", "type": "insight"},
            )
            patterns.append(pattern)

        for improvement in reflection.improvements:
            pattern = LearnedPattern(
                pattern_type="improvement",
                description=improvement,
                context={"skill": skill_name},
                confidence=reflection.confidence,
                metadata={"source": "reflection", "type": "improvement"},
            )
            patterns.append(pattern)

        for pattern_rec in reflection.patterns_recognized:
            pattern = LearnedPattern(
                pattern_type="recognized_pattern",
                description=pattern_rec,
                context={"skill": skill_name},
                confidence=reflection.confidence,
                metadata={"source": "reflection", "type": "pattern"},
            )
            patterns.append(pattern)

        memory.learned_patterns.extend(patterns)
        self._global_patterns.extend(patterns)

        for pattern in patterns:
            await self._event_bus.publish("learning.pattern_discovered", Event(
                type="learning.pattern_discovered",
                payload={
                    "pattern_id": pattern.id,
                    "skill": skill_name,
                    "type": pattern.pattern_type,
                    "confidence": pattern.confidence,
                }
            ))

        return patterns

    async def get_applicable_patterns(
        self,
        skill_name: str,
        context: dict[str, Any],
    ) -> list[LearnedPattern]:
        memory = self.get_skill_memory(skill_name)
        applicable = []

        for pattern in memory.learned_patterns:
            if self._pattern_matches(pattern, context):
                applicable.append(pattern)

        for pattern in self._global_patterns:
            if self._pattern_matches(pattern, context):
                applicable.append(pattern)

        applicable.sort(key=lambda p: p.confidence * p.success_rate, reverse=True)
        return applicable[:10]

    def _pattern_matches(self, pattern: LearnedPattern, context: dict[str, Any]) -> bool:
        return all(condition in str(context).lower() for condition in pattern.conditions)

    async def apply_pattern(
        self,
        pattern: LearnedPattern,
        context: dict[str, Any],
    ) -> dict[str, Any]:
        pattern.usage_count += 1
        pattern.updated_at = datetime.now(UTC)

        return {
            "pattern_id": pattern.id,
            "suggested_actions": pattern.actions,
            "confidence": pattern.confidence,
        }

    async def record_pattern_outcome(
        self,
        pattern_id: str,
        success: bool,
    ) -> None:
        for pattern in self._global_patterns:
            if pattern.id == pattern_id:
                total = pattern.usage_count
                if total > 0:
                    pattern.success_rate = (pattern.success_rate * (total - 1) + (1 if success else 0)) / total
                break

        for memory in self._skill_memories.values():
            for pattern in memory.learned_patterns:
                if pattern.id == pattern_id:
                    total = pattern.usage_count
                    if total > 0:
                        pattern.success_rate = (pattern.success_rate * (total - 1) + (1 if success else 0)) / total
                    break
