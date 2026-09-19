"""Deterministic output-shaping modules.

These are presentation transforms, not model steering or safety bypasses.
"""
from __future__ import annotations
import re

def _hedge_reducer(text: str) -> str:
    patterns = [
        r"\b(it is worth noting that)\s+",
        r"\b(as an ai language model),?\s*",
        r"\b(i would like to point out that)\s+",
        r"\b(in my opinion),?\s*",
        r"\b(please note that)\s+",
    ]
    out = text
    for p in patterns:
        out = re.sub(p, "", out, flags=re.I)
    return out

def _direct_mode(text: str) -> str:
    out = re.sub(r"\n{3,}", "\n\n", text.strip())
    out = re.sub(r"(?m)^\s*[-*]\s*$", "", out)
    return out.strip()

MODULES = {
    "hedge_reducer": _hedge_reducer,
    "direct_mode": _direct_mode,
}

def transform_text(text: str, modules: list[str] | None = None) -> tuple[str, list[str]]:
    result = text
    applied = []
    for name in modules or []:
        fn = MODULES.get(name)
        if fn is not None:
            result = fn(result)
            applied.append(name)
    return result, applied
