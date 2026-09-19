"""Durable autonomous heartbeat for JAMES.

Inspired by Automaton's DB-backed heartbeat/lease design, but deliberately
provider-neutral and without autonomous financial or replication actions.
"""
from __future__ import annotations
from dataclasses import dataclass
import asyncio
import time
import json
from pathlib import Path
from typing import Awaitable, Callable

@dataclass(frozen=True)
class HeartbeatTask:
    name: str
    interval_seconds: float
    action: Callable[[], Awaitable[None]]
    timeout_seconds: float = 30.0
    enabled: bool = True

@dataclass(frozen=True)
class HeartbeatResult:
    name: str
    success: bool
    duration_ms: int
    error: str | None = None

class DurableHeartbeat:
    """Persistent scheduler with overlap protection and timeout."""
    def __init__(self, state_path: str | Path | None = None) -> None:
        self._tasks: dict[str, HeartbeatTask] = {}
        self._state_path = Path(state_path).expanduser() if state_path else None
        self._last_run: dict[str, float] = {}
        self._failures: dict[str, int] = {}
        self._running = False
        self._tick_lock = asyncio.Lock()

    async def restore(self) -> None:
        if not self._state_path or not self._state_path.exists():
            return
        try:
            data = json.loads(self._state_path.read_text(encoding="utf-8"))
            self._last_run = {str(k): float(v) for k, v in data.get("last_run", {}).items()}
            self._failures = {str(k): int(v) for k, v in data.get("failures", {}).items()}
        except (OSError, ValueError, TypeError, json.JSONDecodeError):
            self._last_run = {}
            self._failures = {}

    def _persist(self) -> None:
        if not self._state_path:
            return
        self._state_path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self._state_path.with_suffix(self._state_path.suffix + ".tmp")
        tmp.write_text(json.dumps({"last_run": self._last_run, "failures": self._failures}, sort_keys=True), encoding="utf-8")
        tmp.replace(self._state_path)

    def register(self, task: HeartbeatTask) -> None:
        if task.interval_seconds <= 0:
            raise ValueError("interval_seconds must be positive")
        self._tasks[task.name] = task

    def due(self, now: float | None = None) -> list[HeartbeatTask]:
        now = time.monotonic() if now is None else now
        return [t for t in self._tasks.values() if t.enabled and now - self._last_run.get(t.name, 0.0) >= t.interval_seconds]

    async def tick(self) -> list[HeartbeatResult]:
        if self._tick_lock.locked():
            return []
        async with self._tick_lock:
            results: list[HeartbeatResult] = []
            for task in self.due():
                started = time.monotonic()
                try:
                    await asyncio.wait_for(task.action(), timeout=task.timeout_seconds)
                    self._failures[task.name] = 0
                    results.append(HeartbeatResult(task.name, True, int((time.monotonic()-started)*1000)))
                except Exception as exc:
                    self._failures[task.name] = self._failures.get(task.name, 0) + 1
                    results.append(HeartbeatResult(task.name, False, int((time.monotonic()-started)*1000), str(exc)))
                finally:
                    self._last_run[task.name] = time.monotonic()
            self._persist()
            return results

    async def run(self, poll_seconds: float = 1.0) -> None:
        self._running = True
        while self._running:
            await self.tick()
            await asyncio.sleep(poll_seconds)

    def stop(self) -> None:
        self._running = False

    def failures(self, task_name: str) -> int:
        return self._failures.get(task_name, 0)