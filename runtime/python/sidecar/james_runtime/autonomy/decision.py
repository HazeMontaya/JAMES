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
        events = self.event_log.tail(100)
        failed = [(name, self.heartbeat.failures(name)) for name in (
            "runtime.engine_health", "autonomy.decision"
        )]
        failed = [(name, count) for name, count in failed if count > 0]
        if failed:
            name, count = max(failed, key=lambda item: item[1])
            return AutonomousDecision("inspect_failure", 100,
                f"background task {name} has {count} consecutive failure(s)",
                {"task": name, "failures": count})
        if not any(e.event_type == "AUTONOMY_HEALTH_SWEEP" for e in events):
            return AutonomousDecision("run_health_sweep", 90,
                "no autonomous health sweep has been recorded yet",
                {"recent_events": len(events)})
        errors = [e for e in events if e.event_type in {
            "INFERENCE_ERROR", "MODEL_FALLBACK", "ENGINE_ERROR"
        }]
        if errors:
            latest = errors[-1]
            return AutonomousDecision("inspect_inference_reliability", 70,
                f"recent inference reliability event: {latest.event_type}",
                {"event_id": latest.event_id, "event_type": latest.event_type})
        return AutonomousDecision("observe", 10,
            "runtime is healthy and no higher-priority observation is pending",
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
