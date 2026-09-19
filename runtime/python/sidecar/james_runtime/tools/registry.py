"""JAMES Tool Registry"""
import logging
from typing import Dict, Optional

from james_runtime.tools.base import Tool

logger = logging.getLogger(__name__)


class ToolRegistry:
    """Registry of available tools"""

    def __init__(self):
        self._tools: Dict[str, Tool] = {}

    def register(self, tool: Tool) -> None:
        self._tools[tool.name] = tool

    def register_many(self, tools) -> None:
        for tool in tools:
            self.register(tool)

    def get(self, name: str) -> Optional[Tool]:
        return self._tools.get(name)

    def list_tools(self) -> list:
        return [t for t in self._tools.values()]

    def schemas(self) -> list:
        return [t.schema() for t in self._tools.values()]

    async def execute(
        self,
        name: str,
        args: dict,
        *,
        broker_authorized: bool = False,
        correlation_id: str | None = None,
        causation_id: str | None = None,
    ) -> dict:
        """Execute a tool only after Rust broker authorization."""
        if not broker_authorized:
            return {
                "success": False,
                "output": "",
                "error": "CapabilityExecutionDenied: execution requires Rust broker authorization",
            }
        tool = self._tools.get(name)
        if not tool:
            return {"success": False, "output": "", "error": f"Unknown tool: {name}"}
        result = await tool.run(**args)
        return result.to_dict()
"""JAMES Tool Registry"""
import logging
from typing import Dict, Optional

from james_runtime.tools.base import Tool

logger = logging.getLogger(__name__)


class ToolRegistry:
    """Registry of available tools"""

    def __init__(self):
        self._tools: Dict[str, Tool] = {}

    def register(self, tool: Tool) -> None:
        self._tools[tool.name] = tool

    def register_many(self, tools) -> None:
        for tool in tools:
            self.register(tool)

    def get(self, name: str) -> Optional[Tool]:
        return self._tools.get(name)

    def list_tools(self) -> list:
        return [t for t in self._tools.values()]

    def schemas(self) -> list:
        return [t.schema() for t in self._tools.values()]

    async def execute(self, name: str, args: dict) -> dict:
        """Execute a tool by name"""
        tool = self._tools.get(name)
        if not tool:
            return {"success": False, "output": "", "error": f"Unknown tool: {name}"}
        result = await tool.run(**args)
        return result.to_dict()