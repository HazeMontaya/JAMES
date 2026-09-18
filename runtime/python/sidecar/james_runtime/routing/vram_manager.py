"""JAMES Runtime VRAM Management"""
import logging
from dataclasses import dataclass
from typing import Optional, List
from james_runtime.routing.hardware import HardwareProfile
from james_runtime.abstraction.base import VRAMRequirements, EngineCapabilities
from james_runtime.core.requests import EngineType, RoutingConstraints
from james_runtime.models.registry import ModelSpec

logger = logging.getLogger(__name__)


# Quantization ladder for each engine (best to worst quality)
QUANTIZATION_LADDER = {
    EngineType.VLLM: ["fp16", "bf16", "fp8", "awq_int4", "gptq_int4", "bitsandbytes_int4"],
    EngineType.LLAMACPP: ["fp16", "q8_0", "q5_k_m", "q4_k_m", "q3_k_m", "q2_k"],
    EngineType.AIRLLM: ["fp16", "bf16"],  # AirLLM doesn't quantize model weights
}

# Quantization factor (multiplier for model size)
QUANT_FACTOR = {
    "fp16": 1.0,
    "bf16": 1.0,
    "fp8": 0.5,
    "awq_int4": 0.25,
    "gptq_int4": 0.25,
    "bitsandbytes_int4": 0.25,
    "q8_0": 0.5,
    "q5_k_m": 0.35,
    "q4_k_m": 0.28,
    "q3_k_m": 0.22,
    "q2_k": 0.18,
    "fp16": 1.0,
    "bf16": 1.0,
}


@dataclass
class FitResult:
    fits: bool
    engine: EngineType
    quantization: Optional[str] = None
    offload: bool = False
    required_vram_gb: float = 0.0
    available_vram_gb: float = 0.0
    reason: Optional[str] = None
    estimated_latency_ms: int = 0


