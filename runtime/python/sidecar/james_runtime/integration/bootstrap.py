"""Environment-driven optional ecosystem bootstrap.

No provider is enabled merely because its package is installed. Each adapter
requires an explicit base URL environment variable; credentials are read only
at construction time and never persisted in JAMES events.
"""
from __future__ import annotations

import os
from typing import Any, Callable

from james_runtime.integration.ecosystem import EcosystemRegistry
from james_runtime.integration.http_adapters import (
    CrewAIAdapter, DifyAdapter, FirecrawlAdapter, LiveKitAdapter, N8nAdapter,
)
from james_runtime.integration.tools import ecosystem_tools
from james_runtime.tools.registry import ToolRegistry


def configure_ecosystems(
    tool_registry: ToolRegistry,
    *,
    env: dict[str, str] | None = None,
    event_sink: Callable[..., Any] | None = None,
) -> EcosystemRegistry:
    values = env if env is not None else os.environ
    registry = EcosystemRegistry()

    factories: list[tuple[str, str, Any, str]] = [
        ("firecrawl", "JAMES_FIRECRAWL_URL", FirecrawlAdapter, "JAMES_FIRECRAWL_API_KEY"),
        ("dify", "JAMES_DIFY_URL", DifyAdapter, "JAMES_DIFY_API_KEY"),
        ("n8n", "JAMES_N8N_URL", N8nAdapter, "JAMES_N8N_API_KEY"),
        ("crewai", "JAMES_CREWAI_URL", CrewAIAdapter, "JAMES_CREWAI_API_KEY"),
        ("livekit", "JAMES_LIVEKIT_URL", LiveKitAdapter, "JAMES_LIVEKIT_API_KEY"),
    ]
    for provider, url_key, adapter_cls, key_key in factories:
        base_url = values.get(url_key)
        if not base_url:
            continue
        api_key = values.get(key_key)
        registry.register(provider, adapter_cls(base_url=base_url, api_key=api_key))

    tool_registry.register_many(ecosystem_tools(registry, event_sink=event_sink))
    return registry
