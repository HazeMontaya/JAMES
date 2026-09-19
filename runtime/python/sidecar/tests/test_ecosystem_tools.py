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
