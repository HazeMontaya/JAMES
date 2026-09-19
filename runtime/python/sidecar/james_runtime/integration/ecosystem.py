"""Provider-neutral ecosystem contracts inspired by mature agent platforms.

The external projects are references, not runtime dependencies. JAMES owns the
orchestration, policy, memory and authorization boundaries; adapters implement
these contracts when an external service is enabled.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Mapping, Protocol, Sequence
import time


class IntegrationDomain(str, Enum):
    REALTIME_VOICE = "realtime_voice"
    AGENT_WORKFLOW = "agent_workflow"
    WEB_RESEARCH = "web_research"
    AUTOMATION = "automation"
    MULTI_AGENT = "multi_agent"


@dataclass(frozen=True)
class IntegrationProfile:
    key: str
    domain: IntegrationDomain
    capabilities: tuple[str, ...]
    optional_dependency: str
    source_repository: str


PROFILES: tuple[IntegrationProfile, ...] = (
    IntegrationProfile(
        "livekit",
        IntegrationDomain.REALTIME_VOICE,
        ("voice_sessions", "stt", "tts", "turn_detection", "telephony", "mcp_tools"),
        "livekit-agents",
        "livekit/agents",
    ),
    IntegrationProfile(
        "dify",
        IntegrationDomain.AGENT_WORKFLOW,
        ("visual_workflows", "rag", "agent_runs", "model_management", "observability"),
        "dify",
        "langgenius/dify",
    ),
    IntegrationProfile(
        "firecrawl",
        IntegrationDomain.WEB_RESEARCH,
        ("search", "scrape", "crawl", "extract", "browser_interaction"),
        "@mendable/firecrawl-js",
        "firecrawl/firecrawl",
    ),
    IntegrationProfile(
        "n8n",
        IntegrationDomain.AUTOMATION,
        ("webhooks", "workflow_nodes", "credentials", "schedules", "human_approval"),
        "n8n",
        "n8n-io/n8n",
    ),
    IntegrationProfile(
        "crewai",
        IntegrationDomain.MULTI_AGENT,
        ("crews", "flows", "delegation", "memory", "guardrails", "tracing"),
        "crewai",
        "crewAIInc/crewAI",
    ),
)


class RealtimeVoiceAdapter(Protocol):
    async def start_session(self, *, session_id: str, metadata: Mapping[str, Any]) -> Mapping[str, Any]: ...
    async def stop_session(self, *, session_id: str) -> None: ...


class WebResearchAdapter(Protocol):
    async def search(self, query: str, *, limit: int = 10) -> Sequence[Mapping[str, Any]]: ...
    async def scrape(self, url: str) -> Mapping[str, Any]: ...


class WorkflowAdapter(Protocol):
    async def trigger(self, workflow_id: str, payload: Mapping[str, Any]) -> Mapping[str, Any]: ...


@dataclass
class EcosystemRegistry:
    """Runtime registry for optional external adapters.

    Registration is explicit. Merely installing a provider never gives it
    permission to execute privileged JAMES capabilities.
    """

    profiles: dict[str, IntegrationProfile] = field(
        default_factory=lambda: {p.key: p for p in PROFILES}
    )
    adapters: dict[str, Any] = field(default_factory=dict)
    _health: dict[str, tuple[bool, float, str | None]] = field(default_factory=dict)

    def register(self, key: str, adapter: Any) -> None:
        if key not in self.profiles:
            raise KeyError(f"unknown ecosystem integration: {key}")
        self.adapters[key] = adapter

    def enabled(self) -> tuple[str, ...]:
        return tuple(sorted(self.adapters))

    def capability_matrix(self) -> dict[str, tuple[str, ...]]:
        return {
            key: self.profiles[key].capabilities
            for key in self.enabled()
        }

    def get(self, key: str) -> Any:
        if key not in self.adapters:
            raise KeyError(f"integration is not enabled: {key}")
        return self.adapters[key]

    def provider_for(self, capability: str) -> tuple[str, ...]:
        """Return enabled providers advertising a capability, deterministically."""
        return tuple(key for key in self.enabled() if capability in self.profiles[key].capabilities)

    def capability_providers(self) -> dict[str, tuple[str, ...]]:
        matrix: dict[str, list[str]] = {}
        for provider in self.enabled():
            for capability in self.profiles[provider].capabilities:
                matrix.setdefault(capability, []).append(provider)
        return {name: tuple(values) for name, values in sorted(matrix.items())}

    def mark_health(self, provider: str, healthy: bool, error: str | None = None) -> None:
        if provider not in self.profiles:
            raise KeyError(f"unknown ecosystem integration: {provider}")
        self._health[provider] = (healthy, time.time(), error)

    def health(self, provider: str) -> dict[str, Any]:
        if provider not in self.profiles:
            raise KeyError(f"unknown ecosystem integration: {provider}")
        healthy, checked_at, error = self._health.get(provider, (True, 0.0, None))
        return {"provider": provider, "healthy": healthy, "checked_at": checked_at, "error": error}

    def health_matrix(self) -> dict[str, dict[str, Any]]:
        return {provider: self.health(provider) for provider in self.enabled()}
