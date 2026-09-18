"""JAMES Runtime Engine Abstraction Base"""
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Optional, AsyncGenerator, List, Dict, Any
from pydantic import BaseModel
from james_runtime.config import VLLMConfig, LlamaCppConfig, AirLLMConfig


@dataclass
class EngineCapabilities:
    max_context: int
    streaming: bool = True
    tools: bool = False
    batching: bool = False
    structured_output: bool = False
    speculative_decode: bool = False
    quantization_support: List[str] = field(default_factory=list)
    hardware_targets: List[str] = field(default_factory=list)


@dataclass
class VRAMRequirements:
    model_bytes: int
    kv_cache_bytes: int
    overhead_bytes: int
    total_bytes: int
    quantization: Optional[str] = None
    
    @property
    def total_gb(self) -> float:
        return self.total_bytes / (1024 ** 3)


@dataclass
class HealthStatus:
    healthy: bool
    model_loaded: bool
    vram_used_gb: float
    vram_total_gb: float
    latency_p50_ms: float
    latency_p99_ms: float
    error_rate: float


@dataclass
class EngineConfig:
    model_spec: Any  # ModelSpec from models.registry
    vllm: Any = None
    llamacpp: Any = None
    airllm: Any = None


class InferenceEngine(ABC):
    """Abstract base class for inference engines"""
    
    @property
    @abstractmethod
    def engine_type(self) -> str:
        """Unique engine identifier: 'vllm', 'llamacpp', 'airllm'"""
        pass
    
    @property
    @abstractmethod
    def capabilities(self) -> EngineCapabilities:
        """Engine capabilities"""
        pass
    
    @abstractmethod
    async def initialize(self, config: EngineConfig) -> None:
        """Initialize engine with configuration"""
        pass
    
    @abstractmethod
    async def complete(self, request: Any) -> Any:
        """Non-streaming completion"""
        pass
    
    @abstractmethod
    async def stream(self, request: Any) -> AsyncGenerator[Any, None]:
        """Streaming completion"""
        pass
    
    @abstractmethod
    async def health(self) -> HealthStatus:
        """Health check"""
        pass
    
    @abstractmethod
    def estimate_vram(self, model_spec: Any) -> 'VRAMRequirements':
        """Estimate VRAM requirements for a model"""
        pass
    
    @abstractmethod
    def supports_model(self, model_spec: Any) -> bool:
        """Check if engine can run this model"""
        pass
    
    @abstractmethod
    async def shutdown(self) -> None:
        """Graceful shutdown"""
        pass


# Forward reference for VRAMRequirements
VRAMRequirements = VRAMRequirements