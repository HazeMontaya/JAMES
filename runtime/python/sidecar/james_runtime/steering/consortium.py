"""Multi-model consortium orchestration for synthesis tasks."""
from __future__ import annotations
from dataclasses import dataclass
from typing import Awaitable, Callable, Sequence, Any
import asyncio

@dataclass(frozen=True)
class ConsortiumResult:
    candidates: tuple[Any, ...]
    synthesis: Any | None
    successful: int

async def run_consortium(
    model_ids: Sequence[str],
    generate: Callable[[str], Awaitable[Any]],
    synthesize: Callable[[list[Any]], Awaitable[Any]],
) -> ConsortiumResult:
    """Generate independent candidate answers, then synthesize them."""
    candidates = await asyncio.gather(*(generate(model) for model in model_ids), return_exceptions=True)
    valid = tuple(x for x in candidates if not isinstance(x, Exception))
    synthesis = await synthesize(list(valid)) if valid else None
    return ConsortiumResult(tuple(candidates), synthesis, len(valid))