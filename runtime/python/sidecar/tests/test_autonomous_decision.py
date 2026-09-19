from james_runtime.autonomy.decision import AutonomousDecisionLoop
from james_runtime.autonomy.heartbeat import DurableHeartbeat
from james_runtime.memory.events import EventLog

def test_decision_loop_observes_empty_runtime(tmp_path):
    log = EventLog(tmp_path / "events.jsonl")
    heartbeat = DurableHeartbeat(tmp_path / "heartbeat.json")
    decision = AutonomousDecisionLoop(log, heartbeat).tick()
    assert decision.action == "run_health_sweep"
    assert decision.priority == 90

def test_decision_loop_prioritizes_failures(tmp_path):
    log = EventLog(tmp_path / "events.jsonl")
    heartbeat = DurableHeartbeat(tmp_path / "heartbeat.json")
    heartbeat._failures["runtime.engine_health"] = 3
    decision = AutonomousDecisionLoop(log, heartbeat).tick()
    assert decision.action == "inspect_failure"
    assert decision.priority == 100

def test_decision_loop_prioritizes_selfmade_regression(tmp_path):
    log = EventLog(tmp_path / "events.jsonl")
    heartbeat = DurableHeartbeat(tmp_path / "heartbeat.json")
    log.append("selfmade.evaluation.completed", {"passed": False})
    decision = AutonomousDecisionLoop(log, heartbeat).tick()
    assert decision.action == "inspect_selfmade_regression"
    assert decision.priority == 85

def test_decision_loop_detects_sustained_selfmade_success(tmp_path):
    log = EventLog(tmp_path / "events.jsonl")
    heartbeat = DurableHeartbeat(tmp_path / "heartbeat.json")
    for _ in range(3):
        log.append("selfmade.evaluation.completed", {"passed": True})
    decision = AutonomousDecisionLoop(log, heartbeat).tick()
    assert decision.action == "inspect_selfmade_opportunity"
    assert decision.priority == 60


def test_mission_layer_creates_and_deduplicates(tmp_path):
    import asyncio
    from james_runtime.autonomy.mission import AutonomousMissionManager
    from james_runtime.autonomy.decision import AutonomousDecision
    log = EventLog(tmp_path / "events.jsonl")
    manager = AutonomousMissionManager(log, tmp_path / "missions.json", cooldown_seconds=300)
    async def handler(mission):
        return {"ok": True}
    manager.register("run_health_sweep", handler)
    decision = AutonomousDecision("run_health_sweep", 90, "test", {})
    async def run():
        first = await manager.dispatch(decision)
        second = await manager.dispatch(decision)
        return first, second
    first, second = asyncio.run(run())
    assert first is not None and second is not None
    assert first.mission_id == second.mission_id
    assert first.status == "completed"
    assert [e.event_type for e in log.tail(10)].count("MISSION_DEDUPLICATED") == 1


def test_mission_layer_persists_and_restores(tmp_path):
    import asyncio
    from james_runtime.autonomy.mission import AutonomousMissionManager
    from james_runtime.autonomy.decision import AutonomousDecision
    log = EventLog(tmp_path / "events.jsonl")
    state = tmp_path / "missions.json"
    manager = AutonomousMissionManager(log, state)
    async def handler(mission):
        return {"value": 42}
    manager.register("inspect_repository", handler)
    mission = asyncio.run(manager.dispatch(AutonomousDecision("inspect_repository", 20, "test", {})))
    restored = AutonomousMissionManager(log, state)
    asyncio.run(restored.restore())
    assert mission is not None
    assert restored.missions[mission.mission_id].result["value"] == 42


def test_mission_layer_rejects_unregistered_action(tmp_path):
    import asyncio
    from james_runtime.autonomy.mission import AutonomousMissionManager
    from james_runtime.autonomy.decision import AutonomousDecision
    log = EventLog(tmp_path / "events.jsonl")
    manager = AutonomousMissionManager(log, tmp_path / "missions.json")
    mission = asyncio.run(manager.dispatch(AutonomousDecision("privileged_unknown", 100, "test", {})))
    assert mission is not None
    assert mission.status == "failed"
    assert "no handler registered" in (mission.error or "")
    assert any(e.event_type == "MISSION_FAILED" for e in log.tail(10))
