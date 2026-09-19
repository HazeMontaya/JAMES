import asyncio

from james_runtime.autonomy.heartbeat import DurableHeartbeat, HeartbeatTask
from james_runtime.autonomy.survival import allowed_tasks, budget_for

async def test_heartbeat_runs_due_task_once():
    calls = []
    async def work():
        calls.append("ok")
    hb = DurableHeartbeat()
    hb.register(HeartbeatTask("health", 1, work))
    result = await hb.tick()
    assert result[0].success
    assert calls == ["ok"]
    assert await hb.tick() == []

async def test_heartbeat_timeout_is_recorded():
    async def slow():
        await asyncio.sleep(0.05)
    hb = DurableHeartbeat()
    hb.register(HeartbeatTask("slow", 1, slow, timeout_seconds=0.001))
    result = await hb.tick()
    assert not result[0].success
    assert hb.failures("slow") == 1


async def test_heartbeat_state_survives_restart(tmp_path):
    calls = []
    async def work():
        calls.append("ok")
    state = tmp_path / "heartbeat.json"
    hb = DurableHeartbeat(state)
    hb.register(HeartbeatTask("health", 60, work))
    await hb.tick()
    restored = DurableHeartbeat(state)
    await restored.restore()
    restored.register(HeartbeatTask("health", 60, work))
    assert restored.due(now=restored._last_run["health"] + 1) == []
    assert calls == ["ok"]

def test_survival_budget_degrades_background_work():
    assert budget_for("critical").max_parallel_tasks == 0
    assert allowed_tasks("low_compute", 4) == 1
    assert allowed_tasks("normal", 4) == 2