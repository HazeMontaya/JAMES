"""HTTP model backends for JAMES ModelRouter"""

import time
from typing import Any

import httpx

from .base import ModelBackend, ModelConfig, ModelProvider, ModelResponse


class OpenAICompatibleBackend(ModelBackend):
    """Generic backend for OpenAI-compatible endpoints (vLLM, llama.cpp, Ollama, OpenAI)."""

    def __init__(self, config: ModelConfig):
        super().__init__(config)
        self._http = httpx.AsyncClient(timeout=config.timeout)
        self._base_url = config.base_url.rstrip("/") if config.base_url else None

    async def generate(self, prompt: str, **kwargs: Any) -> ModelResponse:
        if not self._base_url:
            raise RuntimeError(f"No base_url configured for {self.config.provider.value} backend")

        temperature = kwargs.get("temperature", self.config.temperature)
        max_tokens = kwargs.get("max_tokens", self.config.max_tokens)

        start = time.monotonic()
        headers = {}
        if self.config.api_key:
            headers["Authorization"] = f"Bearer {self.config.api_key}"

        if self.config.provider == ModelProvider.OLLAMA:
            payload = {"model": self.config.model_name, "prompt": prompt, "stream": False}
            url = f"{self._base_url}/api/generate"
        else:
            payload = {
                "model": self.config.model_name,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": temperature,
                "max_tokens": max_tokens,
            }
            url = f"{self._base_url}/v1/completions"

        try:
            resp = await self._http.post(url, json=payload, headers=headers)
            resp.raise_for_status()
            data = resp.json()
        except httpx.HTTPStatusError as e:
            raise RuntimeError(f"HTTP {e.response.status_code}: {e.response.text[:300]}")
        except httpx.RequestError as e:
            raise RuntimeError(f"Request failed: {e}")

        if self.config.provider == ModelProvider.OLLAMA:
            content = data.get("response", "")
            tokens = data.get("eval_count", 0)
        elif data.get("choices"):
            choice = data["choices"][0]
            content = choice.get("text") or choice.get("message", {}).get("content", "")
            tokens = data.get("usage", {}).get("total_tokens", 0)
        else:
            raise RuntimeError(f"Unexpected response from {self.config.provider.value}: {data}")

        return ModelResponse(
            content=content,
            model=self.config.model_name,
            provider=self.config.provider,
            tokens_used=tokens,
            latency_ms=(time.monotonic() - start) * 1000,
        )

    async def health_check(self) -> bool:
        if not self._base_url:
            return False
        try:
            resp = await self._http.get(f"{self._base_url}/health", timeout=5)
            return resp.status_code < 500
        except Exception:
            return False

    async def close(self) -> None:
        await self._http.aclose()