class VRAMManager:
    """Manages VRAM allocation and quantization decisions"""
    
    def __init__(self, hardware: HardwareProfile, headroom: float = 0.1):
        self.hardware = hardware
        self.headroom = headroom  # 10% headroom by default
        self._allocated_vram_gb = 0.0
    
    @property
    def available_vram_gb(self) -> float:
        return max(0, self.hardware.vram_gb * (1 - self.headroom) - self._allocated_vram_gb)
    
    @property
    def total_vram_gb(self) -> float:
        return self.hardware.vram_gb
    
    def can_fit(self, model_spec, engine_type: EngineType, constraints: Optional[RoutingConstraints] = None) -> FitResult:
        """Check if model fits in VRAM with given engine"""
        engine = self._get_engine_instance(engine_type)
        if not engine:
            return FitResult(
                fits=False,
                engine=engine_type,
                reason="Engine not available",
            )
        
        required = engine.estimate_vram(model_spec)
        required_gb = required.total_bytes / (1024**3)
        available_gb = self.available_vram_gb
        
        if required_gb <= available_gb:
            return FitResult(
                fits=True,
                engine=engine_type,
                quantization=None,
                required_vram_gb=required_gb,
                available_vram_gb=available_gb,
                estimated_latency_ms=self._estimate_latency(engine_type, model_spec, None),
            )
        
        # Try quantization ladder
        for quant in self._get_quantization_ladder(engine_type):
            if not self._engine_supports_quantization(engine_type, quant):
                continue
            
            quant_factor = QUANT_FACTOR.get(quant, 1.0)
            quant_required_gb = required_gb * quant_factor
            
            if quant_required_gb <= available_gb:
                return FitResult(
                    fits=True,
                    engine=engine_type,
                    quantization=quant,
                    required_vram_gb=quant_required_gb,
                    available_vram_gb=available_gb,
                    estimated_latency_ms=self._estimate_latency(engine_type, model_spec, quant),
                )
        
        # Try AirLLM offload for large models
        if engine_type != EngineType.AIRLLM:
            from james_runtime.abstraction.airllm_engine import AirLLMEngine
            if AirLLMEngine.supports_model(model_spec):
                return FitResult(
                    fits=True,
                    engine=EngineType.AIRLLM,
                    offload=True,
                    required_vram_gb=0.0,  # AirLLM uses minimal VRAM
                    available_vram_gb=available_gb,
                    reason="Using AirLLM layer offloading",
                    estimated_latency_ms=self._estimate_latency(EngineType.AIRLLM, model_spec, "fp16"),
                )
        
        return FitResult(
            fits=False,
            engine=engine_type,
            required_vram_gb=required_gb,
            available_vram_gb=available_gb,
            reason=f"Insufficient VRAM: need {required_gb:.1f}GB, have {available_gb:.1f}GB",
        )
    
    def _get_engine_instance(self, engine_type: EngineType):
        # Import here to avoid circular imports
        if engine_type == EngineType.VLLM:
            from james_runtime.abstraction.vllm_engine import VLLMEngine
            return VLLMEngine(None)  # Config not needed for estimation
        elif engine_type == EngineType.LLAMACPP:
            from james_runtime.abstraction.llamacpp_engine import LlamaCppEngine
            return LlamaCppEngine(None)
        elif engine_type == EngineType.AIRLLM:
            from james_runtime.abstraction.airllm_engine import AirLLMEngine
            return AirLLMEngine(None)
        return None
    
    def _get_quantization_ladder(self, engine_type: EngineType) -> List[str]:
        return QUANTIZATION_LADDER.get(engine_type, [])
    
    def _engine_supports_quantization(self, engine_type: EngineType, quant: str) -> bool:
        # Check if quantization is in supported list
        capabilities = self._get_engine_capabilities(engine_type)
        return quant in capabilities.quantization_support
    
    def _get_engine_capabilities(self, engine_type: EngineType):
        if engine_type == EngineType.VLLM:
            return EngineCapabilities(quantization_support=["fp16", "bf16", "fp8", "awq", "gptq", "bitsandbytes_int4"])
        elif engine_type == EngineType.LLAMACPP:
            return EngineCapabilities(quantization_support=["fp16", "q8_0", "q5_k_m", "q4_k_m", "q3_k_m", "q2_k"])
        elif engine_type == EngineType.AIRLLM:
            return EngineCapabilities(quantization_support=["fp16", "bf16"])
        return EngineCapabilities(quantization_support=[])
    
    def _estimate_latency(self, engine_type: EngineType, model_spec, quantization: Optional[str]) -> int:
        """Rough latency estimation in ms"""
        params_b = getattr(model_spec, 'parameters_b', 7)
        
        base_latency = {
            EngineType.VLLM: 50,
            EngineType.LLAMACPP: 100,
            EngineType.AIRLLM: 1000,  # Much slower due to layer offloading
        }.get(engine_type, 100)
        
        # Scale by model size
        size_factor = params_b / 7.0
        
        # Quantization impact
        quant_factor = 1.0
        if quantization:
            quant_factors = {
                "fp16": 1.0, "bf16": 1.0, "fp8": 1.2,
                "awq_int4": 1.5, "gptq_int4": 1.5, "bitsandbytes_int4": 1.5,
                "q8_0": 1.1, "q5_k_m": 1.2, "q4_k_m": 1.3, "q3_k_m": 1.5, "q2_k": 2.0,
            }
            quant_factor = quant_factors.get(quantization, 1.0)
        
        return int(base_latency * size_factor * quant_factor)
    
    def allocate(self, vram_gb: float) -> bool:
        """Reserve VRAM for a model"""
        if self._allocated_vram_gb + vram_gb <= self.available_vram_gb:
            self._allocated_vram_gb += vram_gb
            return True
        return False
    
    def deallocate(self, vram_gb: float) -> None:
        self._allocated_vram_gb = max(0, self._allocated_vram_gb - vram_gb)
    
    def get_usage_stats(self) -> dict:
        return {
            "total_vram_gb": self.hardware.vram_gb,
            "available_vram_gb": self.available_vram_gb,
            "allocated_vram_gb": self._allocated_vram_gb,
            "headroom_gb": self.hardware.vram_gb * self.headroom,
            "utilization_percent": (self._allocated_vram_gb / self.hardware.vram_gb) * 100 if self.hardware.vram_gb > 0 else 0,
        }