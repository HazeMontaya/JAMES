"""Optional integrations for external agent ecosystems."""
from james_runtime.integration.ecosystem import (
    EcosystemRegistry, IntegrationDomain, IntegrationProfile, PROFILES,
    RealtimeVoiceAdapter, WebResearchAdapter, WorkflowAdapter,
)
from james_runtime.integration.http_adapters import (
    CrewAIAdapter, DifyAdapter, FirecrawlAdapter, LiveKitAdapter,
    N8nAdapter, HttpIntegrationError,
)

__all__ = [
    "EcosystemRegistry", "IntegrationDomain", "IntegrationProfile", "PROFILES",
    "RealtimeVoiceAdapter", "WebResearchAdapter", "WorkflowAdapter",
    "CrewAIAdapter", "DifyAdapter", "FirecrawlAdapter", "LiveKitAdapter",
    "N8nAdapter", "HttpIntegrationError",
]
