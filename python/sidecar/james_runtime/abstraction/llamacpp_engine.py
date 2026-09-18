"""JAMES Runtime llama.cpp Engine Implementation"""
import asyncio
import logging
import subprocess
import time
import httpx
from typing import Optional, AsyncGenerator, List, Dict, Any
from james_runtime.abstraction.base import InferenceEngine, EngineCapabilities, VRAMRequirements, HealthStatus, EngineConfig
from james_runtime.core.requests import CompletionRequest, StreamingRequest, Chunk, CompletionResponse
from james_runtime.core.errors import EngineInitializationError, EngineNotReadyError, InferenceTimeoutError
from james_runtime.models.registry import ModelSpec

logger = logging.getLogger(__name__)


class LlamaCppEngine(InferenceEngine):
    """llama.cpp Universal Local Inference Engine"""
    
    engine_type = "llamacpp"
    
    def __init__(self, config):
        self.config = config
        self.server_process: Optional[subprocess.Popen] = None
        self.client: Optional[httpx.AsyncClient] = None
        self._model_spec = None
        self._initialized = False
        self._server_ready = False
        host = getattr(config, 'host', '127.0.0.1') if config else '127.0.0.1'
        port = getattr(config, 'port', 8080) if config else 8080
        self._base_url = f"http://{host}:{port}"

    @property
    def capabilities(self) -> EngineCapabilities:
        max_context = getattr(self.config, 'max_context', 8192)
        return EngineCapabilities(
            max_context=max_context,
            streaming=True,
            tools=True,
            batching=True,
            structured_output=True,
            speculative_decode=True,  # llama.cpp supports draft models
            quantization_support=["fp16", "q8_0", "q5_k_m", "q4_k_m", "q3_k_m", "q2_k", "gguf"],
            hardware_targets=["cuda", "rocm", "metal", "vulkan", "cpu"],
        )
    
    async def initialize(self, config: EngineConfig) -> None:
        self._model_spec = config.model_spec
        llamacpp_config = config.llamacpp
        
        # Build llama-server command
        args = [
            "llama-server",
            "-m", llamacpp_config.model_path,
            "-ngl", str(llamacpp_config.n_gpu_layers),
            "-c", str(llamacpp_config.max_context),
            "--port", str(llamacpp_config.port),
            "--host", llamacpp_config.host,
            "--parallel", str(llamacpp_config.parallel),
        ]
        
        if llamacpp_config.flash_attn:
            args.extend(["--flash-attn", "auto"])
        if llamacpp_config.mlock:
            # --mlock is not supported on Windows
            import platform
            if platform.system() != "Windows":
                args.append("--mlock")
        if llamacpp_config.n_threads > 0:
            args.extend(["-t", str(llamacpp_config.n_threads)])

        logger.info(f"Starting llama.cpp server: {' '.join(args)}")

        try:
            self.server_process = subprocess.Popen(
                args,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                cwd=r"S:\JAMES\python\sidecar",
            )
        except FileNotFoundError:
            raise EngineInitializationError("llamacpp", "llama-server not found in PATH. Install llama.cpp")

        # Wait for server to be ready
        await self._wait_for_server_ready()
        self._initialized = True
        logger.info("llama.cpp engine initialized successfully")
    
    async def _wait_for_server_ready(self, timeout: float = 90.0) -> None:
        """Wait for llama-server to be ready"""
        start = time.time()
        while time.time() - start < timeout:
            try:
                async with httpx.AsyncClient() as client:
                    resp = await client.get(f"{self._base_url}/health", timeout=2.0)
                    if resp.status_code == 200:
                        self._server_ready = True
                        self.client = httpx.AsyncClient(base_url=self._base_url, timeout=300.0)
                        return
            except Exception:
                pass
            await asyncio.sleep(0.5)
        
        raise EngineInitializationError("llamacpp", f"llama-server did not start within {timeout}s")
    
    def estimate_vram(self, model_spec) -> VRAMRequirements:
        """Estimate VRAM requirements for llama.cpp"""
        # llama.cpp uses GGUF files; size depends on quantization
        gguf_size = getattr(model_spec, 'gguf_size_bytes', None) or 4_500_000_000
        n_layers = getattr(model_spec, 'n_layers', 32)
        
        # GPU layers
        n_gpu_layers = getattr(self.config, 'n_gpu_layers', -1) if self.config else -1
        gpu_layers = min(n_layers, n_gpu_layers if n_gpu_layers > 0 else n_layers)
        
        # VRAM per layer estimate
        vram_per_layer = gguf_size / n_layers if n_layers > 0 else 0
        gpu_vram = vram_per_layer * gpu_layers
        
        # KV cache
        max_context = getattr(model_spec, 'max_context', 8192)
        kv_cache = 2 * getattr(model_spec, 'n_layers', 32) * getattr(model_spec, 'hidden_size', 4096) * 2 * max_context
        
        return VRAMRequirements(
            model_bytes=int(gpu_vram),
            kv_cache_bytes=kv_cache,
            overhead_bytes=1_000_000_000,
            total_bytes=int(gpu_vram + kv_cache + 1_000_000_000),
            quantization=getattr(self._model_spec, 'quantization', 'gguf'),
        )
    
    def supports_model(self, model_spec) -> bool:
        # llama.cpp supports GGUF format primarily
        return True  # Very broad compatibility
    
    async def complete(self, request) -> CompletionResponse:
        if not self._initialized or not self.client:
            raise EngineNotReadyError(self.engine_type)
        
        # Convert to OpenAI format
        messages = [{"role": msg.role, "content": msg.content} for msg in request.messages]
        
        payload = {
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens or 4096,
            "stream": False,
            "top_p": request.top_p,
            "stop": request.stop,
        }
        
        if request.tools:
            payload["tools"] = [t.model_dump() for t in request.tools]
        
        try:
            response = await self.client.post(
                "/v1/chat/completions",
                json=payload,
                timeout=300.0,
            )
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
        except httpx.HTTPStatusError as e:
            logger.error(f"llama.cpp error: {e.response.text}")
            raise
        except Exception as e:
            logger.error(f"llama.cpp completion failed: {e}")
            raise
    
    async def stream(self, request: StreamingRequest) -> AsyncGenerator[Chunk, None]:
        if not self._initialized or not self.client:
            raise EngineNotReadyError(self.engine_type)
        
        messages = [{"role": msg.role, "content": msg.content} for msg in request.messages]
        
        payload = {
            "model": request.model,
            "messages": [{"role": msg.role, "content": msg.content} for msg in request.messages],
            "temperature": request.temperature,
            "max_tokens": request.max_tokens or 4096,
            "stream": True,
            "top_p": request.top_p,
        }
        
        try:
            async with self.client.stream(
                "POST", "/v1/chat/completions", json=payload, timeout=300.0
            ) as response:
                response.raise_for_status()
                async for line in response.aiter_lines():
                    if line.startswith("data: "):
                        data_str = line[6:]
                        if data_str.strip() == "[DONE]":
                            break
                        try:
                            import json
                            data = json.loads(data_str)
                            chunk = Chunk(
                                id=data.get("id", ""),
                                model=data.get("model", request.model),
                                created=data.get("created", int(time.time())),
                                choices=data.get("choices", []),
                            )
                            yield chunk
                        except json.JSONDecodeError:
                            continue
        except Exception as e:
            logger.error(f"llama.cpp streaming failed: {e}")
            raise
    
    async def health(self) -> HealthStatus:
        if not self._initialized:
            return HealthStatus(
                healthy=False,
                model_loaded=False,
                vram_used_gb=0.0,
                vram_total_gb=0.0,
                latency_p50_ms=0.0,
                latency_p99_ms=0.0,
                error_rate=1.0,
            )
        
        # Check server health
        try:
            resp = await self.client.get("/health", timeout=2.0)
            healthy = resp.status_code == 200
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
            model_loaded=True,
            vram_used_gb=vram_used,
            vram_total_gb=vram_total,
            latency_p50_ms=0.0,
            latency_p99_ms=0.0,
            error_rate=0.0,
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
        logger.info("llama.cpp engine shut down")