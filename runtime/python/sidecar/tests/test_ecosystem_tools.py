import pytest

from james_runtime.integration.ecosystem import EcosystemRegistry
from james_runtime.integration.tools import ecosystem_tools


class FakeFirecrawl:
    async def search(self, query, *, limit=10):
        return [{"query": query, "limit": limit}]

    async def scrape(self, url):
        return {"url": url}


class FakeDify:
    async def run_agent(self, query, **kwargs):
        return {"query": query}


class FakeN8n:
    async def trigger_webhook(self, webhook_path, payload):
        return {"path": webhook_path, "payload": payload}


@pytest.mark.asyncio
async def test_ecosystem_tools_require_explicit_provider_registration():
    registry = EcosystemRegistry()
    assert ecosystem_tools(registry) == []

    registry.register("firecrawl", FakeFirecrawl())
    registry.register("dify", FakeDify())
    registry.register("n8n", FakeN8n())
    names = {tool.name for tool in ecosystem_tools(registry)}
    assert {"web_search", "web_scrape", "dify_agent", "n8n_webhook"} <= names

    result = await next(t for t in ecosystem_tools(registry) if t.name == "web_search").run(query="JAMES", limit=2)
    assert result.success
    assert '"limit": 2' in result.output


@pytest.mark.asyncio
async def test_ecosystem_tool_emits_telemetry_and_updates_health():
    registry = EcosystemRegistry()
    registry.register("firecrawl", FakeFirecrawl())
    events = []

    tool = next(
        t for t in ecosystem_tools(registry, event_sink=lambda event_type, payload: events.append((event_type, payload)))
        if t.name == "web_search"
    )
    result = await tool.run(query="telemetry", limit=1)

    assert result.success
    assert result.metadata["provider"] == "firecrawl"
    assert result.metadata["operation"] == "search"
    assert result.metadata["duration_ms"] >= 0
    assert registry.health("firecrawl")["healthy"] is True
    assert events[0][0] == "ECOSYSTEM_TOOL_COMPLETED"
    assert events[0][1]["provider"] == "firecrawl"
    assert "query" not in events[0][1]


@pytest.mark.asyncio
async def test_ecosystem_tool_failure_is_redacted_and_marks_provider_unhealthy():
    class BrokenFirecrawl:
        async def search(self, query, *, limit=10):
            raise RuntimeError("secret query should never enter event payload")

    registry = EcosystemRegistry()
    registry.register("firecrawl", BrokenFirecrawl())
    events = []
    tool = next(
        t for t in ecosystem_tools(registry, event_sink=lambda event_type, payload: events.append((event_type, payload)))
        if t.name == "web_search"
    )

    result = await tool.run(query="secret query", limit=1)

    assert not result.success
    assert registry.health("firecrawl")["healthy"] is False
    assert events[0][0] == "ECOSYSTEM_TOOL_FAILED"
    assert events[0][1]["error_type"] == "RuntimeError"
    assert "secret query" not in str(events[0][1])
