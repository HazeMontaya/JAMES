"""JAMES Runtime Runtime Router - Model + Hardware to Engine Selection"""
import logging
from typing import Optional
from james_runtime.core.requests import RuntimeRequest, EngineSelection, EngineType, RoutingConstraints
from james_runtime.routing.vram_manager import VRAMManager, FitResult
from james_runtime.models.registry import ModelSpec
from james_runtime.abstraction.registry import EngineRegistry
from james_runtime.routing.hardware import HardwareProfile

logger = logging.getLogger(__name__)


class RuntimeRouter:
    """Selects the best inference engine for a model on current hardware"""
    
    def __init__(
        self, 
        engine_registry: EngineRegistry, 
        hardware_profile,
        vram_manager: VRAMManager
    ):
        self.engine_registry = engine_registry
        self.hardware = hardware_profile
        self.vram = vram_manager
    
    def select(self, model: ModelSpec, constraints: Optional[RoutingConstraints] = None) -> EngineSelection:
        """Select the best engine for a model on current hardware
        
        Priority order:
        1. vLLM (best throughput for supported models)
        2. llama.cpp (broadest hardware support)
        3. AirLLM (large model offload when VRAM insufficient)
        """
        constraints = constraints or RoutingConstraints()
        
        # 1. Try vLLM first (highest performance)
        if self.engine_registry.has_engine(EngineType.VLLM.value):
            fit = self.vram.can_fit(model, EngineType.VLLM)
            if fit.fits:
                return self._create_selection(fit, model)
        
        # 2. Try llama.cpp (broadest hardware support)
        if self.engine_registry.has_engine(EngineType.LLAMACPP.value):
            fit = self.vram.can_fit(model, EngineType.LLAMACPP)
            if fit.fits:
                return self._create_selection(fit, model)
        
        # 3. Try AirLLM for large models
        if self.engine_registry.has_engine(EngineType.AIRLLM.value):
            fit = self.vram.can_fit(model, EngineType.AIRLLM)
            if fit.fits:
                return self._create_selection(fit, model)
        
        # No engine can run this model
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
        """Rough VRAM estimate for error messages"""
        params_b = getattr(model, 'parameters_b', 7)
        return params_b * 2  # Rough: 2GB per billion parameters for fp16
    
    def get_all_options(self, model: ModelSpec) -> list:
        """Get all viable engine options for a model"""
        options = []
        
        for engine_type in [EngineType.VLLM, EngineType.LLAMACPP, EngineType.AIRLLM]:
            if not self.engine_registry.has_engine(engine_type.value):
                continue
            
            fit = self.vram.can_fit(model, engine_type)
            if fit.fits:
                options.append(self._create_selection(fit, model))
        
        return options
    
    def get_recommended_config(self, model) -> dict:
        """Get recommended engine configuration for a model"""
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