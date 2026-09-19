"""Liquid multi-model response orchestration for JAMES.

Implements the safe architectural portion of G0DM0D3 ULTRAPLINIAN:
parallel candidate generation, incremental leader upgrades, final winner selection,
and content-free race metadata. Provider/model invocation stays injected so this
layer can reuse JAMES RuntimeRouter and existing engines.
"""
from __future__ import annotations

from dataclasses import dataclass
import asyncio
import time
from typing import Awaitable, Callable, Sequence, Any

from .race import RaceResult, score_response

@dataclass(frozen=True)
class LiquidUpdate:
    kind: str
    model: str
    score: float
    delta: float
    duration_ms: int
    response: Any | None

@dataclass(frozen=True)
class LiquidRaceResult:
    winner: RaceResult | None
    results: tuple[RaceResult, ...]
    updates: tuple[LiquidUpdate, ...]

async def liquid_race(
    model_ids: Sequence[str],
    generate: Callable[[str], Awaitable[Any]],
    *,
    min_delta: float = 8.0,
    on_update: Callable[[LiquidUpdate], None] | None = None,
) -> LiquidRaceResult:
    """Run candidates concurrently and emit only material leader upgrades."""
    current: RaceResult | None = None
    updates: list[LiquidUpdate] = []
    lock = asyncio.Lock()

    async def one(model: str) -> RaceResult:
        nonlocal current
        started = time.perf_counter()
        try:
            response = await generate(model)
            text = ""
            if hasattr(response, "choices") and response.choices:
                msg = response.choices[0].get("message", {})
                text = msg.get("content", "") if isinstance(msg, dict) else ""
            elif isinstance(response, str):
                text = response
            result = RaceResult(model, response, score_response(text), int((time.perf_counter()-started)*1000), True)
        except Exception as exc:
            result = RaceResult(model, None, 0.0, int((time.perf_counter()-started)*1000), False, str(exc))
        if result.success and result.score > 0:
            async with lock:
                previous = current.score if current else 0.0
                material = current is None or result.score >= previous + min_delta
                if material:
                    current = result
                    update = LiquidUpdate("leader" if previous == 0 else "upgrade", result.model, result.score, result.score-previous, result.duration_ms, result.response)
                    updates.append(update)
                    if on_update:
                        on_update(update)
        return result

    results = tuple(await asyncio.gather(*(one(model) for model in model_ids)))
    winner = max((r for r in results if r.success), key=lambda r: r.score, default=None)
    return LiquidRaceResult(winner, tuple(sorted(results, key=lambda r: (-r.score, r.duration_ms, r.model))), tuple(updates))