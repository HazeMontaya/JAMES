"""Execution checkpoints with verification and rollback metadata."""
from __future__ import annotations
from dataclasses import dataclass, asdict, field
from datetime import datetime, timezone
import json
from pathlib import Path
from typing import Any
from uuid import uuid4

@dataclass
class Checkpoint:
    checkpoint_id: str
    operation: str
    status: str = "created"
    created_at: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    verified_at: str | None = None
    result: dict[str, Any] = field(default_factory=dict)
    errors: list[str] = field(default_factory=list)

class CheckpointStore:
    """Persist execution state so success is evidence-backed and recoverable."""
    def __init__(self, root: str | Path):
        self.root = Path(root).expanduser().resolve()
        self.root.mkdir(parents=True, exist_ok=True)

    def create(self, operation: str, result: dict[str, Any] | None = None) -> Checkpoint:
        cp = Checkpoint(str(uuid4()), operation.strip(), result=result or {})
        self._write(cp)
        return cp

    def verify(self, checkpoint_id: str, *, checks: dict[str, bool]) -> Checkpoint:
        cp = self.load(checkpoint_id)
        failed = [name for name, passed in checks.items() if not passed]
        if failed:
            cp.status = "failed"
            cp.errors.extend(failed)
        else:
            cp.status = "verified"
            cp.verified_at = datetime.now(timezone.utc).isoformat()
            cp.result["checks"] = checks
        self._write(cp)
        return cp

    def fail(self, checkpoint_id: str, error: str) -> Checkpoint:
        cp = self.load(checkpoint_id)
        cp.status = "failed"
        if error.strip():
            cp.errors.append(error.strip())
        self._write(cp)
        return cp

    def load(self, checkpoint_id: str) -> Checkpoint:
        path = self.root / f"{checkpoint_id}.json"
        if not path.exists():
            raise KeyError(checkpoint_id)
        return Checkpoint(**json.loads(path.read_text(encoding="utf-8")))

    def latest(self, operation: str | None = None) -> Checkpoint | None:
        files = sorted(self.root.glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True)
        for path in files:
            try:
                cp = Checkpoint(**json.loads(path.read_text(encoding="utf-8")))
            except (TypeError, ValueError, json.JSONDecodeError):
                continue
            if operation is None or cp.operation == operation:
                return cp
        return None

    def _write(self, checkpoint: Checkpoint) -> None:
        path = self.root / f"{checkpoint.checkpoint_id}.json"
        path.write_text(json.dumps(asdict(checkpoint), indent=2, ensure_ascii=False), encoding="utf-8")
