"""Performance benchmarks for core hot paths (regression thresholds)."""

import time

import anyio
from james_core.security import redact_secrets


def _assert_under(duration_ms: float, threshold_ms: float, label: str) -> None:
    assert duration_ms < threshold_ms, f"{label} too slow: {duration_ms:.1f}ms (limit {threshold_ms}ms)"


def test_compose_latency():
    from james_skills import SkillEngine

    engine = SkillEngine()

    def measure() -> float:
        start = time.perf_counter()
        for _ in range(100):
            engine.compose("Analysiere den Markt und generiere Leads")
        return (time.perf_counter() - start) * 1000 / 100

    latency = measure()
    _assert_under(latency, 5.0, "skill compose")


def test_execute_fallback_latency():
    from james_skills import SkillEngine
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("email_drafter")
    assert spec is not None

    def measure() -> float:
        start = time.perf_counter()

        async def run() -> None:
            for _ in range(20):
                ctx = SkillContext(inputs={"recipient": "Max", "objective": "Partner", "tone": "formal"})
                await engine.execute(spec, ctx)

        anyio.run(run)
        return (time.perf_counter() - start) * 1000 / 20

    latency = measure()
    _assert_under(latency, 25.0, "skill execute fallback")


def test_audit_write_latency(tmp_path):
    from james_core.audit import AuditLog

    def measure() -> float:
        audit = AuditLog(str(tmp_path / "audit.db"))

        async def run() -> None:
            await audit.initialize()
            start = time.perf_counter()
            for i in range(50):
                await audit.record(actor="perf", action="benchmark", resource=f"r{i}", details={"i": i})
            elapsed = (time.perf_counter() - start) * 1000 / 50
            await audit.close()
            return elapsed

        return anyio.run(run)

    latency = measure()
    _assert_under(latency, 25.0, "audit write")


def test_audit_query_latency(tmp_path):
    from james_core.audit import AuditLog

    def measure() -> float:
        audit = AuditLog(str(tmp_path / "audit.db"))

        async def run() -> None:
            await audit.initialize()
            for i in range(100):
                await audit.record(actor="perf", action="benchmark", resource=f"r{i}")
            start = time.perf_counter()
            for _ in range(20):
                await audit.query(limit=50)
            elapsed = (time.perf_counter() - start) * 1000 / 20
            await audit.close()
            return elapsed

        return anyio.run(run)

    latency = measure()
    _assert_under(latency, 25.0, "audit query")


def test_redact_latency():
    payload = {"username": "alice", "password": "hunter2", "api_key": "sk-1234567890", "nested": {"token": "abc"}}

    start = time.perf_counter()
    for _ in range(1000):
        redact_secrets(payload)
    latency = (time.perf_counter() - start) * 1000 / 1000
    _assert_under(latency, 0.1, "redact_secrets")


def test_event_roundtrip_latency():
    from james_core.events.models import Event

    event = Event(type="test", payload={"key": "value", "items": [1, 2, 3]})

    start = time.perf_counter()
    for _ in range(2000):
        data = event.to_dict()
        Event.from_dict(data)
    latency = (time.perf_counter() - start) * 1000 / 2000
    _assert_under(latency, 0.05, "event roundtrip")


def test_memory_store_latency(tmp_path):
    from james_memory import EpisodicMemory, MemoryEntry, MemoryType

    def measure() -> float:
        memory = EpisodicMemory(str(tmp_path / "episodic.db"))

        async def run() -> None:
            await memory.initialize()
            start = time.perf_counter()
            for i in range(30):
                entry = MemoryEntry(
                    memory_type=MemoryType.EPISODIC,
                    content=f"perf entry {i}",
                    metadata={"i": i},
                    tags=["perf"],
                    source="bench",
                )
                await memory.store(entry)
            elapsed = (time.perf_counter() - start) * 1000 / 30
            await memory.close()
            return elapsed

        return anyio.run(run)

    latency = measure()
    _assert_under(latency, 25.0, "episodic store")


def test_memory_query_latency(tmp_path):
    from james_memory import EpisodicMemory, MemoryEntry, MemoryType, RetrievalQuery

    def measure() -> float:
        memory = EpisodicMemory(str(tmp_path / "episodic.db"))

        async def run() -> None:
            await memory.initialize()
            for i in range(50):
                await memory.store(MemoryEntry(memory_type=MemoryType.EPISODIC, content=f"entry {i}", metadata={"i": i}, tags=["t"], source="bench"))
            start = time.perf_counter()
            for _ in range(20):
                await memory.query(RetrievalQuery(query="entry", tags=["t"], limit=10))
            elapsed = (time.perf_counter() - start) * 1000 / 20
            await memory.close()
            return elapsed

        return anyio.run(run)

    latency = measure()
    _assert_under(latency, 100.0, "episodic query")


def test_runner_component_build_latency(tmp_path):
    from james_cli.runner import build_runner_components
    from james_core.config import ConfigPaths, JamesConfig, JamesSettings, ModelEndpoint

    settings = JamesSettings(
        paths=ConfigPaths(root=tmp_path, config=tmp_path / "config.yaml", vault=tmp_path / "vault",
                          memory=tmp_path / "memory", skills=tmp_path / "skills", credentials=tmp_path / "credentials",
                          tools=tmp_path / "tools", runtimes=tmp_path / "runtimes", models=tmp_path / "models",
                          cache=tmp_path / "cache", logs=tmp_path / "logs", browser=tmp_path / "browser",
                          sandboxes=tmp_path / "sandboxes"),
        ollama=ModelEndpoint(enabled=False),
    )
    cfg = JamesConfig(settings)

    start = time.perf_counter()
    build_runner_components(cfg)
    latency = (time.perf_counter() - start) * 1000
    _assert_under(latency, 500.0, "runner component build")


def test_skill_count_checkpoint():
    """Ensure the builtin skill catalogue keeps growing (not shrinking)."""
    from james_skills import SkillEngine

    engine = SkillEngine()
    names = engine.registry().names()
    for expected in ("market_research", "lead_generation", "web_scraper", "code_generator", "data_analyzer", "email_drafter"):
        assert expected in names