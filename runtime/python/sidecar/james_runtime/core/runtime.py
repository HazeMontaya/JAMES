"""JAMES Runtime - Main unified API"""
import asyncio
import logging
from typing import Optional, Dict, Any, List, AsyncGenerator
from contextlib import asynccontextmanager
from pathlib import Path
from fastapi import FastAPI, HTTPException
from fastapi.responses import StreamingResponse

from james_runtime.config import RuntimeSettings, get_settings
from james_runtime.core.requests import (
    CompletionRequest, StreamingRequest, CompletionResponse, Chunk,
    RoutingRequest, RoutingDecision, RuntimeRequest, EngineSelection,
    ToolCallRequest, ToolCallResult
)
from james_runtime.core.events import (
    RuntimeEvent, create_model_selected, create_runtime_changed,
    create_model_loading, create_model_ready, create_inference_started,
    create_inference_completed, create_cost_recorded, create_vram_pressure,
    create_oom_recovery, create_budget_alert
)
from james_runtime.core.errors import (
    RuntimeError, OOMError, ModelUnavailableError, NoSuitableModelError,
    NoSuitableEngineError, EngineNotFoundError, BudgetExceededError,
    ModelNotApprovedError, InferenceTimeoutError
)
from james_runtime.abstraction.base import EngineConfig
from james_runtime.abstraction.registry import EngineRegistry
from james_runtime.abstraction.factory import EngineFactory
from james_runtime.routing.model_router import ModelRouter
from james_runtime.routing.runtime_router import RuntimeRouter
from james_runtime.routing.hardware import HardwareDetector
from james_runtime.routing.vram_manager import VRAMManager
from james_runtime.routing.context import ContextManager
from james_runtime.policies.cost import CostTracker, BudgetEnforcer
from james_runtime.models.registry import ModelRegistry
from james_runtime.models.pricing import PricingRegistry
from james_runtime.telemetry.metrics import MetricsCollector
from james_runtime.memory import EventLog
from james_runtime.autonomy.heartbeat import DurableHeartbeat, HeartbeatTask
from james_runtime.autonomy.decision import AutonomousDecisionLoop
from james_runtime.autonomy.mission import AutonomousMissionManager, AutonomousMission
from james_runtime.steering.liquid import liquid_race

logger = logging.getLogger(__name__)


