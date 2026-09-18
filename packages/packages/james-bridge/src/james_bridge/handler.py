"""Capability execution handler for Python side"""

import json
import time
from typing import TYPE_CHECKING, Any

import nats
import structlog
from nats.aio.client import Client as NatsClient
from nats.aio.msg import Msg
from nats.js import JetStreamContext

from .config import BridgeConfig

if TYPE_CHECKING:
    from james_core.cognition.model_router import ModelRouter
    from james_memory import MemoryRouter
    from james_skills import SkillEngine
    from james_skills.engine.models import SkillSpec
    from james_world import WorldAccess

logger = structlog.get_logger()


class CapabilityHandler:
    """Handles capability execution requests from Rust bridge"""

    def __init__(self, config: BridgeConfig):
        self.config = config
        self.nc: NatsClient | None = None
        self.js: JetStreamContext | None = None
        self._skill_engine: SkillEngine | None = None
        self._world: WorldAccess | None = None
        self._memory: MemoryRouter | None = None
        self._model_router: ModelRouter | None = None

    async def connect(self) -> None:
        """Connect to NATS"""
        self.nc = await nats.connect(self.config.nats_url)
        self.js = self.nc.jetstream()
        logger.info("Python bridge connected to NATS", url=self.config.nats_url)

    async def setup_skill_engine(self) -> None:
        """Initialize SkillEngine and dependencies lazily"""
        if self._skill_engine is None:
            from james_core.cognition.model_router import build_model_router
            from james_core.config import JamesConfig
            from james_memory import MemoryRouter
            from james_skills import SkillEngine
            from james_world import WorldAccess

            cfg = JamesConfig()

            # Memory
            from james_memory import (
                BusinessMemory,
                EpisodicMemory,
                MemoryRouter,
                ProceduralMemory,
                SemanticMemory,
                UserMemory,
            )
            memory = MemoryRouter(
                episodic=EpisodicMemory(str(cfg.paths.memory / "episodic.db")),
                semantic=SemanticMemory(str(cfg.paths.vault)),
                procedural=ProceduralMemory(str(cfg.paths.memory / "procedural.db")),
                business=BusinessMemory(str(cfg.paths.memory / "business.db")),
                user=UserMemory(str(cfg.paths.vault)),
            )

            # World
            world = WorldAccess()

            # Model router
            model_router = None
            if cfg.enabled_model_endpoints():
                model_router = build_model_router(cfg)

            # Skill engine
            engine = SkillEngine(skill_dirs=[str(cfg.paths.skills)])

            self._memory = memory
            self._world = world
            self._model_router = model_router
            self._skill_engine = engine

            logger.info("Skill engine and dependencies initialized")

    async def register_capabilities(self) -> None:
        """Register Python skills as capabilities with Rust bridge"""
        await self.setup_skill_engine()

        assert self._skill_engine is not None
        skills = self._skill_engine.list_skills()
        for spec in skills:
            cap_info = {
                "id": spec.name,
                "name": spec.name,
                "category": spec.category,
                "version": spec.version,
                "description": spec.description,
                "required_permissions": spec.prerequisites,
                "input_schema": self._build_input_schema(spec),
                "output_schema": self._build_output_schema(spec),
            }
            await self._publish_capability_register(cap_info)
            logger.info("Registered capability", capability=spec.name)

    def _build_input_schema(self, spec: "SkillSpec") -> dict[str, Any]:
        """Build JSON schema from skill inputs"""
        properties = {}
        required = []
        for inp in spec.inputs:
            properties[inp.name] = {
                "type": inp.type,
                "description": inp.description,
            }
            if inp.required:
                required.append(inp.name)
            if inp.default is not None:
                properties[inp.name]["default"] = inp.default

        return {
            "type": "object",
            "properties": properties,
            "required": required,
        }

    def _build_output_schema(self, spec: "SkillSpec") -> dict[str, Any]:
        """Build JSON schema from skill outputs"""
        properties = {}
        for out in spec.outputs:
            properties[out.name] = {
                "type": out.type,
                "description": out.description,
            }

        return {
            "type": "object",
            "properties": properties,
        }

    async def _publish_capability_register(self, cap_info: dict[str, Any]) -> None:
        """Publish capability registration to Rust bridge"""
        subject = self.config.capability_register_subject
        assert self.nc is not None
        await self.nc.publish(subject, json.dumps(cap_info).encode())

    async def start_listening(self) -> None:
        """Start listening for capability execution requests"""
        await self.connect()
        await self.register_capabilities()

        # Subscribe to capability execute requests
        subject = self.config.capability_execute_subject
        assert self.nc is not None
        await self.nc.subscribe(subject, cb=self._handle_execute_request)

        # Subscribe to capability list requests
        await self.nc.subscribe(self.config.capability_list_subject, cb=self._handle_list_request)

        # Subscribe to health checks
        await self.nc.subscribe(self.config.health_subject, cb=self._handle_health)

        logger.info("Python bridge listening for requests", subject=subject)

    async def _handle_execute_request(self, msg: Msg) -> None:
        """Handle capability execution request from Rust"""
        try:
            data = json.loads(msg.data.decode())
            request_id = data.get("request_id")
            capability_id = data.get("capability_id")
            caller = data.get("caller", "unknown")
            input_data = data.get("input", {})

            logger.info("Executing capability", capability=capability_id, request_id=request_id, caller=caller)

            start = time.monotonic()
            await self.setup_skill_engine()

            # Execute via SkillEngine
            assert self._skill_engine is not None
            assert self._memory is not None
            assert self._world is not None
            spec = self._skill_engine.get_skill(capability_id)
            if spec is None:
                response = {
                    "request_id": request_id,
                    "success": False,
                    "output": None,
                    "error": f"Capability not found: {capability_id}",
                    "duration_ms": 0,
                }
            else:
                from james_skills.engine.models import SkillContext

                ctx = SkillContext(
                    goal_id=request_id,
                    inputs=input_data,
                    memory=self._memory,
                    world=self._world,
                    model_router=self._model_router,
                    event_bus=None,
                )

                result = await self._skill_engine.execute(spec, ctx)
                duration_ms = int((time.monotonic() - start) * 1000)

                response = {
                    "request_id": request_id,
                    "success": result.success,
                    "output": result.outputs if result.success else None,
                    "error": result.error if not result.success else None,
                    "duration_ms": duration_ms,
                }

            # Reply to Rust
            assert self.nc is not None
            if msg.reply:
                await self.nc.publish(msg.reply, json.dumps(response).encode())

        except Exception as e:
            logger.error("Error handling execute request", error=str(e))
            error_response = {
                "request_id": data.get("request_id", "unknown"),
                "success": False,
                "output": None,
                "error": str(e),
                "duration_ms": 0,
            }
            if msg.reply:
                assert self.nc is not None
                await self.nc.publish(msg.reply, json.dumps(error_response).encode())

    async def _handle_list_request(self, msg: Msg) -> None:
        """Handle capability list request"""
        await self.setup_skill_engine()
        assert self._skill_engine is not None
        skills = self._skill_engine.list_skills()
        cap_list = [s.name for s in skills]

        response = {"capabilities": cap_list}

        assert self.nc is not None
        if msg.reply:
            await self.nc.publish(msg.reply, json.dumps(response).encode())

    async def _handle_health(self, msg: Msg) -> None:
        """Handle health check request"""
        await self.setup_skill_engine()
        assert self._skill_engine is not None
        skills = self._skill_engine.list_skills()

        health = {
            "service": self.config.service_name,
            "status": "healthy",
            "timestamp": time.time(),
            "capabilities": [s.name for s in skills],
        }

        assert self.nc is not None
        if msg.reply:
            await self.nc.publish(msg.reply, json.dumps(health).encode())

    async def close(self) -> None:
        """Close connections"""
        if self._memory:
            await self._memory.close()
        if self.nc:
            await self.nc.close()
        logger.info("Python bridge closed")