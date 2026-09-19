"""Persistent EMA preference learning for sampling parameters."""
from __future__ import annotations
from dataclasses import dataclass, asdict
from pathlib import Path
import json
import math

@dataclass
class FeedbackSample:
    context: str
    rating: int
    observed: dict[str, float]
    target: dict[str, float]

class FeedbackStore:
    def __init__(self, path: str | Path, *, alpha: float = 0.30) -> None:
        if not 0 < alpha <= 1:
            raise ValueError("alpha must be in (0, 1]")
        self.path = Path(path).expanduser().resolve()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.alpha = alpha
        self._state = self._load()

    def _load(self) -> dict:
        if not self.path.exists():
            return {}
        try:
            return json.loads(self.path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {}

    def _save(self) -> None:
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        tmp.write_text(json.dumps(self._state, indent=2, ensure_ascii=False), encoding="utf-8")
        tmp.replace(self.path)

    def update(self, context: str, rating: int, observed: dict[str, float], target: dict[str, float]) -> dict[str, float]:
        if rating not in (-1, 1):
            raise ValueError("rating must be -1 or 1")
        key = context or "unknown"
        entry = self._state.setdefault(key, {"count": 0, "ema": {}})
        # Positive feedback moves toward observed values; negative feedback moves away
        # from the observed vector and toward the configured target.
        direction = 1.0 if rating > 0 else -1.0
        ema = entry.setdefault("ema", {})
        keys = set(observed) | set(target)
        for name in keys:
            current = float(ema.get(name, target.get(name, observed.get(name, 0.0))))
            obs = float(observed.get(name, current))
            tgt = float(target.get(name, obs))
            desired = obs if direction > 0 else (2.0 * tgt - obs)
            ema[name] = current + self.alpha * (desired - current)
        entry["count"] = int(entry.get("count", 0)) + 1
        self._save()
        return {k: float(v) for k, v in ema.items()}

    def get(self, context: str) -> dict[str, float]:
        return {k: float(v) for k, v in self._state.get(context, {}).get("ema", {}).items()}

    def stats(self) -> dict:
        return {k: {"count": v.get("count", 0), "ema": dict(v.get("ema", {}))}
                for k, v in self._state.items()}
