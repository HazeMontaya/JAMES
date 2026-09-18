"""Model Router for JAMES - Routes to different LLM backends"""

from typing import Any

from ..config import JamesConfig
from .base import ModelBackend, ModelConfig, ModelProvider, ModelResponse
from .http_backends import OpenAICompatibleBackend

__all__ = [
    "AnthropicBackend",
    "LlamaCppBackend",
    "ModelBackend",
    "ModelConfig",
    "ModelProvider",
    "ModelResponse",
    "ModelRouter",
    "OllamaBackend",
    "OpenAIBackend",
    "VLLMBackend",
    "build_model_router",
]


class VLLMBackend(OpenAICompatibleBackend):
    pass


class LlamaCppBackend(OpenAICompatibleBackend):
    pass


class OpenAIBackend(OpenAICompatibleBackend):
    pass


class AnthropicBackend(OpenAICompatibleBackend):
    """Anthropic uses /v1/messages; override generation."""

    async def generate(self, prompt: str, **kwargs: Any) -> ModelResponse:
        import time

        if not self._base_url and not self.config.api_key:
            raise RuntimeError("No base_url or api_key configured for anthropic backend")

        temperature = kwargs.get("temperature", self.config.temperature)
        max_tokens = kwargs.get("max_tokens", self.config.max_tokens)
        start = time.monotonic()

        payload = {
            "model": self.config.model_name,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": temperature,
            "max_tokens": max_tokens,
        }
        headers = {"anthropic-version": "2023-06-01"}
        if self.config.api_key:
            headers["x-api-key"] = self.config.api_key
        url = (self._base_url or "https://api.anthropic.com") + "/v1/messages"

        resp = await self._http.post(url, json=payload, headers=headers)
        resp.raise_for_status()
        data = resp.json()
        content = "".join(b.get("text", "") for b in data.get("content", []) if b.get("type") == "text")

        return ModelResponse(
            content=content,
            model=self.config.model_name,
            provider=self.config.provider,
            tokens_used=data.get("usage", {}).get("input_tokens", 0) + data.get("usage", {}).get("output_tokens", 0),
            latency_ms=(time.monotonic() - start) * 1000,
        )


class OllamaBackend(OpenAICompatibleBackend):
    pass


class ModelRouter:
    def __init__(self, configs: list[ModelConfig]):
        self._backends: dict[ModelProvider, ModelBackend] = {}
        self._configs = {c.provider: c for c in configs}
        self._initialize_backends()

    def _initialize_backends(self) -> None:
        for provider, config in self._configs.items():
            if provider == ModelProvider.VLLM:
                self._backends[provider] = VLLMBackend(config)
            elif provider == ModelProvider.LLAMA_CPP:
                self._backends[provider] = LlamaCppBackend(config)
            elif provider == ModelProvider.OPENAI:
                self._backends[provider] = OpenAIBackend(config)
            elif provider == ModelProvider.ANTHROPIC:
                self._backends[provider] = AnthropicBackend(config)
            elif provider == ModelProvider.OLLAMA:
                self._backends[provider] = OllamaBackend(config)

    def get_backend(self, provider: ModelProvider) -> ModelBackend | None:
        return self._backends.get(provider)

    def get_healthy_backends(self) -> list[ModelBackend]:
        return [b for b in self._backends.values() if b.healthy]

    async def generate(
        self,
        prompt: str,
        preferred_provider: ModelProvider | None = None,
        fallback: bool = True,
        **kwargs: Any,
    ) -> ModelResponse:
        if preferred_provider and preferred_provider in self._backends:
            backend = self._backends[preferred_provider]
            if backend.healthy:
                return await backend.generate(prompt, **kwargs)

        for backend in sorted(self._backends.values(), key=lambda b: b.config.priority, reverse=True):
            if backend.healthy:
                try:
                    return await backend.generate(prompt, **kwargs)
                except Exception:
                    if not fallback:
                        raise
                    continue

        raise RuntimeError("No healthy model backends available")

    async def health_check_all(self) -> dict[ModelProvider, bool]:
        results = {}
        for provider, backend in self._backends.items():
            try:
                results[provider] = await backend.health_check()
                backend._healthy = results[provider]
            except Exception:
                results[provider] = False
                backend._healthy = False
        return results


_PROVIDER_MAP = {
    "vllm": ModelProvider.VLLM,
    "llama_cpp": ModelProvider.LLAMA_CPP,
    "ollama": ModelProvider.OLLAMA,
    "openai": ModelProvider.OPENAI,
    "anthropic": ModelProvider.ANTHROPIC,
}


def build_model_router(config: JamesConfig) -> ModelRouter:
    """Build a ModelRouter from a JamesConfig (enabled endpoints only)."""
    configs: list[ModelConfig] = []
    for name, ep in config.enabled_model_endpoints().items():
        provider = _PROVIDER_MAP.get(name)
        if not provider:
            continue
        configs.append(
            ModelConfig(
                provider=provider,
                model_name=ep.model,
                base_url=ep.base_url or None,
                api_key=ep.api_key or None,
                max_tokens=ep.max_tokens,
                temperature=0.7,
                timeout=ep.timeout,
                priority=ep.priority,
            )
        )
    return ModelRouter(configs)
