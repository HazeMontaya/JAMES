"""Tests for the model and pricing registries."""
from decimal import Decimal
from james_runtime.models.registry import ModelRegistry
from james_runtime.models.pricing import PricingRegistry


def test_default_model_registry():
    registry = ModelRegistry()
    registry._load_default_models()
    assert len(registry.list_available()) >= 7
    model = registry.get_model_spec("llama-3.3-70b-instruct")
    assert model is not None
    assert model.parameters_b == 70
    assert "coding" in model.capabilities
    assert model.max_context == 131072


def test_pricing_defaults():
    pricing = PricingRegistry()
    pricing.load_defaults()
    assert pricing.get_cost("llama-3.1-8b-instruct") == 0.0
    assert pricing.get_cost("gpt-4o") > 0
    assert pricing.calculate_cost("gpt-4o", 1000, 1000) == Decimal("0.020")


def test_model_registry_accepts_canonical_rust_snapshot():
    registry = ModelRegistry()
    registry.replace_from_canonical([
        {
            "id": "canonical-model",
            "provider": "ollama",
            "parameters_b": 7,
            "context_length": 32768,
            "capabilities": ["coding"],
            "quality_tier": "balanced",
            "quality_score": 0.8,
            "path": ".james/models/canonical.gguf",
        }
    ])
    assert registry.is_canonical_snapshot()
    assert registry.get_model_spec("canonical-model").provider == "ollama"
    assert registry.get_model_spec("llama-3.3-70b-instruct") is None
