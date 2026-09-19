"""Context-adaptive sampling without model-specific APIs.

The implementation is deliberately deterministic and transparent: context scores,
selected profile and confidence are all inspectable and can be persisted as events.
"""
from __future__ import annotations
from dataclasses import dataclass
import re

@dataclass(frozen=True)
class AutoTuneProfile:
    temperature: float
    top_p: float
    frequency_penalty: float
    presence_penalty: float
    max_tokens_multiplier: float = 1.0

PROFILES = {
    "code": AutoTuneProfile(0.20, 0.90, 0.10, 0.00, 1.0),
    "analytical": AutoTuneProfile(0.35, 0.92, 0.05, 0.00, 1.15),
    "creative": AutoTuneProfile(0.95, 0.97, 0.00, 0.15, 1.10),
    "conversational": AutoTuneProfile(0.65, 0.95, 0.00, 0.05, 1.0),
    "chaotic": AutoTuneProfile(1.10, 1.00, -0.05, 0.20, 1.0),
}

PATTERNS = {
    "code": r"\b(code|coding|python|rust|typescript|javascript|function|class|debug|compile|stack trace|api|sql|regex|implement|repository|repo)\b",
    "analytical": r"\b(analy[sz]e|analysis|compare|investigate|research|evaluate|evidence|trade[- ]?off|architecture|benchmark|why|cause)\b",
    "creative": r"\b(write|story|poem|creative|fiction|brainstorm|design|caption|script|imagine|novel)\b",
    "conversational": r"\b(hello|hi|thanks|thank you|explain|what is|how are you|help me)\b",
    "chaotic": r"\b(random|surprise me|wild|absurd|unusual|experimental|chaotic)\b",
}

@dataclass(frozen=True)
class AutoTuneResult:
    context: str
    confidence: float
    scores: dict[str, float]
    profile: AutoTuneProfile
    reasoning: str

def compute_autotune(text: str, *, history: list[str] | None = None) -> AutoTuneResult:
    corpus = " ".join([text, *(history or [])]).lower()
    scores = {k: float(len(re.findall(p, corpus))) for k, p in PATTERNS.items()}
    if not any(scores.values()):
        context = "conversational"
        confidence = 0.20
    else:
        ordered = sorted(scores.items(), key=lambda x: (-x[1], x[0]))
        context, best = ordered[0]
        total = sum(scores.values())
        confidence = min(1.0, best / max(1.0, total))
        if len(ordered) > 1 and ordered[1][1] == best:
            confidence *= 0.75
    profile = PROFILES[context]
    reasoning = f"context={context}; confidence={confidence:.2f}; scores={scores}"
    return AutoTuneResult(context, round(confidence, 4), scores, profile, reasoning)
