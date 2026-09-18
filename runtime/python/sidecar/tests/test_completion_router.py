import asyncio
from types import SimpleNamespace

from james_runtime.core.requests import CompletionRequest, RoutingRequest, RoutingDecision, EngineType, CompletionResponse
from james_runtime.routing.completion_router import CompletionRouter


class FakeEngine:
    engine_type = "llamacpp"

    def __init__(self, fail=False):
        self.fail = fail

    def supports_model(self, model):
        return True

    async def complete(self, request):
        if self.fail:
            raise RuntimeError("synthetic inference failure")
        return CompletionResponse(
            id="test",
            model=request.model,
            created=1,
            choices=[{"message": {"role": "assistant", "content": "ok"}}],
        )


class FakeRegistry:
    def __init__(self, engine):
        self.engine = engine

    def get_engine_types(self):
        return ["llamacpp"]

    def get(self, engine_type):
        return self.engine if engine_type == "llamacpp" else None


class FakeRuntimeRouter:
    def __init__(self, engine):
        self.engine_registry = FakeRegistry(engine)

    async def select_async(self, model, constraints):
        return SimpleNamespace(
            engine_type=EngineType.LLAMACPP,
            estimated_vram_gb=4.0,
            estimated_latency_ms=100,
        )


class FakeModelRegistry:
    def __init__(self):
        self.models = {
            "primary": SimpleNamespace(id="primary", provider="local"),
            "fallback": SimpleNamespace(id="fallback", provider="local"),
        }

    def get_model_spec(self, model_id):
        return self.models.get(model_id)


class FakeModelRouter:
    def __init__(self):
        self.registry = FakeModelRegistry()

    async def route(self, request):
        return RoutingDecision(
            model_id="primary",
            provider="local",
            reasoning="test",
            fallback_chain=["fallback"],
        )


def test_completion_router_executes_selected_model():
    router = CompletionRouter(FakeModelRouter(), FakeRuntimeRouter(FakeEngine()))
    request = CompletionRequest(model="requested", messages=[{"role": "user", "content": "hello"}])

    response = asyncio.run(router.complete(request))

    assert response.model == "primary"
    assert response.runtime == "llamacpp"
    assert response.routing_decision["fallback_model"] is False


def test_completion_router_falls_back_to_next_model():
    router = CompletionRouter(
        FakeModelRouter(),
        FakeRuntimeRouter(FakeEngine(fail=False)),
    )
    request = CompletionRequest(model="requested", messages=[{"role": "user", "content": "hello"}])

    original_complete = router.runtime_router.engine_registry.engine.complete
    calls = {"count": 0}

    async def fail_first(request):
        calls["count"] += 1
        if calls["count"] == 1:
            raise RuntimeError("primary failed")
        return await original_complete(request)

    router.runtime_router.engine_registry.engine.complete = fail_first

    response = asyncio.run(router.complete(request))

    assert response.model == "fallback"
    assert response.routing_decision["fallback_model"] is True
