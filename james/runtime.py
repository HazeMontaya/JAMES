"""Core runtime boundary for JAMES."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class JamesRuntime:
    """Owns runtime state without coupling it to a specific model provider."""

    capabilities: set[str] = field(default_factory=lambda: {
        "runtime",
        "model-routing",
        "tools",
        "memory",
        "tasks",
    })

    def status(self) -> str:
        return (
            "JAMES runtime online | "
            f"version=0.1.0 | capabilities={','.join(sorted(self.capabilities))}"
        )
