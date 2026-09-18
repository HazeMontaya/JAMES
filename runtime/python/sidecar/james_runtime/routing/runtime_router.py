"""JAMES Runtime Router - Model + Hardware to Engine Selection"""
import logging
from typing import Optional

from james_runtime.core.requests import EngineSelection, EngineType, RoutingConstraints
from james_runtime.routing.vram_manager import VRAMManager, FitResult
from james_runtime.models.registry import ModelSpec
from james_runtime.abstraction.registry import EngineRegistry

logger = logging.getLogger(__name__)


class RuntimeRouter:
    """Select the best inference engine for a model on current hardware."""

    def __init__(self, engine_registry: EngineRegistry, hardware_profile, vram_manager: VRAMManager):
        self.engine_registry = engine_registry
        self.hardware = hardware_profile
        self.vram = vram_manager

    def select(self, model: ModelSpec, constraints: Optional[RoutingConstraints] = None) -> EngineSelection:
        """Prefer llama.cpp for the small-VRAM Windows deployment target."""
        constraints = constraints or RoutingConstraints()

        if self.engine_registry.has_engine(EngineType.LLAMACPP.value):
            fit = self.vram.can_fit(model, EngineType.LLAMACPP)
            if fit.fits:
                return self._create_selection(fit, model)

        if self.engine_registry.has_engine(EngineType.VLLM.value):
            fit = self.vram.can_fit(model, EngineType.VLLM)
            if fit.fits:
                return self._create_selection(fit, model)

        if self.engine_registry.has_engine(EngineType.AIRLLM.value):
            fit = self.vram.can_fit(model, EngineType.AIRLLM)
            if fit.fits:
                return self._create_selection(fit, model)

        raise RuntimeError(
            f"No engine can run {model.id} on current hardware. "
            f"Available VRAM: {self.vram.available_vram_gb:.1f}GB, "
            f"Required: {self._estimate_required_vram(model):.1f}GB"
        )

    def _create_selection(self, fit: FitResult, model) -> EngineSelection:
        return EngineSelection(
            engine_type=fit.engine,
            model_id=model.id,
            quantization=fit.quantization,
            offload=fit.offload,
            estimated_vram_gb=fit.required_vram_gb,
            estimated_latency_ms=fit.estimated_latency_ms,
            fallback_engines=[e for e in EngineType if e != fit.engine],
        )

    def _estimate_required_vram(self, model) -> float:
        return getattr(model, "parameters_b", 7) * 2

    def get_all_options(self, model: ModelSpec) -> list:
        options = []
        for engine_type in [EngineType.LLAMACPP, EngineType.VLLM, EngineType.AIRLLM]:
            if not self.engine_registry.has_engine(engine_type.value):
                continue
            fit = self.vram.can_fit(model, engine_type)
            if fit.fits:
                options.append(self._create_selection(fit, model))
        return options

    def get_recommended_config(self, model) -> dict:
        selection = self.select(model)
        return {
            "engine": selection.engine_type.value,
            "model_id": selection.model_id,
            "quantization": selection.quantization,
            "offload": selection.offload,
            "estimated_vram_gb": selection.estimated_vram_gb,
            "estimated_latency_ms": selection.estimated_latency_ms,
            "fallbacks": [e.value for e in selection.fallback_engines],
        }
