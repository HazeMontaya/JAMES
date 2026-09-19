"""Tool-level bridges from JAMES to optional ecosystem adapters.

These wrappers are intentionally provider-specific and return ToolResult so
they can participate in the existing JAMES tool execution contract.
"""
from __future__ import annotations

import json
from typing import Any, Callable, Mapping
import time

from james_runtime.tools.base import Tool, ToolResult
from james_runtime.integration.ecosystem import EcosystemRegistry


class EcosystemTool(Tool):
    def __init__(self, name: str, description: str, parameters: dict[str, dict[str, Any]], registry: EcosystemRegistry, provider: str, event_sink: Callable[..., Any] | None = None) -> None:
        self.name, self.description, self.parameters = name, description, parameters
        self.registry, self.provider, self.event_sink = registry, provider, event_sink

    async def _execute(self, operation: str, awaitable: Any, *, correlation_id: str | None = None, causation_id: str | None = None) -> ToolResult:
        started = time.perf_counter()
        try:
            value = await awaitable
            duration_ms = int((time.perf_counter() - started) * 1000)
            self.registry.mark_health(self.provider, True)
            metadata = {"provider": self.provider, "operation": operation, "duration_ms": duration_ms}
            if correlation_id:
                metadata["correlation_id"] = correlation_id
            if causation_id:
                metadata["causation_id"] = causation_id
            if self.event_sink:
                self.event_sink("ECOSYSTEM_TOOL_COMPLETED", metadata)
            return ToolResult(success=True, output=json.dumps(value, ensure_ascii=False, default=str), metadata=metadata)
        except Exception as exc:
            duration_ms = int((time.perf_counter() - started) * 1000)
            error = f"{type(exc).__name__}: {exc}"
            self.registry.mark_health(self.provider, False, error=error)
            metadata = {"provider": self.provider, "operation": operation, "duration_ms": duration_ms}
            if correlation_id:
                metadata["correlation_id"] = correlation_id
            if causation_id:
                metadata["causation_id"] = causation_id
            if self.event_sink:
                self.event_sink("ECOSYSTEM_TOOL_FAILED", {**metadata, "error_type": type(exc).__name__})
            return ToolResult(success=False, output="", error=error, metadata=metadata)

    def adapter(self) -> Any:
        return self.registry.get(self.provider)

    async def _json_result(self, value: Any) -> ToolResult:
        return ToolResult(success=True, output=json.dumps(value, ensure_ascii=False, default=str), metadata={"provider": self.provider})


class FirecrawlSearchTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry, event_sink: Callable[..., Any] | None = None) -> None:
        super().__init__("web_search", "Search the public web through the explicitly enabled Firecrawl adapter.", {"query": {"type": "string", "description": "Search query"}, "limit": {"type": "integer", "description": "Maximum results"}}, registry, "firecrawl", event_sink)

    async def _run(self, query: str = "", limit: int = 10, **kwargs: Any) -> ToolResult:
        if not query.strip():
            return ToolResult(False, "", "query is required")
        return await self._execute(
            "search",
            self.adapter().search(query, limit=max(1, min(limit, 50))),
            correlation_id=kwargs.get("correlation_id"),
            causation_id=kwargs.get("causation_id"),
        )


class FirecrawlScrapeTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry, event_sink: Callable[..., Any] | None = None) -> None:
        super().__init__("web_scrape", "Extract a public web page through Firecrawl.", {"url": {"type": "string", "description": "Public HTTP(S) URL"}}, registry, "firecrawl", event_sink)

    async def _run(self, url: str = "", **kwargs: Any) -> ToolResult:
        if not url.strip():
            return ToolResult(False, "", "url is required")
        return await self._execute(
            "scrape",
            self.adapter().scrape(url),
            correlation_id=kwargs.get("correlation_id"),
            causation_id=kwargs.get("causation_id"),
        )


class DifyAgentTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry, event_sink: Callable[..., Any] | None = None) -> None:
        super().__init__("dify_agent", "Run an explicitly enabled Dify agent.", {"query": {"type": "string", "description": "Agent request"}}, registry, "dify", event_sink)

    async def _run(self, query: str = "", **kwargs: Any) -> ToolResult:
        if not query.strip():
            return ToolResult(False, "", "query is required")
        return await self._execute(
            "run_agent",
            self.adapter().run_agent(query),
            correlation_id=kwargs.get("correlation_id"),
            causation_id=kwargs.get("causation_id"),
        )


class N8nWebhookTool(EcosystemTool):
    def __init__(self, registry: EcosystemRegistry, event_sink: Callable[..., Any] | None = None) -> None:
        super().__init__("n8n_webhook", "Trigger an explicitly enabled n8n webhook.", {"webhook_path": {"type": "string", "description": "Webhook path"}, "payload": {"type": "object", "description": "Webhook payload"}}, registry, "n8n", event_sink)

    async def _run(self, webhook_path: str = "", payload: Mapping[str, Any] | None = None, **kwargs: Any) -> ToolResult:
        if not webhook_path.strip():
            return ToolResult(False, "", "webhook_path is required")
        return await self._execute(
            "trigger_webhook",
            self.adapter().trigger_webhook(webhook_path, payload or {}),
            correlation_id=kwargs.get("correlation_id"),
            causation_id=kwargs.get("causation_id"),
        )


def ecosystem_tools(registry: EcosystemRegistry, event_sink: Callable[..., Any] | None = None) -> list[Tool]:
    """Return only tools whose provider has been explicitly registered."""
    factories = {
        "firecrawl": (FirecrawlSearchTool, FirecrawlScrapeTool),
        "dify": (DifyAgentTool,),
        "n8n": (N8nWebhookTool,),
    }
    result: list[Tool] = []
    for provider, classes in factories.items():
        if provider in registry.enabled():
            result.extend(cls(registry, event_sink=event_sink) for cls in classes)
    return result
