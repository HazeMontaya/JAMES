"""Capability models for JAMES World Access"""

import enum
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any


class CapabilityCategory(enum.Enum):
    API = "api"
    CLI = "cli"
    BROWSER = "browser"
    SCRAPER = "scraper"
    MCP = "mcp"
    RSS = "rss"
    SEARCH = "search"
    LOCAL = "local"


class BackendType(enum.Enum):
    HTTP = "http"
    CLI = "cli"
    BROWSER = "browser"
    LIBRARY = "library"
    MCP = "mcp"


class HealthStatus(enum.Enum):
    UNKNOWN = "unknown"
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    DOWN = "down"


class PrivacyLevel(enum.Enum):
    PUBLIC = "public"
    AUTHENTICATED = "authenticated"
    PRIVATE = "private"


@dataclass
class RateLimit:
    calls_per_hour: int = 0
    remaining: int = 0
    resets_at: datetime | None = None


@dataclass
class CostEstimate:
    per_call: float = 0.0
    currency: str = "EUR"
    notes: str = ""


@dataclass
class BackendStatus:
    backend_id: str = ""
    health: HealthStatus = HealthStatus.UNKNOWN
    last_checked: datetime = field(default_factory=lambda: datetime.now(UTC))
    latency_ms: float = 0.0
    error: str | None = None


@dataclass
class CapabilityRequest:
    capability: str = ""
    action: str = "read"
    params: dict[str, Any] = field(default_factory=dict)
    timeout: float = 30.0


@dataclass
class CapabilityResponse:
    capability: str = ""
    backend_id: str = ""
    status: HealthStatus = HealthStatus.HEALTHY
    data: Any = None
    latency_ms: float = 0.0
    error: str | None = None


@dataclass
class Backend:
    id: str = ""
    capability: str = ""
    type: BackendType = BackendType.HTTP
    priority: int = 0
    enabled: bool = True
    health_check: Callable[[], Awaitable[bool]] | None = None
    execute: Callable[[CapabilityRequest], Awaitable[Any]] | None = None
    status: BackendStatus = field(default_factory=BackendStatus)


@dataclass
class Capability:
    name: str = ""
    category: CapabilityCategory = CapabilityCategory.API
    backends: list[Backend] = field(default_factory=list)
    health: HealthStatus = HealthStatus.UNKNOWN
    cost_per_call: CostEstimate = field(default_factory=CostEstimate)
    rate_limit: RateLimit = field(default_factory=RateLimit)
    auth_required: bool = False
    privacy_level: PrivacyLevel = PrivacyLevel.PUBLIC
    reliability: float = 0.5
    latency_p50: float = 0.0
    description: str = ""

    def add_backend(self, backend: Backend) -> None:
        backend.capability = self.name
        self.backends.append(backend)
        self.backends.sort(key=lambda b: b.priority, reverse=True)
