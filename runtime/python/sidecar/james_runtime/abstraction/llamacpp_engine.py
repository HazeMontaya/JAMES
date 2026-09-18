"""JAMES Runtime llama.cpp Engine Implementation"""
import asyncio
import logging
import platform
import subprocess
import time
from pathlib import Path
from typing import Optional, AsyncGenerator

import httpx

from james_runtime.abstraction.base import (
    InferenceEngine,
    EngineCapabilities,
    VRAMRequirements,
    HealthStatus,
    EngineConfig,
)
from james_runtime.core.requests import StreamingRequest, Chunk, CompletionResponse
from james_runtime.core.errors import EngineInitializationError, EngineNotReadyError
from james_runtime.models.registry import ModelSpec

logger = logging.getLogger(__name__)


class LlamaCppEngine(InferenceEngine):
    """llama.cpp local OpenAI-compatible inference engine."""

    engine_type = "llamacpp"

    def __init__(self, config):
        self.config = config
        self.server_process: Optional[subprocess.Popen] = None
        self.client: Optional[httpx.AsyncClient] = None
        self._model_spec: Optional[ModelSpec] = None
        self._initialized = False
        self._server_ready = False

        host = getattr(config, "host", "127.0.0.1") if config else "127.0.0.1"
        port = getattr(config, "port", 8080) if config else 8080
        self._base_url = f"http://{host}:{port}"

    @property
    def capabilities(self) -> EngineCapabilities:
        max_context = getattr(self.config, "max_context", 4096)
        return EngineCapabilities(
            max_context=max_context,
            streaming=True,
            tools=True,
            batching=True,
            structured_output=True,
            speculative_decode=True,
            quantization_support=["fp16", "q8_0", "q5_k_m", "q4_k_m", "q3_k_m", "q2_k", "gguf"],
            hardware_targets=["cuda", "rocm", "metal", "vulkan", "cpu"],
        )

    async def initialize(self, config: EngineConfig) -> None:
        self._model_spec = config.model_spec
        llamacpp_config = config.llamacpp or self.config

        executable = getattr(llamacpp_config, "executable", "llama-server")
        working_directory = getattr(llamacpp_config, "working_directory", None)
        auto_start = getattr(llamacpp_config, "auto_start", True)

        cwd = Path(working_directory).expanduser().resolve() if working_directory else Path.cwd()
        model_path = Path(llamacpp_config.model_path).expanduser()
        if not model_path.is_absolute():
            model_path = cwd / model_path
        model_path = model_path.resolve()

        if not model_path.exists():
            raise EngineInitializationError(
                "llamacpp",
                f"Model file not found: {model_path}. Run the local model bootstrap first.",
            )

        if not auto_start:
            logger.info("llama.cpp auto_start disabled; expecting an externally managed server")
            self.client = httpx.AsyncClient(base_url=self._base_url, timeout=300.0)
            await self._wait_for_server_ready()
            self._initialized = True
            return

        args = [
            executable,
            "-m", str(model_path),
            "-ngl", str(llamacpp_config.n_gpu_layers),
            "-c", str(llamacpp_config.max_context),
            "--port", str(llamacpp_config.port),
            "--host", llamacpp_config.host,
            "--parallel", str(llamacpp_config.parallel),
            "--batch-size", str(llamacpp_config.n_batch),
            "--ubatch-size", str(llamacpp_config.n_ubatch),
        ]

        if llamacpp_config.flash_attn:
            args.extend(["--flash-attn", "auto"])
        if llamacpp_config.mlock and platform.system() != "Windows":
            args.append("--mlock")
        if llamacpp_config.n_threads > 0:
            args.extend(["-t", str(llamacpp_config.n_threads)])

        logger.info("Starting llama.cpp server: %s", " ".join(args))

        try:
            self.server_process = subprocess.Popen(
                args,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                cwd=str(cwd),
            )
        except FileNotFoundError as exc:
            raise EngineInitializationError(
                "llamacpp", "llama-server not found in PATH. Install llama.cpp first."
            ) from exc

        self.client = httpx.AsyncClient(base_url=self._base_url, timeout=300.0)
        try:
            await self._wait_for_server_ready()
        except Exception:
            await self.shutdown()
            raise

        self._initialized = True
        logger.info("llama.cpp engine initialized successfully")

    async def _wait_for_server_ready(self, timeout: float = 90.0) -> None:
        """Wait for llama-server health endpoint."""
        if self.client is None:
            self.client = httpx.AsyncClient(base_url=self._base_url, timeout=300.0)

        start = time.time()
        while time.time() - start < timeout:
            try:
                response = await self.client.get("/health", timeout=2.0)
                if response.status_code == 200:
                    self._server_ready = True
                    return
            except Exception:
                pass
            await asyncio.sleep(0.5)

        raise EngineInitializationError(
            "llamacpp",
            f"llama-server did not become healthy within {timeout:.0f}s",
        )

    def estimate_vram(self, model_spec) -> VRAMRequirements:
        """Estimate VRAM requirements for a GGUF model."""
        gguf_size = getattr(model_spec, "gguf_size_bytes", None) or 2_000_000_000
        n_layers = max(getattr(model_spec, "n_layers", 32), 1)
        n_gpu_layers = getattr(self.config, "n_gpu_layers", -1)
        gpu_layers = min(n_layers, n_gpu_layers if n_gpu_layers > 0 else n_layers)

        vram_per_layer = gguf_size / n_layers
        gpu_vram = vram_per_layer * gpu_layers
        max_context = getattr(self.config, "max_context", 4096)
        kv_cache = (
            2
            * n_layers
            * getattr(model_spec, "hidden_size", 3072)
            * 2
            * max_context
        )

        return VRAMRequirements(
            model_bytes=int(gpu_vram),
            kv_cache_bytes=int(kv_cache),
            overhead_bytes=512 * 1024 * 1024,
            total_bytes=int(gpu_vram + kv_cache + 512 * 1024 * 1024),
            quantization=getattr(model_spec, "quantization", "gguf"),
        )

    def supports_model(self, model_spec) -> bool:
        return getattr(model_spec, "format", "").lower() in {"gguf", "ggml", ""}

    async def complete(self, request) -> CompletionResponse:
        if not self._initialized or not self.client:
            raise EngineNotReadyError(self.engine_type)

        payload = {
            "model": request.model,
            "messages": [{"role": msg.role, "content": msg.content} for msg in request.messages],
            "temperature": request.temperature,
            "max_tokens": request.max_tokens or 4096,
            "stream": False,
            "top_p": request.top_p,
            "stop": request.stop,
        }

        if request.tools:
            payload["tools"] = [t.model_dump() for t in request.tools]

        response = await self.client.post("/v1/chat/completions", json=payload, timeout=300.0)
        response.raise_for_status()
        data = response.json()

        return CompletionResponse(
            id=data.get("id", ""),
            model=data.get("model", request.model),
            created=data.get("created", int(time.time())),
            choices=data.get("choices", []),
            usage=data.get("usage"),
            runtime=self.engine_type,
        )

    async def stream(self, request: StreamingRequest) -> AsyncGenerator[Chunk, None]:
        if not self._initialized or not self.client:
            raise EngineNotReadyError(self.engine_type)

        payload = {
            "model": request.model,
            "messages": [{"role": msg.role, "content": msg.content} for msg in request.messages],
            "temperature": request.temperature,
            "max_tokens": request.max_tokens or 4096,
            "stream": True,
            "top_p": request.top_p,
        }

        async with self.client.stream(
            "POST", "/v1/chat/completions", json=payload, timeout=300.0
        ) as response:
            response.raise_for_status()
            async for line in response.aiter_lines():
                if not line.startswith("data: "):
                    continue
                data_str = line[6:]
                if data_str.strip() == "[DONE]":
                    break
                try:
                    import json
                    data = json.loads(data_str)
                except ValueError:
                    continue
                yield Chunk(
                    id=data.get("id", ""),
                    model=data.get("model", request.model),
                    created=data.get("created", int(time.time())),
                    choices=data.get("choices", []),
                )

    async def health(self) -> HealthStatus:
        if not self._initialized or not self.client:
            return HealthStatus(
                healthy=False,
                model_loaded=False,
                vram_used_gb=0.0,
                vram_total_gb=0.0,
                latency_p50_ms=0.0,
                latency_p99_ms=0.0,
                error_rate=1.0,
            )

        try:
            response = await self.client.get("/health", timeout=2.0)
            healthy = response.status_code == 200
        except Exception:
            healthy = False

        vram_used = 0.0
        vram_total = 0.0
        try:
            import GPUtil
            gpus = GPUtil.getGPUs()
            if gpus:
                gpu = gpus[0]
                vram_used = gpu.memoryUsed / 1024
                vram_total = gpu.memoryTotal / 1024
        except Exception:
            pass

        return HealthStatus(
            healthy=healthy,
            model_loaded=healthy,
            vram_used_gb=vram_used,
            vram_total_gb=vram_total,
            latency_p50_ms=0.0,
            latency_p99_ms=0.0,
            error_rate=0.0 if healthy else 1.0,
        )

    async def shutdown(self) -> None:
        if self.server_process:
            self.server_process.terminate()
            try:
                self.server_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.server_process.kill()
            self.server_process = None

        if self.client:
            await self.client.aclose()
            self.client = None

        self._initialized = False
        self._server_ready = False
        logger.info("llama.cpp engine shut down")
