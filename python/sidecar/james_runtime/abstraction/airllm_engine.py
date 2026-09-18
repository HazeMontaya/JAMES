"""JAMES Runtime AirLLM Engine Implementation"""
import asyncio
import logging
import time
from typing import Optional, AsyncGenerator, List, Dict, Any
from concurrent.futures import ThreadPoolExecutor
from james_runtime.abstraction.base import InferenceEngine, EngineCapabilities, VRAMRequirements, HealthStatus, EngineConfig
from james_runtime.core.requests import CompletionRequest, StreamingRequest, Chunk, CompletionResponse
from james_runtime.core.errors import EngineInitializationError, EngineNotReadyError
from james_runtime.models.registry import ModelSpec

logger = logging.getLogger(__name__)

try:
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer
    AIRLLM_AVAILABLE = True
except ImportError:
    AIRLLM_AVAILABLE = False
    torch = None


class AirLLMEngine(InferenceEngine):
    """AirLLM Large Model Offload Engine - Layer-by-layer GPU offloading"""
    
    engine_type = "airllm"
    
    def __init__(self, config):
        self.config = config
        self.model = None
        self.tokenizer = None
        self._model_spec = None
        self._initialized = False
        self._available = AIRLLM_AVAILABLE
        self._executor = ThreadPoolExecutor(max_workers=1)
        if not AIRLLM_AVAILABLE:
            logger.warning("torch/transformers not installed - airllm engine will be unavailable")
    
    @property
    def capabilities(self) -> EngineCapabilities:
        return EngineCapabilities(
            max_context=self.config.max_context,
            streaming=True,
            tools=False,  # AirLLM doesn't natively support tool calling
            batching=False,
            structured_output=False,
            speculative_decode=False,
            quantization_support=["fp16", "bf16"],
            hardware_targets=["cuda", "rocm", "cpu"],
        )
    
    async def initialize(self, config: EngineConfig) -> None:
        if not AIRLLM_AVAILABLE:
            raise EngineInitializationError("airllm", "AirLLM dependencies not available. Install: pip install torch transformers accelerate")

        self._model_spec = config.model_spec
        airllm_config = config.airllm
        
        logger.info(f"Loading AirLLM model: {airllm_config.model_path}")
        
        # Load tokenizer
        self.tokenizer = AutoTokenizer.from_pretrained(
            airllm_config.model_path,
            trust_remote_code=True,
        )
        
        # Load model with AirLLM-style layer offloading
        self.model = AutoModelForCausalLM.from_pretrained(
            airllm_config.model_path,
            device_map="auto",
            torch_dtype=getattr(torch, airllm_config.dtype),
            max_memory={
                0: f"{airllm_config.vram_limit_gb}GB",
                "cpu": f"{airllm_config.ram_limit_gb}GB",
            },
            low_cpu_mem_usage=True,
            trust_remote_code=True,
        )
        
        if airllm_config.offload_buffers and hasattr(self.model, 'to_bettertransformer'):
            try:
                self.model = self.model.to_bettertransformer()
            except Exception:
                pass
        
        self._model_spec = config.model_spec
        self._initialized = True
        logger.info("AirLLM engine initialized successfully")
    
    def estimate_vram(self, model_spec) -> VRAMRequirements:
        # AirLLM only needs VRAM for active layers (1-2 at a time) + KV cache
        max_context = getattr(model_spec, 'max_context', 4096)
        layers = getattr(model_spec, 'n_layers', 80)
        hidden_size = getattr(model_spec, 'hidden_size', 8192)
        vram_limit_gb = getattr(self.config, 'vram_limit_gb', 4.0) if self.config else 4.0
        
        # Only 1-2 layers in GPU at a time
        active_layer_bytes = 2 * layers * hidden_size * hidden_size * 2  # fp16
        
        # KV cache for full context
        kv_cache = 2 * getattr(model_spec, 'n_layers', 80) * hidden_size * 2 * max_context
        
        return VRAMRequirements(
            model_bytes=0,  # Model weights in CPU RAM
            kv_cache_bytes=kv_cache,
            overhead_bytes=int(vram_limit_gb * 1_000_000_000),
            total_bytes=int(vram_limit_gb * 1_000_000_000 + kv_cache),
            quantization="fp16",
        )
    
    def supports_model(self, model_spec) -> bool:
        # AirLLM supports large HF models
        return True
    
    def _generate_sync(self, request) -> CompletionResponse:
        """Synchronous generation (runs in thread pool)"""
        inputs = self.tokenizer(
            self._messages_to_prompt(request.messages),
            return_tensors="pt",
            truncation=True,
            max_length=self.config.max_context,
        ).to("cuda" if torch.cuda.is_available() else "cpu")
        
        with torch.no_grad():
            outputs = self.model.generate(
                **inputs,
                max_new_tokens=request.max_tokens or 512,
                temperature=request.temperature,
                do_sample=request.temperature > 0,
                top_p=request.top_p,
                pad_token_id=self.tokenizer.eos_token_id,
                eos_token_id=self.tokenizer.eos_token_id,
            )
        
        # Decode only the new tokens
        new_tokens = outputs[0][inputs.input_ids.shape[1]:]
        text = self.tokenizer.decode(new_tokens, skip_special_tokens=True)
        
        return CompletionResponse(
            id=f"airllm_{int(time.time())}",
            model=request.model,
            created=int(time.time()),
            choices=[{
                "index": 0,
                "message": {"role": "assistant", "content": text},
                "finish_reason": "stop",
            }],
            usage={
                "prompt_tokens": inputs.input_ids.shape[1],
                "completion_tokens": new_tokens.shape[0],
                "total_tokens": inputs.input_ids.shape[1] + new_tokens.shape[0],
            },
            runtime=self.engine_type,
        )
    
    async def complete(self, request) -> CompletionResponse:
        if not self._initialized:
            raise EngineNotReadyError(self.engine_type)
        
        # Run in thread pool to avoid blocking event loop
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(self._executor, self._generate_sync, request)
    
    def _messages_to_prompt(self, messages) -> str:
        prompt = ""
        for msg in messages:
            role = msg.role
            content = msg.content
            if role == "system":
                prompt += f"<|system|>\n{content}\n"
            elif role == "user":
                prompt += f"<|user|>\n{content}\n"
            elif role == "assistant":
                prompt += f"<|assistant|>\n{content}\n"
            elif role == "tool":
                prompt += f"<|tool|>\n{content}\n"
        prompt += "<|assistant|>\n"
        return prompt
    
    async def stream(self, request: StreamingRequest) -> AsyncGenerator[Chunk, None]:
        # AirLLM doesn't support native streaming well
        # Simulate by yielding the full response
        response = await self.complete(request)
        for choice in response.choices:
            chunk = Chunk(
                id=response.id,
                model=response.model,
                created=response.created,
                choices=[{
                    "index": 0,
                    "delta": {"content": choice["message"]["content"]},
                    "finish_reason": "stop",
                }]
            )
            yield chunk
    
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
        
        vram_used = 0.0
        vram_total = 0.0
        try:
            if torch.cuda.is_available():
                vram_used = torch.cuda.memory_allocated() / (1024**3)
                vram_total = torch.cuda.get_device_properties(0).total_memory / (1024**3)
        except Exception:
            pass
        
        return HealthStatus(
            healthy=self._initialized,
            model_loaded=True,
            vram_used_gb=vram_used,
            vram_total_gb=vram_total,
            latency_p50_ms=0.0,
            latency_p99_ms=0.0,
            error_rate=0.0,
        )
    
    async def shutdown(self) -> None:
        self._executor.shutdown(wait=True)
        if self.model:
            del self.model
        if self.tokenizer:
            del self.tokenizer
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
        self._initialized = False
        logger.info("AirLLM engine shut down")