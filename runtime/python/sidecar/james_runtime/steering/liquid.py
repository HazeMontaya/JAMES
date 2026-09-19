"""Liquid multi-model response orchestration for JAMES."""
from __future__ import annotations
from dataclasses import dataclass
import asyncio
import time
from typing import Awaitable, Callable, Sequence, Any
from .race import RaceResult, response_text, score_response


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


async def liquid_race(model_ids: Sequence[str], generate: Callable[[str], Awaitable[Any]], *,
                      min_delta: float = 8.0,
                      on_update: Callable[[LiquidUpdate], None] | None = None) -> LiquidRaceResult:
    """Run candidates concurrently and emit only material leader upgrades."""
    current: RaceResult | None = None
    updates: list[LiquidUpdate] = []
    lock = asyncio.Lock()

    async def one(model: str) -> RaceResult:
        nonlocal current
        started = time.perf_counter()
        try:
            response = await generate(model)
            result = RaceResult(model, response, score_response(response_text(response)),
                                int((time.perf_counter() - started) * 1000), True)
        except Exception as exc:
            result = RaceResult(model, None, 0.0,
                                int((time.perf_counter() - started) * 1000), False, str(exc))
        if result.success and result.score > 0:
            async with lock:
                previous = current.score if current else 0.0
                if current is None or result.score >= previous + min_delta:
                    kind = "leader" if current is None else "upgrade"
                    current = result
                    update = LiquidUpdate(kind, result.model, result.score, result.score - previous,
                                          result.duration_ms, result.response)
                    updates.append(update)
                    if on_update:
                        on_update(update)
        return result

    results = tuple(await asyncio.gather(*(one(model) for model in model_ids)))
    winner = max((r for r in results if r.success), key=lambda r: r.score, default=None)
    return LiquidRaceResult(
        winner,
        tuple(sorted(results, key=lambda r: (-r.score, r.duration_ms, r.model))),
        tuple(updates),
    )
