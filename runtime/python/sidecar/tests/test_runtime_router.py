import pytest

from james_runtime.routing.runtime_router import RuntimeRouter
from james_runtime.core.requests import EngineType, RoutingConstraints


class FakeHealth:
    def __init__(self, healthy):
        self.healthy = healthy


class FakeEngine:
    def supports_model(self, model):
        return True


class FakeRegistry:
    def __init__(self):
        self.engines = {e.value: FakeEngine() for e in EngineType}

    def has_engine(self, name):
        return name in self.engines

    def get(self, name):
        return self.engines.get(name)

    async def check_all_health(self):
        return {
            EngineType.VLLM.value: FakeHealth(False),
            EngineType.LLAMACPP.value: FakeHealth(True),
            EngineType.AIRLLM.value: FakeHealth(True),
        }


class FakeFit:
    def __init__(self, engine):
        self.fits = True
        self.engine = engine
        self.quantization = None
        self.offload = False
        self.required_vram_gb = 4.0
        self.estimated_latency_ms = 100


class FakeVRAM:
    available_vram_gb = 11.0

    def can_fit(self, model, engine):
        return FakeFit(engine)


def test_health_aware_async_selection_prefers_healthy_engine():
    router = RuntimeRouter(FakeRegistry(), None, FakeVRAM())
    model = object()

    import asyncio
    selection = asyncio.run(router.select_async(model))

    assert selection.engine_type == EngineType.LLAMACPP


def test_preferred_engine_is_enforced():
    router = RuntimeRouter(FakeRegistry(), None, FakeVRAM())

    with pytest.raises(RuntimeError):
        router.select(
            object(),
            RoutingConstraints(preferred_engine=EngineType.VLLM.value),
            health={EngineType.VLLM.value: FakeHealth(False)},
        )


def test_vram_constraint_is_enforced():
    router = RuntimeRouter(FakeRegistry(), None, FakeVRAM())

    with pytest.raises(RuntimeError):
        router.select(
            object(),
            RoutingConstraints(max_vram_gb=2.0),
            health={e.value: FakeHealth(True) for e in EngineType},
        )
