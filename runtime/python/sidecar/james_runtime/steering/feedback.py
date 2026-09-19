"""Persistent EMA preference learning for sampling parameters."""
from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
import json

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
        ema = entry.setdefault("ema", {})

        for name in set(observed) | set(target):
            obs = float(observed.get(name, target.get(name, 0.0)))
            tgt = float(target.get(name, obs))

            if name not in ema:
                # Positive feedback starts from the actual observed behavior.
                # Negative feedback starts from the supplied target.
                learned = obs if rating > 0 else tgt
            elif rating > 0:
                learned = float(ema[name]) + self.alpha * (obs - float(ema[name]))
            else:
                learned = float(ema[name]) + self.alpha * (tgt - float(ema[name]))

            ema[name] = learned

        entry["count"] = int(entry.get("count", 0)) + 1
        self._save()
        return {k: float(v) for k, v in ema.items()}

    def get(self, context: str) -> dict[str, float]:
        return {k: float(v) for k, v in self._state.get(context, {}).get("ema", {}).items()}

    def stats(self) -> dict:
        return {
            k: {"count": v.get("count", 0), "ema": dict(v.get("ema", {}))}
            for k, v in self._state.items()
        }
