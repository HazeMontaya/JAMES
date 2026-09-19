"""Provider-neutral parallel model racing and transparent response scoring."""
from __future__ import annotations
from dataclasses import dataclass
import asyncio
import time
import re
from typing import Any, Awaitable, Callable

@dataclass(frozen=True)
class RaceResult:
    model: str
    response: Any | None
    score: float
    duration_ms: int
    success: bool
    error: str | None = None

def response_text(response: Any) -> str:
    """Extract assistant text from common OpenAI-compatible response shapes."""
    if isinstance(response, str):
        return response
    choices = response.get("choices") if isinstance(response, dict) else getattr(response, "choices", None)
    if not choices:
        return ""
    first = choices[0]
    message = first.get("message", {}) if isinstance(first, dict) else getattr(first, "message", {})
    content = message.get("content", "") if isinstance(message, dict) else getattr(message, "content", "")
    if isinstance(content, list):
        parts=[]
        for item in content:
            if isinstance(item, dict):
                text=item.get("text")
                if text:
                    parts.append(str(text))
            else:
                text=getattr(item,"text",None)
                if text:
                    parts.append(str(text))
        return "".join(parts)
    return content if isinstance(content, str) else ("" if content is None else str(content))

def score_response(text: str) -> float:
    """Deterministic heuristic favoring usable substance, structure and lexical diversity."""
    if not text.strip():
        return 0.0
    words = re.findall(r"\b\w+\b", text)
    if not words:
        return 0.0
    sentences = max(1, len(re.findall(r"[.!?]+", text)))
    unique = len(set(w.lower() for w in words))
    structure = min(25.0, sentences * 2.5 + (10.0 if "\n" in text else 0.0))
    substance = min(55.0, len(words) * 1.5)
    diversity = min(20.0, unique / len(words) * 20.0)
    return round(min(100.0, structure + substance + diversity), 2)

async def race_models(model_ids: list[str], generate: Callable[[str], Awaitable[Any]]) -> list[RaceResult]:
    """Run independent model calls concurrently and return ranked results."""
    async def one(model: str) -> RaceResult:
        started = time.perf_counter()
        try:
            response = await generate(model)
            return RaceResult(model, response, score_response(response_text(response)),
                              int((time.perf_counter() - started) * 1000), True)
        except Exception as exc:
            return RaceResult(model, None, 0.0,
                              int((time.perf_counter() - started) * 1000), False, str(exc))
    results = await asyncio.gather(*(one(m) for m in model_ids))
    return sorted(results, key=lambda x: (-x.score, x.duration_ms, x.model))

def best_result(results: list[RaceResult]) -> RaceResult | None:
    return results[0] if results else None
