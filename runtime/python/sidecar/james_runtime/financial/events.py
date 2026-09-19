"""Financial Cortex domain events.

These are domain-specialized RuntimeEvent instances; the Python sidecar has one
wire/event envelope rather than a second transport model.
"""
from __future__ import annotations
from typing import Any, Dict
from pydantic import Field
from james_runtime.core.events import RuntimeEvent


class FinancialEvent(RuntimeEvent):
    event_type: str
    source: str = "financial.cortex"
    payload: Dict[str, Any] = Field(default_factory=dict)
