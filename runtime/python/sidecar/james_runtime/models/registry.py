"""Compatibility model catalog for the Python sidecar.

The registry is intentionally dependency-free.  It describes models; engines remain
responsible for actually loading them.  This keeps routing usable before any local
model is downloaded and makes the registry safe to extend from discovery later.
"""
from dataclasses import dataclass, field
from typing import Dict, List, Optional

from pydantic import BaseModel, Field


class CanonicalModel(BaseModel):
    """Typed projection of the Rust ModelsModule ModelInfo contract."""

    id: str
    name: str = ""
    provider: str
    model_type: str = "LLM"
    capabilities: list[str] = Field(default_factory=list)
    context_length: int = 0
    parameters: Optional[str] = None
    quantization: Optional[str] = None
    size_bytes: Optional[int] = None
    path: Optional[str] = None
    endpoint: Optional[str] = None
    api_key_required: bool = False
    cost_per_1k_input: Optional[float] = None
    cost_per_1k_output: Optional[float] = None
    metadata: dict = Field(default_factory=dict)


class ModelSnapshot(BaseModel):
    """Versioned typed snapshot emitted by the canonical Rust model registry."""

    schema_version: int
    models: list[CanonicalModel] = Field(default_factory=list)


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

    def replace_from_canonical(self, snapshot: ModelSnapshot) -> None:
        """Replace the sidecar cache from a validated canonical Rust snapshot."""
        if snapshot.schema_version != 1:
            raise ValueError(f"unsupported model snapshot schema: {snapshot.schema_version}")
        canonical: Dict[str, ModelSpec] = {}
        for raw in snapshot.models:
            model_id = raw.id.strip()
            if not model_id:
                continue
            parameters_b = 0.0
            if raw.parameters:
                try:
                    parameters_b = float(raw.parameters.rstrip("Bb"))
                except ValueError:
                    parameters_b = 0.0
            canonical[model_id] = ModelSpec(
                id=model_id,
                provider=raw.provider,
                parameters_b=parameters_b,
                max_context=raw.context_length,
                capabilities=[cap.lower() for cap in raw.capabilities],
                quality_tier="balanced",
                quality_score=0.5,
                local_path=raw.path,
                format=raw.quantization,
                enabled=True,
            )
        self._models = canonical
        self._snapshot_source = "rust"

    def replace_from_canonical_payload(self, payload: object) -> None:
        """Validate an untrusted transport payload at the bridge boundary."""
        self.replace_from_canonical(ModelSnapshot.model_validate(payload))

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
