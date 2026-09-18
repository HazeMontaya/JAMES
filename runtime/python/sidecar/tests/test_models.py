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
