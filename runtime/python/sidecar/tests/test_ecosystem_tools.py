import pytest

class FakeFirecrawl:
    async def search(self, query: str, *, limit: int = 10):
        return [{"query": query, "limit": limit}]

    async def scrape(self, url: str):
        return {"url": url}


class FakeDify:
    async def run_agent(self, query: str, **kwargs):
        return {"query": query}


class FakeN8n:
    async def trigger_webhook(self, webhook_path: str, payload):
        return {"webhook_path": webhook_path, "payload": payload}


def test_ecosystem_capability_metadata_has_explicit_risk_permissions():
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge

    registry = __import__("james_runtime.tools.registry", fromlist=["ToolRegistry"]).ToolRegistry()
    registry.register("firecrawl", FakeFirecrawl())
    registry.register("dify", FakeDify())
    registry.register("n8n", FakeN8n())

    bridge = NatsCapabilityBridge(registry)
    assert bridge._capability_info(next(t for t in registry.list_tools() if t.name == "web_search"))["required_permissions"] == ["network.public_web"]
    assert bridge._capability_info(next(t for t in registry.list_tools() if t.name == "dify_agent"))["risk_level"] == "high"
    assert bridge._capability_info(next(t for t in registry.list_tools() if t.name == "n8n_webhook"))["required_permissions"] == ["integration.n8n.execute", "integration.n8n.communicate"]


@pytest.mark.asyncio
async def test_nats_capability_bridge_rejects_missing_or_invalid_token(monkeypatch):
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
    from james_runtime.tools.registry import ToolRegistry

    monkeypatch.setenv("JAMES_BRIDGE_TOKEN", "test-secret")
    registry = ToolRegistry()
    bridge = NatsCapabilityBridge(registry)

    missing = await bridge._execute_request(
        b'{"request_id":"r1","capability_id":"missing","caller":"attacker","input":{}}'
    )
    assert missing["success"] is False
    assert "authentication" in missing["error"]

    invalid = await bridge._execute_request(
        b'{"request_id":"r2","capability_id":"missing","caller":"attacker","bridge_token":"wrong","input":{}}'
    )
    assert invalid["success"] is False
    assert "authentication" in invalid["error"]


@pytest.mark.asyncio
async def test_nats_capability_bridge_accepts_valid_token(monkeypatch):
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
    from james_runtime.tools.base import Tool, ToolResult
    from james_runtime.tools.registry import ToolRegistry

    class EchoTool(Tool):
        name = "echo"
        description = "echo"
        parameters = {"value": {"type": "string"}}

        async def _run(self, **kwargs):
            return ToolResult(success=True, output=kwargs["value"])

    monkeypatch.setenv("JAMES_BRIDGE_TOKEN", "test-secret")
    registry = ToolRegistry()
    registry.register(EchoTool())
    bridge = NatsCapabilityBridge(registry)

    result = await bridge._execute_request(
        b'{"request_id":"r3","capability_id":"echo","caller":"broker","bridge_token":"test-secret","input":{"value":"ok"}}'
    )
    assert result["success"] is True
    assert result["request_id"] == "r3"
    assert result["output"] == "ok"


@pytest.mark.asyncio
async def test_nats_capability_bridge_redacts_tool_errors(monkeypatch):
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
    from james_runtime.tools.base import Tool, ToolResult
    from james_runtime.tools.registry import ToolRegistry

    class BrokenTool(Tool):
        name = "broken"
        description = "broken"
        parameters = {}

        async def _run(self, **kwargs):
            return ToolResult(
                success=False,
                output="",
                error="secret payload must not cross the bridge",
            )

    monkeypatch.setenv("JAMES_BRIDGE_TOKEN", "test-secret")
    registry = ToolRegistry()
    registry.register(BrokenTool())
    bridge = NatsCapabilityBridge(registry)

    result = await bridge._execute_request(
        b'{"request_id":"r4","capability_id":"broken","caller":"broker","bridge_token":"test-secret","input":{}}'
    )
    assert result["success"] is False
    assert "secret payload" not in str(result)


@pytest.mark.asyncio
async def test_tool_registry_denies_direct_execution_without_rust_broker_authorization():
    from james_runtime.tools.base import Tool, ToolResult
    from james_runtime.tools.registry import ToolRegistry

    class EchoTool(Tool):
        name = "echo_direct"
        description = "echo"
        parameters = {"value": {"type": "string"}}

        async def _run(self, **kwargs):
            return ToolResult(success=True, output=kwargs["value"])

    registry = ToolRegistry()
    registry.register(EchoTool())
    denied = await registry.execute("echo_direct", {"value": "blocked"})
    assert denied["success"] is False
    assert "Rust broker authorization" in denied["error"]

    allowed = await registry.execute("echo_direct", {"value": "ok"}, broker_authorized=True)
    assert allowed["success"] is True
    assert allowed["output"] == "ok"
