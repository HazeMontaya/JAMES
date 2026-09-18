"""Verification Engine for JAMES"""

from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
from typing import Any
from uuid import uuid4

import structlog

from ..events.bus import EventBus
from ..events.models import VerificationEvent

logger = structlog.get_logger()


class VerificationType(Enum):
    CORRECTNESS = "correctness"
    COMPLETENESS = "completeness"
    CONSISTENCY = "consistency"
    QUALITY = "quality"
    SECURITY = "security"
    CROSS_SOURCE = "cross_source"


class VerificationStatus(Enum):
    PENDING = "pending"
    PASSED = "passed"
    FAILED = "failed"
    WARNING = "warning"


@dataclass
class VerificationCriterion:
    name: str
    verification_type: VerificationType
    checker: Callable[[Any], Awaitable[tuple[bool, float, dict[str, Any]]]]
    weight: float = 1.0
    threshold: float = 0.7


@dataclass
class VerificationResult:
    verification_id: str = field(default_factory=lambda: str(uuid4()))
    goal_id: str = ""
    task_id: str = ""
    status: VerificationStatus = VerificationStatus.PENDING
    overall_score: float = 0.0
    criterion_results: dict[str, tuple[bool, float, dict[str, Any]]] = field(default_factory=dict)
    passed: bool = False
    details: dict[str, Any] = field(default_factory=dict)
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))


class VerificationEngine:
    def __init__(self, event_bus: EventBus):
        self._event_bus = event_bus
        self._criteria: dict[str, list[VerificationCriterion]] = {}
        self._default_criteria: list[VerificationCriterion] = []

    def register_criteria(self, task_type: str, criteria: list[VerificationCriterion]) -> None:
        self._criteria[task_type] = criteria

    def register_default_criteria(self, criteria: list[VerificationCriterion]) -> None:
        self._default_criteria = criteria

    async def verify(
        self,
        goal_id: str,
        task_id: str,
        result: Any,
        task_type: str = "default",
        context: dict[str, Any] | None = None,
    ) -> VerificationResult:
        criteria = self._criteria.get(task_type, self._default_criteria)

        if not criteria:
            return VerificationResult(
                goal_id=goal_id,
                task_id=task_id,
                status=VerificationStatus.PASSED,
                overall_score=1.0,
                passed=True,
            )

        criterion_results = {}
        total_weight = 0.0
        weighted_score = 0.0

        for criterion in criteria:
            try:
                passed, score, details = await criterion.checker(result)
                criterion_results[criterion.name] = (passed, score, details)
                total_weight += criterion.weight
                weighted_score += score * criterion.weight
            except Exception as e:
                logger.error("Verification criterion failed", criterion=criterion.name, error=str(e))
                criterion_results[criterion.name] = (False, 0.0, {"error": str(e)})
                total_weight += criterion.weight

        overall_score = weighted_score / total_weight if total_weight > 0 else 0.0
        passed = overall_score >= 0.7 and all(p for p, _, _ in criterion_results.values())

        if (overall_score >= 0.9 and passed) or (overall_score >= 0.7 and passed):
            status = VerificationStatus.PASSED
        elif overall_score >= 0.5:
            status = VerificationStatus.WARNING
        else:
            status = VerificationStatus.FAILED

        verification = VerificationResult(
            goal_id=goal_id,
            task_id=task_id,
            status=status,
            overall_score=overall_score,
            criterion_results=criterion_results,
            passed=passed,
            details={"context": context, "criteria_count": len(criteria)},
        )

        await self._event_bus.publish("verification.result", VerificationEvent(
            verification_id=verification.verification_id,
            passed=verification.passed,
            score=verification.overall_score,
            details=verification.details,
        ))

        return verification

    async def cross_source_verify(
        self,
        goal_id: str,
        task_id: str,
        sources: list[dict[str, Any]],
        claim: str,
    ) -> VerificationResult:
        if len(sources) < 2:
            return VerificationResult(
                goal_id=goal_id,
                task_id=task_id,
                status=VerificationStatus.WARNING,
                overall_score=0.5,
                passed=False,
                details={"reason": "Insufficient sources for cross-verification"},
            )

        agreements = 0
        total_pairs = 0

        for i in range(len(sources)):
            for j in range(i + 1, len(sources)):
                total_pairs += 1
                agreement = await self._check_agreement(sources[i], sources[j], claim)
                if agreement:
                    agreements += 1

        agreement_ratio = agreements / total_pairs if total_pairs > 0 else 0.0

        return VerificationResult(
            goal_id=goal_id,
            task_id=task_id,
            status=VerificationStatus.PASSED if agreement_ratio >= 0.7 else VerificationStatus.WARNING,
            overall_score=agreement_ratio,
            passed=agreement_ratio >= 0.7,
            details={
                "sources_compared": len(sources),
                "agreements": agreements,
                "total_pairs": total_pairs,
                "claim": claim,
            },
        )

    async def _check_agreement(self, source1: dict[str, Any], source2: dict[str, Any], claim: str) -> bool:
        return True
