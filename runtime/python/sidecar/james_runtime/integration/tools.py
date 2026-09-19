"""Tool-level bridges from JAMES to optional ecosystem adapters.

These wrappers are intentionally provider-specific and return ToolResult so
they can participate in the existing JAMES tool execution contract.
"""
from __future__ import annotations

import json
from typing import Any, Mapping

from james_runtime.tools.base import Tool, ToolResult
from james_runtime.integration.ecosystem import EcosystemRegistry


class EcosystemTool(Tool):
    def __init__(self, name: str, description: str, parameters: dict[str, dict[str, Any]], registry: EcosystemRegistry, provider: str) -> None:
        self.name, self.description, self.parameters = name, description, parameters
        self.registry, self.provider = registry, provider

    def adapter(self) -> Any:
        return self.registry.get(self.provider)

    async def _json_result(self, value: Any) -> ToolResult:
        return ToolResult(success=True, output=json.dumps(value, ensure_ascii=False, default=str), metadata={"provider": self.provider})


class FirecrawlSearchTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry) -> None:
        super().__init__("web_search", "Search the public web through the explicitly enabled Firecrawl adapter.", {"query": {"type": "string", "description": "Search query"}, "limit": {"type": "integer", "description": "Maximum results"}}, registry, "firecrawl")

    async def _run(self, query: str = "", limit: int = 10, **kwargs: Any) -> ToolResult:
        if not query.strip():
            return ToolResult(False, "", "query is required")
        return await self._json_result(await self.adapter().search(query, limit=max(1, min(limit, 50))))


class FirecrawlScrapeTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry) -> None:
        super().__init__("web_scrape", "Extract a public web page through Firecrawl.", {"url": {"type": "string", "description": "Public HTTP(S) URL"}}, registry, "firecrawl")

    async def _run(self, url: str = "", **kwargs: Any) -> ToolResult:
        if not url.strip():
            return ToolResult(False, "", "url is required")
        return await self._json_result(await self.adapter().scrape(url))


class DifyAgentTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry) -> None:
        super().__init__("dify_agent", "Run an explicitly enabled Dify agent.", {"query": {"type": "string", "description": "Agent request"}}, registry, "dify")

    async def _run(self, query: str = "", **kwargs: Any) -> ToolResult:
        if not query.strip():
            return ToolResult(False, "", "query is required")
        return await self._json_result(await self.adapter().run_agent(query))


class N8nWebhookTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry) -> None:
        super().__init__("n8n_webhook", "Trigger an explicitly enabled n8n webhook.", {"webhook_path": {"type": "string", "description": "Webhook path"}, "payload": {"type": "object", "description": "Webhook payload"}}, registry, "n8n")

    async def _run(self, webhook_path: str = "", payload: Mapping[str, Any] | None = None, **kwargs: Any) -> ToolResult:
        if not webhook_path.strip():
            return ToolResult(False, "", "webhook_path is required")
        return await self._json_result(await self.adapter().trigger_webhook(webhook_path, payload or {}))


def ecosystem_tools(registry: EcosystemRegistry) -> list[Tool]:
    """Return only tools whose provider has been explicitly registered."""
    factories = {
        "firecrawl": (FirecrawlSearchTool, FirecrawlScrapeTool),
        "dify": (DifyAgentTool,),
        "n8n": (N8nWebhookTool,),
    }
    result: list[Tool] = []
    for provider, classes in factories.items():
        if provider in registry.enabled():
            result.extend(cls(registry) for cls in classes)
    return result
