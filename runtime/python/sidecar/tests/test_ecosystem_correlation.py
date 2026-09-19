import pytest

from james_runtime.integration.ecosystem import EcosystemRegistry
from james_runtime.integration.tools import FirecrawlSearchTool
from james_runtime.tools.base import ToolResult


class FakeFirecrawl:
    async def search(self, query: str, *, limit: int = 10):
        return [{"query": query, "limit": limit, "ok": True}]

    async def scrape(self, url: str):
        return {"url": url}


class BrokenFirecrawl:
    async def search(self, query: str, *, limit: int = 10):
        raise RuntimeError("sensitive provider payload")

    async def scrape(self, url: str):
        return {"url": url}


@pytest.mark.asyncio
async def test_ecosystem_tool_propagates_correlation_and_causation_metadata():
    registry = EcosystemRegistry()
    registry.register("firecrawl", FakeFirecrawl())
    events = []

    def sink(event_type, metadata):
        events.append((event_type, metadata))

    tool = FirecrawlSearchTool(registry, event_sink=sink)
    result = await tool._run(
        query="james",
        limit=3,
        correlation_id="corr-123",
        causation_id="cause-456",
    )

    assert result.success is True
    assert result.metadata["correlation_id"] == "corr-123"
    assert result.metadata["causation_id"] == "cause-456"
    assert events == [
        (
            "ECOSYSTEM_TOOL_COMPLETED",
            {
                "provider": "firecrawl",
                "operation": "search",
                "duration_ms": events[0][1]["duration_ms"],
                "correlation_id": "corr-123",
                "causation_id": "cause-456",
            },
        )
    ]
    assert registry.health("firecrawl")["healthy"] is True


@pytest.mark.asyncio
async def test_ecosystem_tool_failure_keeps_correlation_and_redacts_error_event():
    registry = EcosystemRegistry()
    registry.register("firecrawl", BrokenFirecrawl())
    events = []

    def sink(event_type, metadata):
        events.append((event_type, metadata))

    tool = FirecrawlSearchTool(registry, event_sink=sink)
    result = await tool._run(
        query="private",
        correlation_id="corr-failure",
        causation_id="cause-failure",
    )

    assert result.success is False
    assert result.metadata["correlation_id"] == "corr-failure"
    assert result.metadata["causation_id"] == "cause-failure"
    assert "sensitive provider payload" in result.error
    assert events[0][0] == "ECOSYSTEM_TOOL_FAILED"
    assert events[0][1]["correlation_id"] == "corr-failure"
    assert events[0][1]["causation_id"] == "cause-failure"
    assert "error" not in events[0][1]
    assert "query" not in events[0][1]
    assert "sensitive provider payload" not in str(events[0][1])
    assert registry.health("firecrawl")["healthy"] is False


@pytest.mark.asyncio
async def test_nats_bridge_tracks_capability_health_after_execution(monkeypatch):
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
    from james_runtime.tools.base import Tool, ToolResult
    from james_runtime.tools.registry import ToolRegistry

    class EchoTool(Tool):
        name = "echo"
        description = "echo"
        parameters = {}

        async def _run(self, **kwargs):
            return ToolResult(success=True, output="ok")

    monkeypatch.setenv("JAMES_BRIDGE_TOKEN", "test-secret")
    registry = ToolRegistry()
    registry.register(EchoTool())
    bridge = NatsCapabilityBridge(registry)

    result = await bridge._execute_request(
        b'{"request_id":"health-1","capability_id":"echo","caller":"broker","bridge_token":"test-secret","input":{}}'
    )
    assert result["success"] is True
    assert bridge._capability_health["echo"] is True


@pytest.mark.asyncio
async def test_nats_bridge_health_payload_reports_degraded_capability(monkeypatch):
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
    from james_runtime.tools.base import Tool
    from james_runtime.tools.registry import ToolRegistry

    class EchoTool(Tool):
        name = "echo"
        description = "echo"
        parameters = {}

        async def _run(self, **kwargs):
            return None

    monkeypatch.setenv("JAMES_BRIDGE_TOKEN", "test-secret")
    registry = ToolRegistry()
    registry.register(EchoTool())
    bridge = NatsCapabilityBridge(registry)
    bridge._capability_health["echo"] = False

    payload = bridge._health_payload()
    assert payload["status"] == "degraded"
    assert payload["capability_health"] == {"echo": False}


@pytest.mark.asyncio
async def test_tool_registry_preserves_execution_correlation_metadata():
    from james_runtime.tools.registry import ToolRegistry
    from james_runtime.tools.base import Tool, ToolResult

    class CaptureTool(Tool):
        name = "capture"
        description = "capture metadata"
        parameters = {}

        async def _run(self, **kwargs):
            return ToolResult(success=True, output=kwargs)

    registry = ToolRegistry()
    registry.register(CaptureTool())
    result = await registry.execute(
        "capture", {}, correlation_id="corr-reg", causation_id="cause-reg"
    )
    assert result["success"] is True
    assert result["output"]["correlation_id"] == "corr-reg"
    assert result["output"]["causation_id"] == "cause-reg"


@pytest.mark.asyncio
async def test_nats_bridge_passes_correlation_context_to_tool(monkeypatch):
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
    from james_runtime.tools.base import Tool, ToolResult
    from james_runtime.tools.registry import ToolRegistry

    seen = {}

    class CaptureTool(Tool):
        name = "capture"
        description = "capture metadata"
        parameters = {}

        async def _run(self, **kwargs):
            seen.update(kwargs)
            return ToolResult(success=True, output="ok")

    monkeypatch.setenv("JAMES_BRIDGE_TOKEN", "test-secret")
    registry = ToolRegistry()
    registry.register(CaptureTool())
    bridge = NatsCapabilityBridge(registry)
    result = await bridge._execute_request(
        b'{"request_id":"corr-1","capability_id":"capture","caller":"broker","bridge_token":"test-secret","correlation_id":"corr-outer","causation_id":"cause-outer","input":{}}'
    )
    assert result["success"] is True
    assert seen["correlation_id"] == "corr-outer"
    assert seen["causation_id"] == "cause-outer"
