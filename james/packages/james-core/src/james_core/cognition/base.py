"""Base model backend types for JAMES ModelRouter"""

from collections.abc import AsyncGenerator
from dataclasses import dataclass
from enum import Enum
from typing import Any


class ModelProvider(Enum):
    VLLM = "vllm"
    LLAMA_CPP = "llama_cpp"
    OPENAI = "openai"
    ANTHROPIC = "anthropic"
    OLLAMA = "ollama"


@dataclass
class ModelConfig:
    provider: ModelProvider
    model_name: str
    base_url: str | None = None
    api_key: str | None = None
    max_tokens: int = 4096
    temperature: float = 0.7
    timeout: float = 120.0
    priority: int = 0
    health_check_interval: int = 60


@dataclass
class ModelResponse:
    content: str
    model: str
    provider: ModelProvider
    tokens_used: int = 0
    latency_ms: float = 0.0
    metadata: dict[str, Any] | None = None


class ModelBackend:
    def __init__(self, config: ModelConfig):
        self.config = config
        self._healthy = True
        self._last_health_check = 0.0

    async def generate(self, prompt: str, **kwargs: Any) -> ModelResponse:
        raise NotImplementedError

    async def stream(self, prompt: str, **kwargs: Any) -> AsyncGenerator[str, None]:
        raise NotImplementedError

    async def health_check(self) -> bool:
        raise NotImplementedError

    @property
    def healthy(self) -> bool:
        return self._healthy

    async def close(self) -> None:
        pass