class JamesRuntime:
    """Main JAMES Runtime - Unified API for all inference operations"""
    
    def __init__(self, settings: Optional[RuntimeSettings] = None):
        self.settings = settings or get_settings()
        self._initialized = False
        
        # Core components
        self.engine_registry = EngineRegistry()
        self.engine_factory = EngineFactory(self.settings)
        self.model_registry = ModelRegistry()
        self.pricing_registry = PricingRegistry()
        
        # Detection & Management
        self.hardware_detector = HardwareDetector()
        self.hardware_profile = None
        self.vram_manager = None
        
        # Routers
        self.model_router = None
        self.runtime_router = None
        self.context_manager = ContextManager()
        
        # Policies
        self.cost_tracker = None
        self.budget_enforcer = None
        
        # Telemetry
        self.metrics = MetricsCollector()
        self.event_handlers: List[callable] = []
        # Durable event history mirrors Jared-style session continuity while
        # keeping runtime events machine-replayable. The log is outside source code.
        self.event_log = EventLog(Path(".james") / "events.jsonl")
        self.heartbeat = DurableHeartbeat(Path(".james") / "heartbeat.json")
        self.autonomy = AutonomousDecisionLoop(self.event_log, self.heartbeat)
        self.missions = AutonomousMissionManager(self.event_log, Path(".james") / "missions.json")

        # Agent system (lazy)
        self._agent_system = None
        
        # Health
        self._health_check_task: Optional[asyncio.Task] = None
        self._heartbeat_task: Optional[asyncio.Task] = None
    
    async def initialize(self) -> None:
        """Initialize all components"""
        if self._initialized:
            return
        
        logger.info("Initializing JAMES Runtime...")
        
        # 1. Detect hardware
        self.hardware_profile = self.hardware_detector.detect()
        self.vram_manager = VRAMManager(self.hardware_profile, self.settings.vram_headroom)
        self._emit_event("HARDWARE_DETECTED", {
            "gpu_name": self.hardware_profile.gpu_name,
            "vram_gb": self.hardware_profile.vram_gb,
            "cuda_version": self.hardware_profile.cuda_version,
            "metal_support": self.hardware_profile.metal_support,
            "cpu_cores": self.hardware_profile.cpu_cores,
            "ram_gb": self.hardware_profile.ram_gb,
        })
        
        # 2. Initialize engines
        await self._initialize_engines()
        
        # 3. Initialize registries
        self.model_registry.initialize()
        self.pricing_registry.load_defaults()
        
        # 4. Initialize routers
        self.model_router = ModelRouter(self.model_registry, self.pricing_registry, self.settings.router)
        self.runtime_router = RuntimeRouter(
            self.engine_registry, 
            self.hardware_profile, 
            self.vram_manager
        )
        
        # 5. Initialize policies
        self.cost_tracker = CostTracker(str(self.settings.db_path))
        self.budget_enforcer = BudgetEnforcer(self.settings.budget, self.cost_tracker)
        
        # 6. Restore durable autonomy state and register bounded handlers.
        await self.heartbeat.restore()
        await self.missions.restore()
        self._register_autonomous_missions()
        self.heartbeat.register(HeartbeatTask("runtime.engine_health", 30.0, self._heartbeat_engine_health, timeout_seconds=15.0))
        self.heartbeat.register(HeartbeatTask("autonomy.decision", 60.0, self._heartbeat_autonomy_decision, timeout_seconds=5.0))
        self._health_check_task = asyncio.create_task(self._health_check_loop(), name="james-health")
        self._heartbeat_task = asyncio.create_task(self.heartbeat.run(poll_seconds=1.0), name="james-heartbeat")
        
        self._initialized = True
        logger.info("JAMES Runtime initialized successfully")
    
    def _register_autonomous_missions(self) -> None:
        """Register read-only mission handlers; registration is the policy boundary."""
        self.missions.register("run_health_sweep", self._mission_health_sweep)
        self.missions.register("inspect_failure", self._mission_inspect_failure)
        self.missions.register("inspect_inference_reliability", self._mission_inspect_inference)
        self.missions.register("inspect_repository", self._mission_inspect_repository)
        self.missions.register("inspect_selfmade_regression", self._mission_inspect_selfmade_regression)
        self.missions.register("inspect_selfmade_opportunity", self._mission_inspect_selfmade_opportunity)

    async def _heartbeat_autonomy_decision(self) -> None:
        decision = self.autonomy.tick()
        mission = await self.missions.dispatch(decision)
        if mission is not None:
            self._emit_event("AUTONOMY_MISSION_RESULT", {
                "mission_id": mission.mission_id,
                "action": mission.action,
                "status": mission.status,
                "attempts": mission.attempts,
                "error": mission.error,
            })

    async def _mission_health_sweep(self, mission: AutonomousMission) -> dict[str, Any]:
        await self._heartbeat_engine_health()
        return {"observation": "engine_health", "mission_id": mission.mission_id}

    async def _mission_inspect_failure(self, mission: AutonomousMission) -> dict[str, Any]:
        failures = {name: self.heartbeat.failures(name) for name in ("runtime.engine_health", "autonomy.decision")}
        self._emit_event("AUTONOMY_FAILURE_INSPECTION", {"mission_id": mission.mission_id, "failures": failures})
        return {"failures": failures}

    async def _mission_inspect_inference(self, mission: AutonomousMission) -> dict[str, Any]:
        events = self.event_log.tail(250)
        relevant = [e.event_type for e in events if e.event_type in {"INFERENCE_ERROR", "MODEL_FALLBACK", "ENGINE_ERROR"}][-10:]
        self._emit_event("AUTONOMY_INFERENCE_INSPECTION", {"mission_id": mission.mission_id, "events": relevant})
        return {"reliability_events": relevant}

    async def _mission_inspect_repository(self, mission: AutonomousMission) -> dict[str, Any]:
        self._emit_event("AUTONOMY_REPOSITORY_INSPECTION", {"mission_id": mission.mission_id})
        return {"observation": "repository_state_recorded"}

    async def _mission_inspect_selfmade_regression(self, mission: AutonomousMission) -> dict[str, Any]:
        events = self.event_log.tail(250)
        failures = [e for e in events if e.event_type in {"selfmade.evaluation.completed", "selfmade.change.failed"} and e.payload.get("passed") is False]
        latest = failures[-1] if failures else None
        self._emit_event("AUTONOMY_SELFMADE_REGRESSION_INSPECTION", {
            "mission_id": mission.mission_id,
            "event_id": latest.event_id if latest else None,
        })
        return {"failed_evaluations": len(failures), "latest_event_id": latest.event_id if latest else None}

    async def _mission_inspect_selfmade_opportunity(self, mission: AutonomousMission) -> dict[str, Any]:
        events = self.event_log.tail(250)
        successful = [e for e in events if e.event_type == "selfmade.evaluation.completed" and e.payload.get("passed") is True]
        self._emit_event("AUTONOMY_SELFMADE_OPPORTUNITY_INSPECTION", {
            "mission_id": mission.mission_id,
            "successful_evaluations": len(successful),
        })
        return {"successful_evaluations": len(successful), "next_action": "operator_review"}

    async def _initialize_engines(self) -> None:
        """Initialize all available engines"""
        engine_config = EngineConfig(
            model_spec=None,
            vllm=self.settings.vllm,
            llamacpp=self.settings.llamacpp,
            airllm=self.settings.airllm,
        )
        engine_configs = [
            ("vllm", self.settings.vllm),
            ("llamacpp", self.settings.llamacpp),
            ("airllm", self.settings.airllm),
        ]
        
        for engine_type, config in engine_configs:
            try:
                engine = self.engine_factory.create(engine_type, config)
                await engine.initialize(engine_config)
                self.engine_registry.register(engine)
                logger.info(f"Engine {engine_type} initialized successfully")
            except Exception as e:
                logger.warning(f"Engine {engine_type} initialization failed: {e}")
                # Continue without this engine
    
    async def shutdown(self) -> None:
        """Graceful shutdown"""
        logger.info("Shutting down JAMES Runtime...")
        self.heartbeat.stop()
        
        for task in (self._heartbeat_task, self._health_check_task):
            if task:
                task.cancel()
                try:
                    await task
                except asyncio.CancelledError:
                    pass
        self._heartbeat_task = None
        self._health_check_task = None
        
        # Shutdown engines
        for engine in self.engine_registry.list_all():
            await engine.shutdown()
        
        logger.info("JAMES Runtime shut down complete")
    
    # ============ Core API ============
    
    async def generate(self, request: CompletionRequest) -> CompletionResponse:
        """Generate completion with automatic routing"""
        return await self._execute_generation(request, stream=False)
    
    async def stream(self, request: StreamingRequest) -> AsyncGenerator[str, None]:
        """Stream completion with automatic routing"""
        gen = await self._execute_generation(request, stream=True)
        async for chunk in gen:
            yield f"data: {chunk.model_dump_json()}\n\n"
        yield "data: [DONE]\n\n"
    
    async def _execute_generation(self, request, stream: bool) -> CompletionResponse | AsyncGenerator[Chunk, None]:
        # 1. Budget check
        routing_req = self._to_routing_request(request)
        budget_check = self.budget_enforcer.check(routing_req)
        if not budget_check.allowed:
            raise BudgetExceededError(budget_check.reason, 0, 0)
        
        # 2. Model routing. The router filters out models whose advertised
        # context window cannot hold the complete prompt + requested output.
        routing_decision = await self.model_router.route(routing_req)

        selected_spec = self.model_registry.get_model_spec(routing_decision.model_id)
        if selected_spec is not None:
            prompt_tokens = ContextManager.estimate_message_tokens([
                {"role": m.role, "content": m.content} for m in request.messages
            ])
            requested_output = request.max_tokens or 512
            required_context = prompt_tokens + requested_output
            model_context = getattr(selected_spec, "max_context", None)
            if model_context and required_context > model_context:
                raise ModelUnavailableError(
                    routing_decision.model_id,
                    f"request needs about {required_context} tokens but model supports {model_context}; "
                    "reduce history/output or use a larger-context model",
                )
        
        # Emit model selected event
        self._emit_event("MODEL_SELECTED", {
            "model_id": routing_decision.model_id,
            "task": "completion",
            "quality_tier": request.temperature,  # simplified
            "privacy": "prefer_local",  # simplified
            "reasoning": routing_decision.reasoning,
        })
        
        # 3. Runtime selection
        model_spec = self.model_registry.get_model_spec(routing_decision.model_id)
        runtime_selection = self.runtime_router.select(model_spec)
        
        # Emit runtime changed if different from hint
        if request.runtime_hint and request.runtime_hint != runtime_selection.engine_type.value:
            self._emit_event("RUNTIME_CHANGED", {
                "from_engine": request.runtime_hint,
                "to_engine": runtime_selection.engine_type.value,
                "model_id": routing_decision.model_id,
                "reason": "optimal for hardware",
            })
        
        # 3.5 Emit model loading
        self._emit_event("MODEL_LOADING", {
            "model_id": routing_decision.model_id,
            "engine": runtime_selection.engine_type.value,
            "estimated_vram_gb": runtime_selection.estimated_vram_gb,
        })
        
        # 4. Get engine and execute
        engine = self.engine_registry.get(runtime_selection.engine_type.value)
        if not engine:
            raise EngineNotFoundError(runtime_selection.engine_type.value)
        
        # Emit model ready
        self._emit_event("MODEL_READY", {
            "model_id": routing_decision.model_id,
            "engine": runtime_selection.engine_type.value,
        })
        
        # 5. Execute with budget tracking
        request_copy = request.model_copy()
        request_copy.model = routing_decision.model_id
        # Normalize oversized histories before handing them to an engine.
        model_context = getattr(model_spec, "max_context", None)
        if model_context:
            input_budget = max(1, model_context - (request.max_tokens or 512))
            # The engine remains responsible for exact tokenization; this only
            # bounds pathological prompt sizes when no tokenizer is available.
            messages = [m.model_dump() for m in request.messages]
            estimated = ContextManager.estimate_message_tokens(messages)
            if estimated > input_budget:
                raise ModelUnavailableError(
                    routing_decision.model_id,
                    f"prompt is approximately {estimated} tokens but only "
                    f"{input_budget} input tokens remain",
                )
        request_copy.runtime_hint = runtime_selection.engine_type.value
        request_copy.model = routing_decision.model_id
        
        start_time = asyncio.get_event_loop().time()
        self._emit_event("INFERENCE_STARTED", {"request_id": request.request_id})
        
        try:
            if hasattr(request, 'stream') and request.stream:
                return self._stream_with_tracking(engine, request_copy, routing_decision, runtime_selection)
            else:
                response = await asyncio.wait_for(
                    engine.complete(request_copy),
                    timeout=request.max_tokens * 100 / 1000 if request.max_tokens else 30.0  # rough timeout
                )
                
                duration_ms = int((asyncio.get_event_loop().time() - start_time) * 1000)
                self._track_and_emit(response, routing_decision, duration_ms, runtime_selection.engine_type.value)
                return response
                
        except asyncio.TimeoutError:
            raise InferenceTimeoutError("request", 30000)
        except Exception as e:
            # Try fallback chain
            return await self._try_fallback(request, routing_decision, e)
    
    async def _stream_with_tracking(self, engine, request, routing_decision, runtime_selection):
        """Stream with tracking and events"""
        start_time = asyncio.get_event_loop().time()
        token_count = 0
        
        async for chunk in engine.stream(request):
            self._emit_event("TOKEN_STREAM", {
                "request_id": request.request_id,
                "token": chunk.choices[0].get("delta", {}).get("content", "") if chunk.choices else "",
                "token_index": token_count,
            })
            token_count += 1
            yield chunk
        
        duration_ms = int((asyncio.get_event_loop().time() - start_time) * 1000)
        # Track cost and emit completion
        self._emit_event("INFERENCE_COMPLETED", {
            "model_id": request.model,
            "engine": runtime_selection.engine_type.value,
            "tokens_generated": token_count,
            "duration_ms": duration_ms,
        })
    
    def _to_routing_request(self, request: CompletionRequest) -> RoutingRequest:
        # Simple heuristic: determine task from messages
        last_msg = request.messages[-1].content if request.messages else ""
        task = self._classify_task(last_msg)
        
        prompt_tokens = ContextManager.estimate_message_tokens([
            {"role": m.role, "content": m.content} for m in request.messages
        ])
        requested_output = request.max_tokens or 512
        # Route on total required context, not only output length. This prevents
        # large prompts from reaching models whose context window cannot hold them.
        context_needed = prompt_tokens + requested_output

        return RoutingRequest(
            task=task,
            required_capabilities=self._extract_capabilities(last_msg),
            quality_tier="best" if requested_output > 2000 else "balanced",
            privacy="prefer_local",
            context_length_needed=context_needed,
            preferred_model=request.model if request.model else None,
        )
    
    def _classify_task(self, content: str) -> str:
        content_lower = content.lower()
        if any(kw in content_lower for kw in ["code", "program", "function", "debug", "implement"]):
            return "coding"
        elif any(kw in content_lower for kw in ["plan", "strategy", "steps", "roadmap"]):
            return "planning"
        elif any(kw in content_lower for kw in ["analyze", "research", "investigate", "compare"]):
            return "analysis"
        elif any(kw in content_lower for kw in ["summarize", "summary", "tl;dr"]):
            return "summarization"
        elif any(kw in content_lower for kw in ["translate", "übersetz"]):
            return "translation"
        return "conversation"
    
    def _extract_capabilities(self, content: str) -> List[str]:
        caps = []
        content_lower = content.lower()
        if any(kw in content_lower for kw in ["code", "program", "function"]):
            caps.append("coding")
        if any(kw in content_lower for kw in ["plan", "strategy"]):
            caps.append("planning")
        if any(kw in content_lower for kw in ["analyze", "reason"]):
            caps.append("reasoning")
        return caps or ["conversation"]
    
    def _track_and_emit(self, response, routing_decision, duration_ms, engine="llamacpp"):
        """Track cost and emit events"""
        usage = getattr(response, "usage", None) or {}
        tokens_in = int(usage.get("prompt_tokens", usage.get("input_tokens", 0)) or 0)
        tokens_out = int(usage.get("completion_tokens", usage.get("output_tokens", 0)) or 0)
        cost = self.pricing_registry.calculate_cost(
            routing_decision.model_id, tokens_in, tokens_out
        )

        self.cost_tracker.record_usage(
            model=routing_decision.model_id,
            tokens_in=tokens_in,
            tokens_out=tokens_out,
            cost_usd=cost,
            engine=engine,
        )

        self._emit_event("INFERENCE_COMPLETED", {
            "model_id": routing_decision.model_id,
            "tokens_generated": tokens_out,
            "duration_ms": duration_ms,
            "cost_usd": float(cost),
        })
    
    async def _try_fallback(self, request, routing_decision, original_error):
        """Try fallback models"""
        for fallback_model in routing_decision.fallback_chain:
            try:
                logger.warning(f"Trying fallback model: {fallback_model}")
                self._emit_event("MODEL_FALLBACK", {
                    "from_model": routing_decision.model_id,
                    "to_model": fallback_model,
                    "reason": str(original_error),
                })
                
                # Retry with fallback model
                fallback_request = request.model_copy()
                fallback_request.model = fallback_model
                return await self.generate(fallback_request)
            except Exception:
                continue
        
        # All fallbacks failed
        raise RuntimeError(f"All fallbacks exhausted. Original error: {original_error}")
    
    async def liquid_generate(self, request: CompletionRequest, model_ids: list[str], *, min_delta: float = 8.0):
        """Run candidate models through existing runtime engines with leader upgrades."""
        async def generate(model_id: str):
            candidate = request.model_copy(update={"model": model_id, "stream": False})
            model_spec = self.model_registry.get_model_spec(model_id)
            if model_spec is None:
                raise ModelUnavailableError(model_id, "model is not registered")
            selection = self.runtime_router.select(model_spec)
            engine = self.engine_registry.get(selection.engine_type.value)
            if engine is None:
                raise EngineNotFoundError(selection.engine_type.value)
            candidate.runtime_hint = selection.engine_type.value
            return await engine.complete(candidate)

        result = await liquid_race(model_ids, generate, min_delta=min_delta)
        self._emit_event("LIQUID_RACE_COMPLETE", {
            "models": model_ids,
            "winner": result.winner.model if result.winner else None,
            "winner_score": result.winner.score if result.winner else 0.0,
            "upgrades": len(result.updates),
            "results": [{"model": x.model, "score": x.score, "duration_ms": x.duration_ms, "success": x.success} for x in result.results],
        })
        return result

    async def _heartbeat_engine_health(self):
        """Autonomous maintenance task: probe all registered engines."""
        health = await self.engine_registry.check_all_health()
        unhealthy = [name for name, status in health.items() if not status.healthy]
        self._emit_event("AUTONOMY_HEALTH_SWEEP", {
            "engine_count": len(health),
            "unhealthy_engines": unhealthy,
        })

    async def heartbeat_tick(self):
        """Run registered background health/maintenance tasks once."""
        results = await self.heartbeat.tick()
        for result in results:
            self._emit_event("HEARTBEAT_TASK", {
                "task": result.name,
                "success": result.success,
                "duration_ms": result.duration_ms,
                "error": result.error,
            })
        return results

    # ============ Model Management ============
    
    async def load_model(self, model_id: str) -> Dict[str, Any]:
        """Load a specific model"""
        model_spec = self.model_registry.get_model_spec(model_id)
        selection = self.runtime_router.select(model_spec)
        engine = self.engine_registry.get(selection.engine_type.value)
        
        self._emit_event("MODEL_LOADING", {
            "model_id": model_id,
            "engine": selection.engine_type.value,
            "estimated_vram_gb": selection.estimated_vram_gb,
        })
        
        # In practice, model is loaded during engine initialization
        return {"status": "ready", "model_id": model_id, "engine": selection.engine_type.value}
    
    def get_model_info(self, model_id: str) -> Dict[str, Any]:
        return self.model_registry.get_model_info(model_id)
    
    def list_models(self) -> List[Dict[str, Any]]:
        return self.model_registry.list_models()
    
    # ============ Routing ============
    
    async def route_model(self, request: RoutingRequest) -> Dict[str, Any]:
        decision = await self.model_router.route(request)
        return decision.model_dump()
    
    def select_runtime(self, request: RuntimeRequest) -> EngineSelection:
        model_spec = self.model_registry.get_model_spec(request.model_id)
        return self.runtime_router.select(model_spec)
    
    # ============ Hardware & Health ============
    
    def get_hardware_profile(self) -> Dict[str, Any]:
        return self.hardware_profile.model_dump() if self.hardware_profile else {}
    
    async def check_health(self) -> Dict[str, Any]:
        health = await self.engine_registry.check_all_health()
        return {
            "runtime": "healthy" if self._initialized else "initializing",
            "engines": health,
            "hardware": self.get_hardware_profile(),
            "budget": {
                "daily_spent": float(self.cost_tracker.daily_spent()),
                "monthly_spent": float(self.cost_tracker.monthly_spent()),
                "daily_limit": self.settings.budget.daily_ai_budget_usd,
                "monthly_limit": self.settings.budget.monthly_ai_budget_usd,
            }
        }
    
    # ============ Event System ============
    
    @property
    def agent_system(self):
        if self._agent_system is None:
            from james_runtime.agents import AgentSystem
            self._agent_system = AgentSystem(self, base_dir=Path(".james"))
        return self._agent_system
    
    def add_event_handler(self, handler: callable):
        self.event_handlers.append(handler)
    
    @staticmethod
    def _redact_persistent_event(event_type: str, payload: Dict[str, Any]) -> Dict[str, Any]:
        """Persist operational metadata, not prompt/response content."""
        safe = dict(payload)
        sensitive = {"token", "prompt", "prompt_preview", "content", "response", "messages", "args"}
        for key in list(safe):
            if key.lower() in sensitive or key.lower().endswith("_text"):
                value = safe.pop(key)
                safe[f"{key}_length"] = len(value) if isinstance(value, str) else None
        return safe

    def _emit_event(self, event_type: str, payload: Dict[str, Any]):
        from james_runtime.core.events import RuntimeEvent
        from datetime import datetime
        from uuid import uuid4
        
        event = RuntimeEvent(
            event_type=event_type,
            payload=payload,
            timestamp=datetime.utcnow(),
        )
        try:
            safe_payload = self._redact_persistent_event(event_type, payload)
            self.event_log.append(
                event_type,
                safe_payload,
                source="james-runtime",
                correlation_id=getattr(event, "correlation_id", None),
            )
        except Exception as e:
            logger.warning(f"Persistent event log error: {e}")

        for handler in self.event_handlers:
            try:
                handler(event)
            except Exception as e:
                logging.getLogger(__name__).warning(f"Event handler error: {e}")
    
    async def _health_check_loop(self):
        """Background health checking"""
        while True:
            try:
                await asyncio.sleep(self.settings.vram_poll_interval_ms / 1000)
                if self._initialized:
                    health = await self.engine_registry.check_all_health()
                    
                    # Check VRAM pressure
                    if self.hardware_profile and self.hardware_profile.vram_gb > 0:
                        # Would need actual VRAM monitoring - placeholder
                        pass
                    
                    for engine_type, health in health.items():
                        self._emit_event("ENGINE_HEALTH", {
                            "engine": engine_type,
                            "healthy": health.healthy,
                            "vram_used_gb": health.vram_used_gb,
                            "vram_total_gb": health.vram_total_gb,
                            "latency_p50_ms": health.latency_p50_ms,
                            "error_rate": health.error_rate,
                        })
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Health check error: {e}")