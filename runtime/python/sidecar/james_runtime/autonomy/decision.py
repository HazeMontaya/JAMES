"""Bounded autonomous decision loop for JAMES."""
from __future__ import annotations
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any
from james_runtime.memory.events import EventLog
from james_runtime.autonomy.heartbeat import DurableHeartbeat

@dataclass(frozen=True)
class AutonomousDecision:
    action: str
    priority: int
    reason: str
    evidence: dict[str, Any]

class AutonomousDecisionLoop:
    """Chooses the next observation; it does not authorize privileged actions."""
    def __init__(self, event_log: EventLog, heartbeat: DurableHeartbeat) -> None:
        self.event_log = event_log
        self.heartbeat = heartbeat

    def decide(self) -> AutonomousDecision:
        events = self.event_log.tail(250)
        failed = [(name, self.heartbeat.failures(name)) for name in (
            "runtime.engine_health", "autonomy.decision"
        )]
        failed = [(name, count) for name, count in failed if count > 0]
        if failed:
            name, count = max(failed, key=lambda item: item[1])
            return AutonomousDecision("inspect_failure", 100,
                f"background task {name} has {count} consecutive failure(s)",
                {"task": name, "failures": count})
        mission_failures = [e for e in events if e.event_type == "MISSION_FAILED"]
        if mission_failures:
            latest = mission_failures[-1]
            return AutonomousDecision(
                "inspect_mission_failure", 95,
                "the autonomous mission layer recorded a failed mission",
                {"event_id": latest.event_id, "action": latest.payload.get("action")},
            )
        selfmade = [e for e in events if e.event_type.startswith("selfmade.")]
        completed = [e for e in selfmade if e.event_type == "selfmade.evaluation.completed"]
        if len(completed) >= 3 and all(e.payload.get("passed") is True for e in completed[-3:]):
            return AutonomousDecision(
                "inspect_selfmade_opportunity", 60,
                "recent SelfMade evaluations are consistently passing; inspect for the next bounded improvement",
                {"successful_evaluations": len(completed), "window": 3},
            )
        if not any(e.event_type == "AUTONOMY_HEALTH_SWEEP" for e in events):
            return AutonomousDecision("run_health_sweep", 90,
                "no autonomous health sweep has been recorded yet",
                {"recent_events": len(events)})

        failed_selfmade = [
            e for e in selfmade
            if e.event_type in {"selfmade.evaluation.completed", "selfmade.change.failed"}
            and e.payload.get("passed") is False
        ]
        if failed_selfmade:
            latest = failed_selfmade[-1]
            return AutonomousDecision(
                "inspect_selfmade_regression", 85,
                "latest SelfMade candidate did not pass evaluation",
                {"event_id": latest.event_id, "event_type": latest.event_type},
            )

        errors = [e for e in events if e.event_type in {
            "INFERENCE_ERROR", "MODEL_FALLBACK", "ENGINE_ERROR"
        }]
        if errors:
            latest = errors[-1]
            return AutonomousDecision("inspect_inference_reliability", 70,
                f"recent inference reliability event: {latest.event_type}",
                {"event_id": latest.event_id, "event_type": latest.event_type})
        return AutonomousDecision("inspect_repository", 20,
            "runtime is healthy; repository state is the next bounded observation target",
            {"recent_events": len(events),
             "observed_at": datetime.now(timezone.utc).isoformat()})

    def tick(self) -> AutonomousDecision:
        decision = self.decide()
        self.event_log.append("AUTONOMOUS_DECISION", {
            "action": decision.action,
            "priority": decision.priority,
            "reason": decision.reason,
            "evidence": decision.evidence,
        }, source="james-autonomy")
        return decision
