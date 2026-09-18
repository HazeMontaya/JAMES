"""Regression tests for the local-first runtime configuration."""
from pathlib import Path

from james_runtime.config import LlamaCppConfig, get_settings
from james_runtime.models.registry import ModelRegistry


def test_llamacpp_defaults_match_bootstrap_model():
    config = LlamaCppConfig()
    assert config.model_path.endswith("Qwen2.5-3B-Instruct-Q4_K_M.gguf")
    assert config.port == 8080
    assert config.parallel == 1
    assert config.auto_start is True


def test_runtime_yaml_loads():
    settings = get_settings()
    assert settings.http_port == 38242
    assert settings.llamacpp.model_path.endswith("Qwen2.5-3B-Instruct-Q4_K_M.gguf")


def test_bootstrap_model_is_registered():
    registry = ModelRegistry()
    registry.initialize()
    model = registry.get_model_spec("qwen2.5-3b-instruct-q4")
    assert model is not None
    assert model.provider == "local"
    assert model.format == "gguf"
    assert model.local_path
