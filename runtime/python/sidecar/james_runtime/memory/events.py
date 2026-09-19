"""Append-only runtime event log."""
from __future__ import annotations
import json
from pathlib import Path
from typing import Any, Iterable

from james_runtime.core.events import RuntimeEvent


class EventLog:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path).expanduser()
        self.path.parent.mkdir(parents=True, exist_ok=True)
    def append(self, event_type: str, payload: dict[str, Any] | None = None, *, source: str = "james", correlation_id: str | None = None, causation_id: str | None = None) -> RuntimeEvent:
        if not event_type.strip():
            raise ValueError("event_type must not be empty")
        event = RuntimeEvent(event_type=event_type.strip(), payload=payload or {}, source=source, correlation_id=correlation_id, causation_id=causation_id)
        with self.path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(event.model_dump(mode="json"), ensure_ascii=False, sort_keys=True) + "\n")
            handle.flush()
        return event
    def tail(self, limit: int = 100) -> list[RuntimeEvent]:
        if limit < 1 or not self.path.exists():
            return []
        result = []
        for row in self.path.read_text(encoding="utf-8").splitlines()[-limit:]:
            try:
                result.append(RuntimeEvent.model_validate(json.loads(row)))
            except (TypeError, ValueError, json.JSONDecodeError):
                continue
        return result
    def replay(self) -> Iterable[RuntimeEvent]:
        if not self.path.exists():
            return
        with self.path.open("r", encoding="utf-8") as handle:
            for row in handle:
                try:
                    yield RuntimeEvent(**json.loads(row))
                except (TypeError, ValueError, json.JSONDecodeError):
                    continue
