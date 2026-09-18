"""JAMES Runtime vLLM Engine Implementation"""
import asyncio
import logging
import time
from typing import Optional, AsyncGenerator, List, Dict, Any
from james_runtime.abstraction.base import InferenceEngine, EngineCapabilities, VRAMRequirements, HealthStatus, EngineConfig
from james_runtime.core.requests import CompletionRequest, StreamingRequest, Chunk, CompletionResponse
from james_runtime.core.errors import EngineInitializationError, EngineNotReadyError, InferenceTimeoutError
from james_runtime.models.registry import ModelSpec

logger = logging.getLogger(__name__)

try:
    from vllm import AsyncLLMEngine, AsyncEngineArgs, SamplingParams
    from vllm.outputs import RequestOutput
    VLLM_AVAILABLE = True
except ImportError:
    VLLM_AVAILABLE = False
    AsyncLLMEngine = None
    AsyncEngineArgs = None
    SamplingParams = None


class VLLMEngine(InferenceEngine):
    """vLLM High-Performance Inference Engine"""
    
    engine_type = "vllm"
    
    def __init__(self, config):
        self.config = config
        self.engine: Optional[Any] = None
        self._model_spec: Optional[Any] = None
        self._initialized = False
        self._available = VLLM_AVAILABLE
        self._request_count = 0
        self._total_latency_ms = 0.0
        self._error_count = 0
        if not VLLM_AVAILABLE:
            logger.warning("vLLM not installed - vllm engine will be unavailable")

    @property
    def capabilities(self) -> EngineCapabilities:
        max_context = getattr(self.config, 'max_context', 8192)
        return EngineCapabilities(
            max_context=max_context,
            streaming=True,
            tools=True,
            batching=True,
            structured_output=True,
            speculative_decode=False,  # vLLM supports but complex to configure
            quantization_support=["fp16", "bf16", "fp8", "awq", "gptq", "bitsandbytes_int4"],
            hardware_targets=["cuda", "rocm"],
        )
    
    async def initialize(self, config: EngineConfig) -> None:
        if not VLLM_AVAILABLE:
            raise EngineInitializationError("vllm", "vLLM not available")
        
        self._model_spec = config.model_spec
        vllm_config = config.vllm
        
        engine_args = AsyncEngineArgs(
            model=vllm_config.model_path,
            tensor_parallel_size=vllm_config.tp_size,
            gpu_memory_utilization=vllm_config.gpu_mem_util,
            dtype=vllm_config.dtype if vllm_config.dtype != "auto" else "auto",
            max_model_len=vllm_config.max_context,
            enable_prefix_caching=True,
            enable_chunked_prefill=True,
            quantization=vllm_config.quantization,
            enforce_eager=vllm_config.enforce_eager,
            trust_remote_code=vllm_config.trust_remote_code,
            max_num_seqs=vllm_config.max_num_seqs,
            max_num_batched_tokens=vllm_config.max_num_batched_tokens,
        )
        
        logger.info(f"Initializing vLLM engine with model: {vllm_config.model_path}")
        self.engine = AsyncLLMEngine.from_engine_args(engine_args)
        self._initialized = True
        logger.info("vLLM engine initialized successfully")
    
    def estimate_vram(self, model_spec) -> VRAMRequirements:
        """Estimate VRAM requirements for a model"""
        params_b = getattr(model_spec, 'parameters_b', 7)
        quantization = getattr(model_spec, 'quantization', 'fp16')
        
        bytes_per_param = {
            "fp16": 2, "bf16": 2, "fp8": 1, "int4": 0.5,
            "awq": 0.5, "gptq": 0.5, "bitsandbytes_int4": 0.5,
        }.get(quantization, 2)
        
        model_bytes = int(params_b * 1e9 * bytes_per_param)
        
        # KV cache estimation: 2 * layers * hidden_size * max_context * dtype_bytes
        layers = getattr(model_spec, 'n_layers', 32)
        hidden_size = getattr(model_spec, 'hidden_size', 4096)
        max_context = getattr(model_spec, 'max_context', 8192)
        kv_dtype_bytes = 2  # fp16 for KV cache
        
        kv_cache_per_token = 2 * layers * hidden_size * kv_dtype_bytes
        kv_cache = kv_cache_per_token * max_context
        
        overhead = 2_000_000_000  # 2GB overhead
        
        return VRAMRequirements(
            model_bytes=model_bytes,
            kv_cache_bytes=kv_cache,
            overhead_bytes=2_000_000_000,
            total_bytes=model_bytes + kv_cache + 2_000_000_000,
            quantization=quantization,
        )
    
    def supports_model(self, model_spec) -> bool:
        """Check if vLLM can run this model"""
        if not VLLM_AVAILABLE:
            return False

        # vLLM supports most HF models with proper quantization
        supported_formats = ['hf', 'awq', 'gptq', 'fp8']
        model_format = getattr(model_spec, 'format', 'hf')
        return model_format in supported_formats
    
    async def complete(self, request) -> CompletionResponse:
        if not self._initialized or not self.engine:
            raise EngineNotReadyError(self.engine_type)
        
        sampling_params = SamplingParams(
            temperature=request.temperature,
            max_tokens=request.max_tokens or 4096,
            top_p=request.top_p,
            top_k=request.top_k or -1,
            stop=request.stop or [],
            presence_penalty=request.presence_penalty,
            frequency_penalty=request.frequency_penalty,
            skip_special_tokens=True,
        )
        
        # Handle tools if present
        if request.tools:
            # vLLM tool calling requires specific format
            pass
        
        request_id = getattr(request, 'request_id', 'req_' + str(hash(str(request))))
        
        try:
            result_generator = self.engine.generate(
                prompt=getattr(request, 'prompt', self._messages_to_prompt(request.messages)),
                sampling_params=sampling_params,
                request_id=request_id,
            )
            
            final_output = None
            async for output in result_generator:
                final_output = output
            
            if not final_output or not final_output.outputs:
                raise RuntimeError("No output generated")
            
            output = final_output.outputs[0]
            text = output.text
            
            # Build response
            response = CompletionResponse(
                id=final_output.request_id,
                model=request.model,
                created=int(time.time()),
                choices=[{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": text,
                    },
                    "finish_reason": output.finish_reason,
                }],
                usage={
                    "prompt_tokens": len(final_output.prompt_token_ids) if final_output.prompt_token_ids else 0,
                    "completion_tokens": len(output.token_ids) if output.token_ids else 0,
                    "total_tokens": len(final_output.prompt_token_ids) + len(output.token_ids) if final_output.prompt_token_ids and output.token_ids else 0,
                },
                runtime=self.engine_type,
            )
            
            return response
            
        except Exception as e:
            logger.error(f"vLLM completion failed: {e}")
            raise
    
    async def stream(self, request: StreamingRequest) -> AsyncGenerator[Chunk, None]:
        if not self._initialized or not self.engine:
            raise EngineNotReadyError(self.engine_type)
        
        sampling_params = SamplingParams(
            temperature=request.temperature,
            max_tokens=request.max_tokens or 4096,
            top_p=request.top_p,
            top_k=request.top_k or -1,
            stop=request.stop or [],
            presence_penalty=request.presence_penalty,
            frequency_penalty=request.frequency_penalty,
        )
        
        request_id = getattr(request, 'request_id', 'req_' + str(hash(str(request))))
        
        result_generator = self.engine.generate(
            prompt=getattr(request, 'prompt', self._messages_to_prompt(request.messages)),
            sampling_params=sampling_params,
            request_id=request_id,
        )
        
        token_index = 0
        try:
            async for output in result_generator:
                if output.outputs:
                    output_text = output.outputs[0].text
                    # For streaming, we'd need to track delta - simplified here
                    chunk = Chunk(
                        id=output.request_id,
                        model=request.model,
                        created=int(time.time()),
                        choices=[{
                            "index": 0,
                            "delta": {"content": output_text},
                            "finish_reason": output.outputs[0].finish_reason,
                        }]
                    )
                    yield chunk
        except Exception as e:
            logger.error(f"vLLM streaming failed: {e}")
            raise
    
    def _messages_to_prompt(self, messages) -> str:
        """Convert chat messages to prompt string"""
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
        
        # Try to get actual GPU memory usage
        vram_used = 0.0
        vram_total = 0.0
        try:
            import GPUtil
            gpus = GPUtil.getGPUs()
            if gpus:
                gpu = gpus[0]
                vram_used = gpu.memoryUsed / 1024  # MB to GB
                vram_total = gpu.memoryTotal / 1024
        except Exception:
            pass
        
        return HealthStatus(
            healthy=self._initialized,
            model_loaded=True,
            vram_used_gb=vram_used,
            vram_total_gb=vram_total,
            latency_p50_ms=self._total_latency_ms / max(1, self._request_count),
            latency_p99_ms=self._total_latency_ms / max(1, self._request_count) * 1.5,
            error_rate=self._error_count / max(1, self._request_count),
        )
    
    async def shutdown(self) -> None:
        if self.engine:
            # vLLM doesn't have explicit shutdown, just dereference
            self.engine = None
            self._initialized = False
            logger.info("vLLM engine shut down")