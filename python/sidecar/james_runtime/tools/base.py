"""JAMES Tool Abstraction"""
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional


@dataclass
class ToolResult:
    """Result of a tool execution"""
    success: bool
    output: str
    error: Optional[str] = None
    metadata: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "success": self.success,
            "output": self.output,
            "error": self.error,
            "metadata": self.metadata,
        }


class Tool(ABC):
    """Base class for all tools"""

    name: str = ""
    description: str = ""
    parameters: Dict[str, Dict[str, Any]] = {}

    def schema(self) -> Dict[str, Any]:
        """JSON schema for the tool"""
        return {
            "name": self.name,
            "description": self.description,
            "parameters": {
                prop: {
                    "type": spec.get("type", "string"),
                    "description": spec.get("description", ""),
                }
                for prop, spec in self.parameters.items()
            },
        }

    async def run(self, **kwargs) -> ToolResult:
        """Execute the tool"""
        try:
            return await self._run(**kwargs)
        except Exception as e:
            return ToolResult(success=False, output="", error=f"{type(e).__name__}: {e}")

    @abstractmethod
    async def _run(self, **kwargs) -> ToolResult:
        """Tool implementation"""
        raise NotImplementedError