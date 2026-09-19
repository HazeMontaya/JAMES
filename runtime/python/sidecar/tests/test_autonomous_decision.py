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
