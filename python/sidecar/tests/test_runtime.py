"""JAMES Runtime Tests"""
import pytest
import asyncio
from james_runtime.config import RuntimeSettings, VLLMConfig, LlamaCppConfig, AirLLMConfig, RouterConfig, BudgetPolicy
from james_runtime.core.requests import CompletionRequest, RoutingRequest, EngineType, EngineSelection
from james_runtime.routing.vram_manager import VRAMManager, FitResult, EngineType
from james_runtime.routing.hardware import HardwareProfile, HardwareDetector
from james_runtime.models.registry import ModelRegistry, ModelSpec
from james_runtime.models.pricing import PricingRegistry, ModelPricing
from decimal import Decimal


def test_settings_defaults():
    settings = RuntimeSettings()
    assert settings.http_port == 38242
    assert settings.grpc_port == 38243
    assert settings.vllm.tp_size == 1
    assert settings.llamacpp.n_gpu_layers == -1


def test_routing_request_creation():
    req = RoutingRequest(task="coding")
    assert req.task == "coding"
    assert req.quality_tier == "balanced"
    assert req.privacy == "prefer_local"


def test_hardware_profile():
    profile = HardwareProfile(
        gpu_name="RTX 3090",
        vram_gb=24.0,
        cuda_version="12.1",
        cpu_cores=16,
        ram_gb=64.0,
    )
    assert profile.vram_gb == 24.0
    assert profile.gpu_name == "RTX 3090"


def test_model_registry():
    registry = ModelRegistry()
    registry._load_default_models()
    
    models = registry.list_available()
    assert len(models) >= 7
    
    llama_70b = registry.get_model_spec("llama-3.3-70b-instruct")
    assert llama_70b is not None
    assert llama_70b.parameters_b == 70
    assert "coding" in llama_70b.capabilities


def test_pricing_registry():
    pricing = PricingRegistry()
    
    # Local models should be free
    cost = pricing.get_cost("llama-3.3-70b-instruct")
    assert cost == 0.0
    
    # Cloud models should have cost
    cost = pricing.get_cost("gpt-4o")
    assert cost is not None
    assert cost > 0


def test_vram_manager():
    hardware = HardwareProfile(
        gpu_name="RTX 3090",
        vram_gb=24.0,
        cpu_cores=16,
        ram_gb=64.0,
    )
    vram_manager = VRAMManager(hardware, headroom=0.1)
    
    assert vram_manager.available_vram_gb == pytest.approx(21.6, rel=0.1)
    assert vram_manager.total_vram_gb == 24.0


def test_engine_type():
    assert EngineType.VLLM.value == "vllm"
    assert EngineType.LLAMACPP.value == "llamacpp"
    assert EngineType.AIRLLM.value == "airllm"


def test_completion_request():
    req = CompletionRequest(
        model="llama-3.1-8b-instruct",
        messages=[
            {"role": "system", "content": "You are JAMES"},
            {"role": "user", "content": "Hello"},
        ],
        temperature=0.7,
        max_tokens=512,
    )
    assert req.model == "llama-3.1-8b-instruct"
    assert len(req.messages) == 2


def test_routing_decision():
    selection = EngineSelection(
        engine_type="vllm",
        model_id="llama-3.1-8b-instruct",
        quantization="fp16",
        offload=False,
        estimated_vram_gb=16.0,
        estimated_latency_ms=100,
    )
    assert selection.engine_type == "vllm"
    assert selection.estimated_vram_gb == 16.0


if __name__ == "__main__":
    pytest.main([__file__, "-v"])