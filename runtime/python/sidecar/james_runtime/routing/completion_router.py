"""End-to-end model -> runtime -> inference routing for JAMES.

This module is the execution boundary between logical model routing and concrete
inference engines. It deliberately keeps provider/model selection separate from
engine selection so either layer can fail over independently.
"""
import logging
from typing import Optional

from james_runtime.core.requests import (
    CompletionRequest,
    CompletionResponse,
    RoutingConstraints,
    RoutingRequest,
)
from james_runtime.routing.model_router import ModelRouter
from james_runtime.routing.runtime_router import RuntimeRouter

logger = logging.getLogger(__name__)


class CompletionRouter:
    """Resolve a task to a model, select a healthy engine, and execute it."""

    def __init__(
        self,
        model_router: ModelRouter,
        runtime_router: RuntimeRouter,
    ):
        self.model_router = model_router
        self.runtime_router = runtime_router

    async def complete(
        self,
        request: CompletionRequest,
        *,
        routing: Optional[RoutingRequest] = None,
    ) -> CompletionResponse:
        """Route and execute a completion with model and engine fallbacks."""
        routing = routing or RoutingRequest(task="completion", preferred_model=request.model)
        decision = await self.model_router.route(routing)

        model_ids = [decision.model_id, *decision.fallback_chain]
        last_error: Optional[Exception] = None

        for model_id in model_ids:
            model = self.model_router.registry.get_model_spec(model_id)
            if model is None:
                continue

            constraints = RoutingConstraints(
                max_latency_ms=routing.latency_budget_ms,
                max_cost_per_1k=routing.cost_budget_per_1k,
                allow_cloud=routing.privacy != "local_only",
            )

            engine_types = self._engine_order()
            for engine_type in engine_types:
                try:
                    selection = await self.runtime_router.select_async(
                        model,
                        constraints.model_copy(update={"preferred_engine": engine_type}),
                    )
                except RuntimeError as exc:
                    last_error = exc
                    continue

                engine = self.runtime_router.engine_registry.get(selection.engine_type.value)
                if engine is None:
                    continue

                try:
                    execution_request = request.model_copy(update={"model": model.id})
                    response = await engine.complete(execution_request)
                    response.runtime = selection.engine_type.value
                    response.routing_decision = {
                        "model_id": model.id,
                        "provider": model.provider,
                        "engine": selection.engine_type.value,
                        "fallback_model": model.id != decision.model_id,
                        "estimated_vram_gb": selection.estimated_vram_gb,
                        "estimated_latency_ms": selection.estimated_latency_ms,
                    }
                    return response
                except Exception as exc:
                    last_error = exc
                    logger.warning(
                        "Inference failed for model=%s engine=%s; trying next fallback: %s",
                        model.id,
                        selection.engine_type.value,
                        exc,
                    )

        raise RuntimeError("All configured model/runtime fallbacks failed") from last_error

    def _engine_order(self) -> list[str]:
        """Return registered engines in runtime preference order."""
        preferred = ["vllm", "llamacpp", "airllm"]
        registered = set(self.runtime_router.engine_registry.get_engine_types())
        return [engine for engine in preferred if engine in registered]
