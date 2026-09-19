"""Compatibility model catalog for the Python sidecar.

The registry is intentionally dependency-free.  It describes models; engines remain
responsible for actually loading them.  This keeps routing usable before any local
model is downloaded and makes the registry safe to extend from discovery later.
"""
from dataclasses import dataclass, field
from typing import Dict, List, Optional


@dataclass(frozen=True)
class ModelSpec:
    id: str
    provider: str
    parameters_b: float
    max_context: int
    capabilities: List[str] = field(default_factory=list)
    quality_tier: str = "balanced"
    quality_score: float = 0.5
    local_path: Optional[str] = None
    format: Optional[str] = None
    enabled: bool = True


class ModelRegistry:
    """Sidecar model cache; canonical Rust metadata wins when synchronized."""

    def __init__(self):
        self._models: Dict[str, ModelSpec] = {}
        self._snapshot_source = ""

    def initialize(self) -> None:
        if not self._models:
            self._load_default_models()

    def _load_default_models(self) -> None:
        defaults = [
            ModelSpec("llama-3.1-8b-instruct", "local", 8, 8192,
                      ["conversation", "coding", "reasoning", "planning", "tool_use", "analysis"],
                      "balanced", 0.78),
            ModelSpec("llama-3.1-8b-instruct-q4", "local", 8, 8192,
                      ["conversation", "coding", "reasoning", "planning", "tool_use", "analysis"],
                      "fast", 0.74),
            ModelSpec("qwen2.5-3b-instruct-q4", "local", 3, 32768, ["conversation", "coding", "reasoning", "summarization", "translation", "analysis"], "fast", 0.72, local_path=".james/models/Qwen2.5-3B-Instruct-Q4_K_M.gguf", format="gguf"),
            ModelSpec("llama-3.2-3b-instruct", "local", 3, 8192,
                      ["conversation", "coding", "reasoning", "summarization", "translation"],
                      "fast", 0.68),
            ModelSpec("qwen2.5-coder-7b-instruct", "local", 7, 32768,
                      ["conversation", "coding", "reasoning", "analysis", "planning", "tool_use"],
                      "best", 0.84),
            ModelSpec("qwen2.5-3b-instruct", "local", 3, 32768,
                      ["conversation", "coding", "reasoning", "translation", "summarization"],
                      "fast", 0.70),
            ModelSpec("mistral-7b-instruct", "local", 7, 32768,
                      ["conversation", "reasoning", "analysis", "summarization", "translation"],
                      "balanced", 0.76),
            ModelSpec("llama-3.3-70b-instruct", "local", 70, 131072,
                      ["conversation", "coding", "reasoning", "planning", "tool_use", "analysis"],
                      "best", 0.93),
            ModelSpec("deepseek-r1-distill-qwen-7b", "local", 7, 32768,
                      ["conversation", "coding", "reasoning", "math", "analysis"],
                      "best", 0.86),
        ]
        for model in defaults:
            self.register(model)
        self._snapshot_source = "fallback"

    def replace_from_canonical(self, models: List[dict]) -> None:
        """Replace the sidecar cache from a canonical Rust model snapshot."""
        canonical: Dict[str, ModelSpec] = {}
        for raw in models:
            model_id = str(raw.get("id", "")).strip()
            if not model_id:
                continue
            canonical[model_id] = ModelSpec(
                id=model_id,
                provider=str(raw.get("provider", "unknown")),
                parameters_b=float(raw.get("parameters_b", 0) or 0),
                max_context=int(raw.get("max_context", raw.get("context_length", 0)) or 0),
                capabilities=list(raw.get("capabilities", [])),
                quality_tier=str(raw.get("quality_tier", "balanced")),
                quality_score=float(raw.get("quality_score", 0.5) or 0.5),
                local_path=raw.get("local_path", raw.get("path")),
                format=raw.get("format"),
                enabled=bool(raw.get("enabled", True)),
            )
        self._models = canonical
        self._snapshot_source = "rust"

    def is_canonical_snapshot(self) -> bool:
        return bool(self._models) and self._snapshot_source == "rust"

    def register(self, model: ModelSpec) -> None:
        if not model.id:
            raise ValueError("Model id cannot be empty")
        self._models[model.id] = model

    def get_model_spec(self, model_id: str) -> Optional[ModelSpec]:
        return self._models.get(model_id)

    def list_available(self) -> List[ModelSpec]:
        return [m for m in self._models.values() if m.enabled]

    def list_models(self) -> List[dict]:
        return [
            {
                "id": m.id, "provider": m.provider, "parameters_b": m.parameters_b,
                "max_context": m.max_context, "capabilities": list(m.capabilities),
                "quality_tier": m.quality_tier, "quality_score": m.quality_score,
                "local_path": m.local_path, "format": m.format, "enabled": m.enabled,
            }
            for m in self.list_available()
        ]

    def get_model_info(self, model_id: str) -> dict:
        model = self.get_model_spec(model_id)
        if model is None:
            raise KeyError(f"Unknown model: {model_id}")
        return {
            "id": model.id, "provider": model.provider,
            "parameters_b": model.parameters_b, "max_context": model.max_context,
            "capabilities": list(model.capabilities),
            "quality_tier": model.quality_tier, "quality_score": model.quality_score,
            "local_path": model.local_path, "format": model.format,
            "enabled": model.enabled,
        }
