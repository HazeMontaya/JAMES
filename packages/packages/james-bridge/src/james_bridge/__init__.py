"""JAMES Bridge - Python side of the Rust/Python bridge"""

from .client import BridgeClient
from .config import BridgeConfig
from .handler import CapabilityHandler

__version__ = "0.1.0"

__all__ = [
    "BridgeClient",
    "BridgeConfig",
    "CapabilityHandler",
]