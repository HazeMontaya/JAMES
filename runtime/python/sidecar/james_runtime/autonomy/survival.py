from __future__ import annotations
from dataclasses import dataclass
from typing import Iterable

@dataclass(frozen=True)
class HealthBudget:
    """Compute-aware execution budget for autonomous background work."""
    tier: str
    max_parallel_tasks: int
    allow_background_reasoning: bool

TIER_ORDER = {"critical": 0, "low_compute": 1, "normal": 2, "high": 3}

def budget_for(tier: str) -> HealthBudget:
    level = TIER_ORDER.get(tier, 0)
    if level <= 0:
        return HealthBudget(tier, 0, False)
    if level == 1:
        return HealthBudget(tier, 1, False)
    if level == 2:
        return HealthBudget(tier, 2, True)
    return HealthBudget(tier, 4, True)

def allowed_tasks(tier: str, requested: int) -> int:
    return min(max(0, requested), budget_for(tier).max_parallel_tasks)