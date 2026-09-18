"""JAMES World Access Layer"""

from .access import WorldAccess
from .capability import (
    Backend,
    BackendStatus,
    BackendType,
    Capability,
    CapabilityCategory,
    CapabilityRegistry,
    CapabilityRequest,
    CapabilityResponse,
    HealthStatus,
)
from .credentials import CredentialVault

__version__ = "0.1.0"

__all__ = [
    "Backend",
    "BackendStatus",
    "BackendType",
    "Capability",
    "CapabilityCategory",
    "CapabilityRegistry",
    "CapabilityRequest",
    "CapabilityResponse",
    "CredentialVault",
    "HealthStatus",
    "WorldAccess",
]
