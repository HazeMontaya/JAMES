"""Tests for capability-aware model routing."""

import pytest

from james_core.cognition.base import ModelConfig, ModelProvider
from james_core.cognition.model_router import ModelRequirements, ModelRouter


def make_router() -> ModelRouter:
    return ModelRouter(
        [
            ModelConfig(
                provider=ModelProvider.OLLAMA,
                model_name="local-small",
                base_url="http://localhost:11434",
                max_tokens=4096,
                timeout=120,
                priority=10,
            ),
            ModelConfig(
                provider=ModelProvider.LLAMA_CPP,
                model_name="local-long",
                base_url="http://localhost:8080",
                max_tokens=32768,
                timeout=30,
                priority=5,
            ),
        ]
    )


def test_select_backend_honors_context_requirement():
    router = make_router()
    selected = router.select_backend(ModelRequirements(min_context_tokens=16000))
    assert selected is not None
    assert selected.config.provider is ModelProvider.LLAMA_CPP


def test_select_backend_honors_explicit_provider():
    router = make_router()
    selected = router.select_backend(preferred_provider=ModelProvider.OLLAMA)
    assert selected is not None
    assert selected.config.provider is ModelProvider.OLLAMA


def test_select_backend_honors_latency_requirement():
    router = make_router()
    selected = router.select_backend(ModelRequirements(max_latency_ms=60_000))
    assert selected is not None
    assert selected.config.provider is ModelProvider.LLAMA_CPP


def test_select_backend_returns_none_when_requirements_cannot_be_met():
    router = make_router()
    selected = router.select_backend(ModelRequirements(min_context_tokens=100_000))
    assert selected is None
