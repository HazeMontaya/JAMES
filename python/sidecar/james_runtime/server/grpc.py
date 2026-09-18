"""JAMES Runtime gRPC Server"""
import logging
import asyncio
import json
import grpc
from concurrent import futures
from typing import Optional

from james_runtime.config import get_settings
from james_runtime.core.runtime import JamesRuntime

logger = logging.getLogger(__name__)

# Import generated gRPC modules (will be generated from proto)
try:
    import james_runtime_pb2
    import james_runtime_pb2_grpc
    GRPC_AVAILABLE = True
except ImportError:
    GRPC_AVAILABLE = False
    james_runtime_pb2 = None
    james_runtime_pb2_grpc = None


class JamesRuntimeServicer:
    """gRPC servicer for JamesRuntime"""
    
    def __init__(self, runtime: JamesRuntime):
        self.runtime = runtime
    
    async def Complete(self, request, context):
        """Unary completion"""
        from james_runtime.core.requests import CompletionRequest, CompletionResponse
        
        req = CompletionRequest(
            model=request.model,
            messages=[{"role": m.role, "content": m.content} for m in request.messages],
            temperature=request.temperature,
            max_tokens=request.max_tokens if request.max_tokens > 0 else None,
            stream=False,
        )
        
        response = await self.runtime.generate(req)
        
        return james_runtime_pb2.CompletionResponse(
            id=response.id,
            model=response.model,
            created=response.created,
            choices=[james_runtime_pb2.Choice(
                index=c.get("index", 0),
                message=james_runtime_pb2.ChatMessage(
                    role=c.get("message", {}).get("role", "assistant"),
                    content=c.get("message", {}).get("content", ""),
                ),
                finish_reason=c.get("finish_reason", "stop"),
            ) for c in response.choices],
            usage=dict((k, str(v)) for k, v in (response.usage or {}).items()),
        )
    
    async def StreamComplete(self, request, context):
        """Streaming completion"""
        from james_runtime.core.requests import StreamingRequest
        
        req = StreamingRequest(
            model=request.model,
            messages=[{"role": m.role, "content": m.content} for m in request.messages],
            temperature=request.temperature,
            max_tokens=request.max_tokens if request.max_tokens > 0 else None,
            stream=True,
        )
        
        async for line in self.runtime.stream(req):
            if not line.startswith("data: "):
                continue
            payload_str = line[len("data: "):].strip()
            if payload_str == "[DONE]":
                break
            payload = json.loads(payload_str)
            yield james_runtime_pb2.Chunk(
                id=payload.get("id", ""),
                model=payload.get("model", ""),
                created=payload.get("created", 0),
                choices=[
                    james_runtime_pb2.Choice(
                        index=c.get("index", 0),
                        message=james_runtime_pb2.ChatMessage(
                            role=c.get("delta", {}).get("role", "assistant"),
                            content=c.get("delta", {}).get("content", ""),
                        ),
                        finish_reason=c.get("finish_reason"),
                    ) for c in payload.get("choices", [])
                ],
            )
    
    def _quality_tier_name(self, value: int) -> str:
        return {1: "fast", 2: "balanced", 3: "best"}.get(value, "balanced")

    def _privacy_name(self, value: int) -> str:
        return {1: "local_only", 2: "prefer_local", 3: "allow_cloud"}.get(value, "prefer_local")

    async def RouteModel(self, request, context):
        """Route model for a task"""
        from james_runtime.core.requests import RoutingRequest, RoutingDecision

        req = RoutingRequest(
            task=request.task,
            required_capabilities=list(request.required_capabilities),
            quality_tier=self._quality_tier_name(request.quality_tier),
            privacy=self._privacy_name(request.privacy),
            latency_budget_ms=request.latency_budget_ms if request.latency_budget_ms > 0 else None,
            cost_budget_per_1k=request.cost_budget_per_1k if request.cost_budget_per_1k > 0 else None,
            context_length_needed=request.context_length_needed if request.context_length_needed > 0 else None,
        )
        
        decision = await self.runtime.route_model(req)
        
        return james_runtime_pb2.RoutingDecision(
            model_id=decision["model_id"],
            provider=decision["provider"],
            reasoning=decision.get("reasoning", ""),
            fallback_chain=decision.get("fallback_chain", ""),
            estimated_cost_per_1k=decision.get("estimated_cost_per_1k") or 0.0,
            estimated_latency_ms=decision.get("estimated_latency_ms") or 0,
        )
    
    def _engine_type_name(self, value: str) -> str:
        return {
            "vllm": "ENGINE_TYPE_VLLM",
            "llamacpp": "ENGINE_TYPE_LLAMACPP",
            "airllm": "ENGINE_TYPE_AIRLLM",
        }.get(value, "ENGINE_TYPE_UNSPECIFIED")

    async def SelectRuntime(self, request, context):
        """Select runtime for a model"""
        from james_runtime.core.requests import RuntimeRequest, EngineSelection
        
        req = RuntimeRequest(
            model_id=request.model_id,
            quantization=request.quantization if request.quantization else None,
            offload=request.offload,
            max_latency_ms=request.max_latency_ms if request.max_latency_ms > 0 else None,
            max_vram_gb=request.max_vram_gb if request.max_vram_gb > 0 else None,
        )
        
        selection = self.runtime.select_runtime(req)
        
        return james_runtime_pb2.RuntimeSelection(
            engine_type=self._engine_type_name(selection.engine_type.value),
            model_id=selection.model_id,
            quantization=selection.quantization or "",
            offload=selection.offload,
            estimated_vram_gb=selection.estimated_vram_gb,
            estimated_latency_ms=selection.estimated_latency_ms,
            fallback_engines=[self._engine_type_name(e.value) for e in selection.fallback_engines],
        )
    
    async def GetHardwareProfile(self, request, context):
        """Get hardware profile"""
        profile = self.runtime.get_hardware_profile()
        # Convert to protobuf
        return james_runtime_pb2.HardwareProfile(
            gpu_name=profile.get("gpu_name", ""),
            vram_gb=profile.get("vram_gb", 0.0),
            cuda_version=profile.get("cuda_version", ""),
            rocm_version=profile.get("rocm_version", ""),
            metal_support=profile.get("metal_support", False),
            cpu_cores=profile.get("cpu_cores", 0),
            ram_gb=profile.get("ram_gb", 0.0),
        )
    
    async def GetModelRegistry(self, request, context):
        """Get model registry"""
        models = self.runtime.list_models()
        return james_runtime_pb2.ModelRegistry(
            models=[james_runtime_pb2.ModelInfo(
                id=m["id"],
                name=m["name"],
                provider=m["provider"],
                parameters_b=int(m["parameters_b"]),
                max_context=m["max_context"],
                capabilities=m["capabilities"],
                quality_tier=m["quality_tier"],
                quality_score=m["quality_score"],
                quantization=m["quantization"],
                format=m["format"],
            ) for m in models]
        )
    
    async def GetCostReport(self, request, context):
        """Get cost report"""
        tracker = self.runtime.cost_tracker

        def _float_map(m):
            return {k: float(v) for k, v in m.items()}

        return james_runtime_pb2.CostReport(
            total_cost_usd=float(tracker.total_spent()),
            cost_by_model=_float_map(tracker.spent_by("model")),
            cost_by_engine=_float_map(tracker.spent_by("engine")),
            cost_by_agent=_float_map(tracker.spent_by("agent_id")),
        )
    
    async def UpdateBudget(self, request, context):
        """Update budget policy"""
        from james_runtime.config import BudgetPolicy

        policy = BudgetPolicy(
            daily_ai_budget_usd=request.daily_ai_budget_usd,
            monthly_ai_budget_usd=request.monthly_ai_budget_usd,
            max_single_request_usd=request.max_single_request_usd,
            approved_models=list(request.approved_models),
            approved_providers=list(request.approved_providers),
        )
        self.runtime.budget_enforcer.policy = policy
        self.runtime.settings.budget = policy
        self.runtime._emit_event("BUDGET_UPDATED", {
            "daily_ai_budget_usd": policy.daily_ai_budget_usd,
            "monthly_ai_budget_usd": policy.monthly_ai_budget_usd,
            "max_single_request_usd": policy.max_single_request_usd,
        })
        
        tracker = self.runtime.cost_tracker
        daily_spent = tracker.daily_spent()
        monthly_spent = tracker.monthly_spent()
        return james_runtime_pb2.BudgetStatus(
            daily_spent=float(daily_spent),
            daily_limit=policy.daily_ai_budget_usd,
            monthly_spent=float(monthly_spent),
            monthly_limit=policy.monthly_ai_budget_usd,
            daily_exceeded=policy.daily_ai_budget_usd > 0 and daily_spent >= policy.daily_ai_budget_usd,
            monthly_exceeded=policy.monthly_ai_budget_usd > 0 and monthly_spent >= policy.monthly_ai_budget_usd,
        )
    
    async def StreamEvents(self, request, context):
        """Stream runtime events"""
        queue: asyncio.Queue = asyncio.Queue(maxsize=1000)
        allowed: set = set(request.event_types) if request.event_types else None

        def handler(event):
            try:
                if allowed and event.event_type not in allowed:
                    return
                queue.put_nowait(event)
            except Exception:
                pass

        self.runtime.add_event_handler(handler)
        try:
            while True:
                try:
                    event = await asyncio.wait_for(queue.get(), timeout=30.0)
                except asyncio.TimeoutError:
                    continue
                payload = {k: (v if isinstance(v, str) else json.dumps(v)) for k, v in event.payload.items()}
                yield james_runtime_pb2.RuntimeEvent(
                    event_type=event.event_type,
                    timestamp=int(event.timestamp.timestamp() * 1000),
                    event_id=getattr(event, "event_id", "") or "",
                    correlation_id=getattr(event, "correlation_id", "") or "",
                    payload=payload,
                    source=getattr(event, "source", "") or "sidecar",
                    severity="info",
                )
        except (asyncio.CancelledError, Exception):
            pass


async def serve_grpc(runtime, port: int = 38243):
    """Start gRPC server"""
    if not GRPC_AVAILABLE:
        logger.warning("gRPC not available, skipping gRPC server")
        return
    
    server = grpc.aio.server()
    servicer = JamesRuntimeServicer(runtime)

    # Add servicer to server
    james_runtime_pb2_grpc.add_JamesRuntimeServicer_to_server(servicer, server)
    
    server.add_insecure_port(f"[::]:{port}")
    await server.start()
    logger.info(f"gRPC server started on port {port}")
    
    await server.wait_for_termination()


async def start_grpc_server(runtime: JamesRuntime, port: int = 38243):
    """Start gRPC server as background task"""
    if not GRPC_AVAILABLE:
        logger.warning("gRPC not available (protobuf not compiled)")
        return None
    
    task = asyncio.create_task(serve_grpc(runtime, port))
    return task