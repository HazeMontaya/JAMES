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
    
    async def select_async(self, model: ModelSpec, constraints: Optional[RoutingConstraints] = None) -> EngineSelection:
        """Select an engine while excluding engines that currently report unhealthy."""
        constraints = constraints or RoutingConstraints()
        health = await self.engine_registry.check_all_health()
        return self.select(model, constraints, health=health)

    def select(self, model: ModelSpec, constraints: Optional[RoutingConstraints] = None, health: Optional[dict] = None) -> EngineSelection:
        """Select the best engine for a model on current hardware
        
        Priority order:
        1. vLLM (best throughput for supported models)
        2. llama.cpp (broadest hardware support)
        3. AirLLM (large model offload when VRAM insufficient)
        """
        constraints = constraints or RoutingConstraints()
        health = health or {}

        def usable(engine_type: EngineType) -> bool:
            if not self.engine_registry.has_engine(engine_type.value):
                return False
            if constraints.preferred_engine and constraints.preferred_engine != engine_type.value:
                return False
            engine = self.engine_registry.get(engine_type.value)
            if engine is None or not engine.supports_model(model):
                return False
            status = health.get(engine_type.value)
            return status is None or getattr(status, "healthy", False)

        # 1. Try vLLM first (highest performance)
        if usable(EngineType.VLLM):
            fit = self.vram.can_fit(model, EngineType.VLLM)
            if fit.fits and self._meets_constraints(fit, constraints):
                return self._create_selection(fit, model)
        
        # 2. Try llama.cpp (broadest hardware support)
        if usable(EngineType.LLAMACPP):
            fit = self.vram.can_fit(model, EngineType.LLAMACPP)
            if fit.fits and self._meets_constraints(fit, constraints):
                return self._create_selection(fit, model)
        
        # 3. Try AirLLM for large models
        if usable(EngineType.AIRLLM):
            fit = self.vram.can_fit(model, EngineType.AIRLLM)
            if fit.fits and self._meets_constraints(fit, constraints):
                return self._create_selection(fit, model)
        
        # No engine can run this model
        raise RuntimeError(
            f"No engine can run {model.id} on current hardware. "
            f"Available VRAM: {self.vram.available_vram_gb:.1f}GB, "
            f"Required: {self._estimate_required_vram(model):.1f}GB"
        )
    
    @staticmethod
    def _meets_constraints(fit: FitResult, constraints: RoutingConstraints) -> bool:
        if constraints.max_vram_gb is not None and fit.required_vram_gb > constraints.max_vram_gb:
            return False
        if constraints.max_latency_ms is not None and fit.estimated_latency_ms > constraints.max_latency_ms:
            return False
        return True

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