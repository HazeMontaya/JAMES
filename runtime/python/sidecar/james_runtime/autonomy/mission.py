"""Durable mission layer for bounded autonomous actions.

Missions turn autonomous decisions into explicit, replayable units of work.
The mission layer never grants authorization; handlers are registered by the
runtime and only explicitly registered actions can execute.
"""
from __future__ import annotations

from dataclasses import asdict, dataclass, field
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
from typing import Any, Awaitable, Callable
from uuid import uuid4

from james_runtime.autonomy.decision import AutonomousDecision
from james_runtime.memory.events import EventLog

MissionHandler = Callable[["AutonomousMission"], Awaitable[dict[str, Any]]]


@dataclass
class AutonomousMission:
    mission_id: str
    action: str
    priority: int
    reason: str
    evidence: dict[str, Any] = field(default_factory=dict)
    status: str = "created"
    created_at: str = field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    started_at: str | None = None
    completed_at: str | None = None
    result: dict[str, Any] = field(default_factory=dict)
    error: str | None = None
    correlation_id: str | None = None
    causation_id: str | None = None
    attempts: int = 0

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


class AutonomousMissionManager:
    """Persist, deduplicate, and execute only explicitly registered missions."""

    def __init__(
        self,
        event_log: EventLog,
        state_path: str | Path,
        *,
        max_active: int = 4,
        cooldown_seconds: float = 300.0,
    ) -> None:
        if max_active < 1:
            raise ValueError("max_active must be positive")
        self.event_log = event_log
        self.state_path = Path(state_path).expanduser()
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        self.max_active = max_active
        self.cooldown = timedelta(seconds=max(0.0, cooldown_seconds))
        self.missions: dict[str, AutonomousMission] = {}
        self.handlers: dict[str, MissionHandler] = {}

    async def restore(self) -> None:
        if not self.state_path.exists():
            return
        try:
            raw = json.loads(self.state_path.read_text(encoding="utf-8"))
            for item in raw.get("missions", []):
                mission = AutonomousMission(**item)
                self.missions[mission.mission_id] = mission
        except (OSError, TypeError, ValueError, json.JSONDecodeError):
            self.missions = {}

    def register(self, action: str, handler: MissionHandler) -> None:
        if not action.strip():
            raise ValueError("action must not be empty")
        self.handlers[action.strip()] = handler

    def active(self) -> list[AutonomousMission]:
        return [
            mission for mission in self.missions.values()
            if mission.status in {"created", "running"}
        ]

    def _persist(self) -> None:
        payload = {
            "version": 1,
            "missions": [m.to_dict() for m in self.missions.values()],
        }
        tmp = self.state_path.with_suffix(self.state_path.suffix + ".tmp")
        tmp.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
        tmp.replace(self.state_path)

    def _recent_duplicate(self, action: str) -> AutonomousMission | None:
        now = datetime.now(timezone.utc)
        for mission in reversed(list(self.missions.values())):
            if mission.action != action:
                continue
            if mission.status in {"created", "running"}:
                return mission
            if mission.completed_at:
                try:
                    completed = datetime.fromisoformat(mission.completed_at)
                    if now - completed <= self.cooldown:
                        return mission
                except ValueError:
                    continue
        return None

    def create(
        self,
        decision: AutonomousDecision,
        *,
        correlation_id: str | None = None,
        causation_id: str | None = None,
    ) -> AutonomousMission | None:
        duplicate = self._recent_duplicate(decision.action)
        if duplicate is not None:
            self.event_log.append(
                "MISSION_DEDUPLICATED",
                {"mission_id": duplicate.mission_id, "action": decision.action},
                source="james-autonomy",
                correlation_id=correlation_id,
            )
            return duplicate
        if len(self.active()) >= self.max_active:
            self.event_log.append(
                "MISSION_REJECTED",
                {"action": decision.action, "reason": "active mission limit reached"},
                source="james-autonomy",
                correlation_id=correlation_id,
            )
            return None

        mission = AutonomousMission(
            mission_id=f"mission-{uuid4()}",
            action=decision.action,
            priority=decision.priority,
            reason=decision.reason,
            evidence=dict(decision.evidence),
            correlation_id=correlation_id,
            causation_id=causation_id,
        )
        self.missions[mission.mission_id] = mission
        self._persist()
        self.event_log.append(
            "MISSION_CREATED",
            {
                "mission_id": mission.mission_id,
                "action": mission.action,
                "priority": mission.priority,
                "reason": mission.reason,
                "evidence": mission.evidence,
            },
            source="james-autonomy",
            correlation_id=mission.correlation_id,
        )
        return mission

    async def execute(self, mission: AutonomousMission) -> AutonomousMission:
        if mission.status not in {"created", "failed"}:
            return mission
        handler = self.handlers.get(mission.action)
        if handler is None:
            mission.status = "failed"
            mission.error = f"no handler registered for action {mission.action}"
            mission.completed_at = datetime.now(timezone.utc).isoformat()
            self._persist()
            self.event_log.append(
                "MISSION_FAILED",
                {"mission_id": mission.mission_id, "action": mission.action, "error": mission.error},
                source="james-autonomy",
                correlation_id=mission.correlation_id,
                causation_id=mission.causation_id,
            )
            return mission

        mission.status = "running"
        mission.started_at = datetime.now(timezone.utc).isoformat()
        mission.attempts += 1
        mission.error = None
        self._persist()
        self.event_log.append(
            "MISSION_STARTED",
            {"mission_id": mission.mission_id, "action": mission.action, "attempt": mission.attempts},
            source="james-autonomy",
            correlation_id=mission.correlation_id,
        )
        try:
            mission.result = await handler(mission)
            mission.status = "completed"
            mission.completed_at = datetime.now(timezone.utc).isoformat()
            self._persist()
            self.event_log.append(
                "MISSION_COMPLETED",
                {"mission_id": mission.mission_id, "action": mission.action, "result": mission.result},
                source="james-autonomy",
                correlation_id=mission.correlation_id,
            )
        except Exception as exc:
            mission.status = "failed"
            mission.error = str(exc)
            mission.completed_at = datetime.now(timezone.utc).isoformat()
            self._persist()
            self.event_log.append(
                "MISSION_FAILED",
                {"mission_id": mission.mission_id, "action": mission.action, "error": mission.error},
                source="james-autonomy",
                correlation_id=mission.correlation_id,
            )
        return mission

    async def dispatch(
        self,
        decision: AutonomousDecision,
        *,
        correlation_id: str | None = None,
        causation_id: str | None = None,
    ) -> AutonomousMission | None:
        mission = self.create(
            decision,
            correlation_id=correlation_id,
            causation_id=causation_id,
        )
        if mission is None or mission.status == "completed":
            return mission
        return await self.execute(mission)
