"""Capability registry - discovery, health checks, routing"""

import builtins
import time
from datetime import UTC, datetime
from typing import Any

import structlog

from .models import (
    Backend,
    Capability,
    CapabilityRequest,
    CapabilityResponse,
    HealthStatus,
)

logger = structlog.get_logger()


class CapabilityRegistry:
    def __init__(self) -> None:
        self._capabilities: dict[str, Capability] = {}

    def register(self, capability: Capability) -> None:
        self._capabilities[capability.name] = capability

    def add_backend(self, capability_name: str, backend: Backend) -> None:
        cap = self._capabilities.get(capability_name)
        if not cap:
            cap = Capability(name=capability_name)
            self._capabilities[capability_name] = cap
        cap.add_backend(backend)

    def get(self, capability_name: str) -> Capability | None:
        return self._capabilities.get(capability_name)

    def list(self) -> list[Capability]:
        return list(self._capabilities.values())

    def names(self) -> builtins.list[str]:
        return sorted(self._capabilities.keys())

    def available_names(self) -> builtins.list[str]:
        return sorted(n for n, c in self._capabilities.items() if c.health == HealthStatus.HEALTHY and c.backends)

    async def check_health(self, capability_name: str | None = None) -> None:
        targets = [self._capabilities[n] for n in (list([capability_name]) if capability_name else self._capabilities)]
        for cap in targets:
            await self._check_capability_health(cap)

    async def _check_capability_health(self, cap: Capability) -> None:
        healthy = 0
        for backend in cap.backends:
            if not backend.enabled:
                continue
            if backend.health_check:
                start = time.monotonic()
                try:
                    ok = await backend.health_check()
                    backend.status.health = HealthStatus.HEALTHY if ok else HealthStatus.DOWN
                except Exception as e:
                    ok = False
                    backend.status.health = HealthStatus.DOWN
                    backend.status.error = str(e)
                backend.status.latency_ms = (time.monotonic() - start) * 1000
                backend.status.last_checked = datetime.now(UTC)
                if ok:
                    healthy += 1
            else:
                backend.status.health = HealthStatus.UNKNOWN
                healthy += 1
        cap.health = HealthStatus.HEALTHY if healthy > 0 else HealthStatus.DOWN

    async def execute(self, request: CapabilityRequest) -> CapabilityResponse:
        cap = self._capabilities.get(request.capability)
        if not cap:
            return CapabilityResponse(capability=request.capability, status=HealthStatus.DOWN, error=f"Capability not found: {request.capability}")

        for backend in sorted(cap.backends, key=lambda b: b.priority, reverse=True):
            if not backend.enabled:
                continue
            if backend.status.health == HealthStatus.DOWN:
                continue
            if not backend.execute:
                continue
            start = time.monotonic()
            try:
                data = await backend.execute(request)
                return CapabilityResponse(
                    capability=request.capability,
                    backend_id=backend.id,
                    status=HealthStatus.HEALTHY,
                    data=data,
                    latency_ms=(time.monotonic() - start) * 1000,
                )
            except Exception as e:
                backend.status.health = HealthStatus.DOWN
                backend.status.error = str(e)
                logger.warning("Backend failed, trying next", capability=request.capability, backend=backend.id, error=str(e))

        return CapabilityResponse(
            capability=request.capability,
            status=HealthStatus.DOWN,
            error=f"All backends failed for capability: {request.capability}",
        )

    def to_dict(self) -> dict[str, dict[str, Any]]:
        out = {}
        for name, cap in self._capabilities.items():
            out[name] = {
                "category": cap.category.value,
                "health": cap.health.value,
                "backends": {b.id: {"type": b.type.value, "enabled": b.enabled, "health": b.status.health.value} for b in cap.backends},
                "auth_required": cap.auth_required,
                "privacy": cap.privacy_level.value,
                "reliability": cap.reliability,
            }
        return out
