"""NATS capability bridge for the JAMES Python sidecar.

The Rust python-bridge owns the Rust capability registry and sends execution
requests over NATS request/reply. This module exposes the Python ToolRegistry
as remotely executable capabilities and publishes capability metadata.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import time
from typing import Any

from nats.aio.client import Client as NATS

from james_runtime.tools.registry import ToolRegistry

logger = logging.getLogger(__name__)


class NatsCapabilityBridge:
    """Expose a ToolRegistry to the Rust JAMES capability broker."""

    def __init__(
        self,
        registry: ToolRegistry,
        nats_url: str | None = None,
        subject_prefix: str | None = None,
        service_name: str | None = None,
    ) -> None:
        self.registry = registry
        self.nats_url = nats_url or os.getenv("JAMES_BRIDGE_NATS_URL", "nats://localhost:4222")
        self.subject_prefix = subject_prefix or os.getenv("JAMES_BRIDGE_SUBJECT_PREFIX", "james.bridge")
        self.service_name = service_name or os.getenv("JAMES_BRIDGE_PYTHON_SERVICE", "james-python")
        self.client = NATS()
        self._subscriptions: list[Any] = []

    @property
    def execute_subject(self) -> str:
        return f"{self.subject_prefix}.capability.execute.*"

    @property
    def register_subject(self) -> str:
        return f"{self.subject_prefix}.capability.register"

    @property
    def health_subject(self) -> str:
        return f"{self.subject_prefix}.health"

    @property
    def list_subject(self) -> str:
        return f"{self.subject_prefix}.capability.list"

    @staticmethod
    def _category(tool_name: str) -> str:
        if tool_name.startswith("file_"):
            return "file"
        if tool_name in {"python_exec"}:
            return "process"
        if tool_name in {"http_get"}:
            return "network"
        if tool_name.startswith("memory_"):
            return "database"
        if tool_name in {"get_current_time", "math_eval", "calculator"}:
            return "system"
        return "automation"

    @staticmethod
    def _capability_info(tool: Any) -> dict[str, Any]:
        return {
            "id": tool.name,
            "name": tool.name.replace("_", " ").title(),
            "category": NatsCapabilityBridge._category(tool.name),
            "version": "1.0.0",
            "description": tool.description,
            "required_permissions": [],
            "input_schema": {"type": "object", "properties": tool.parameters},
            "output_schema": None,
        }

    async def start(self) -> None:
        await self.client.connect(
            servers=[self.nats_url],
            name=self.service_name,
            reconnect_time_wait=2,
            max_reconnect_attempts=-1,
        )

        await self.client.subscribe(self.execute_subject, cb=self._handle_execute)
        await self.client.subscribe(self.health_subject, cb=self._handle_health)
        await self.client.subscribe(self.list_subject, cb=self._handle_list)

        for tool in self.registry.list_tools():
            await self._register(tool)

        await self.client.flush()
        logger.info(
            "NATS capability bridge online: %d tools exposed on %s",
            len(self.registry.list_tools()),
            self.execute_subject,
        )

    async def _register(self, tool: Any) -> None:
        payload = self._capability_info(tool)
        await self.client.publish(
            self.register_subject,
            json.dumps(payload).encode("utf-8"),
        )

    async def register_tool(self, tool: Any) -> None:
        """Register a tool added after bridge startup."""
        self.registry.register(tool)
        if self.client.is_connected:
            await self._register(tool)

    async def _execute_request(self, payload: bytes) -> dict[str, Any]:
        started = time.perf_counter()
        request_id = ""
        try:
            request = json.loads(payload.decode("utf-8"))
            request_id = str(request.get("request_id", ""))
            capability_id = str(request.get("capability_id", ""))
            caller = str(request.get("caller", "unknown"))
            args = request.get("input", {})

            if not request_id:
                raise ValueError("missing request_id")
            if not capability_id:
                raise ValueError("missing capability_id")
            if not isinstance(args, dict):
                raise ValueError("capability input must be a JSON object")

            tool = self.registry.get(capability_id)
            if tool is None:
                raise ValueError(f"capability not found: {capability_id}")

            logger.info(
                "Executing Python capability %s for caller %s",
                capability_id,
                caller,
            )
            result = await self.registry.execute(capability_id, args)
            return {
                "request_id": request_id,
                "success": bool(result.get("success")),
                "output": result.get("output"),
                "error": result.get("error"),
                "duration_ms": int((time.perf_counter() - started) * 1000),
            }
        except Exception as exc:
            logger.exception("Python capability execution failed")
            return {
                "request_id": request_id,
                "success": False,
                "output": None,
                "error": str(exc),
                "duration_ms": int((time.perf_counter() - started) * 1000),
            }

    async def _handle_execute(self, msg: Any) -> None:
        response = await self._execute_request(msg.data)
        if msg.reply:
            await self.client.publish(msg.reply, json.dumps(response).encode("utf-8"))

    async def _handle_list(self, msg: Any) -> None:
        if not msg.reply:
            return
        payload = {"capabilities": [self._capability_info(tool) for tool in self.registry.list_tools()]}
        await self.client.publish(msg.reply, json.dumps(payload).encode("utf-8"))

    async def _handle_health(self, msg: Any) -> None:
        if not msg.reply:
            return
        payload = {
            "service": self.service_name,
            "status": "healthy",
            "timestamp": __import__("datetime").datetime.now(__import__("datetime").timezone.utc).isoformat(),
            "capabilities": [tool.name for tool in self.registry.list_tools()],
        }
        await self.client.publish(msg.reply, json.dumps(payload).encode("utf-8"))

    async def close(self) -> None:
        if self.client.is_connected:
            await self.client.drain()
            await self.client.close()
