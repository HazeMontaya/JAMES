from google.protobuf import empty_pb2 as _empty_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class QualityTier(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    QUALITY_TIER_UNSPECIFIED: _ClassVar[QualityTier]
    QUALITY_TIER_FAST: _ClassVar[QualityTier]
    QUALITY_TIER_BALANCED: _ClassVar[QualityTier]
    QUALITY_TIER_BEST: _ClassVar[QualityTier]

class PrivacyLevel(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    PRIVACY_LEVEL_UNSPECIFIED: _ClassVar[PrivacyLevel]
    PRIVACY_LEVEL_LOCAL_ONLY: _ClassVar[PrivacyLevel]
    PRIVACY_LEVEL_PREFER_LOCAL: _ClassVar[PrivacyLevel]
    PRIVACY_LEVEL_ALLOW_CLOUD: _ClassVar[PrivacyLevel]

class EngineType(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    ENGINE_TYPE_UNSPECIFIED: _ClassVar[EngineType]
    ENGINE_TYPE_VLLM: _ClassVar[EngineType]
    ENGINE_TYPE_LLAMACPP: _ClassVar[EngineType]
    ENGINE_TYPE_AIRLLM: _ClassVar[EngineType]
QUALITY_TIER_UNSPECIFIED: QualityTier
QUALITY_TIER_FAST: QualityTier
QUALITY_TIER_BALANCED: QualityTier
QUALITY_TIER_BEST: QualityTier
PRIVACY_LEVEL_UNSPECIFIED: PrivacyLevel
PRIVACY_LEVEL_LOCAL_ONLY: PrivacyLevel
PRIVACY_LEVEL_PREFER_LOCAL: PrivacyLevel
PRIVACY_LEVEL_ALLOW_CLOUD: PrivacyLevel
ENGINE_TYPE_UNSPECIFIED: EngineType
ENGINE_TYPE_VLLM: EngineType
ENGINE_TYPE_LLAMACPP: EngineType
ENGINE_TYPE_AIRLLM: EngineType

class ChatMessage(_message.Message):
    __slots__ = ("role", "content", "name", "tool_calls", "tool_call_id")
    ROLE_FIELD_NUMBER: _ClassVar[int]
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    TOOL_CALLS_FIELD_NUMBER: _ClassVar[int]
    TOOL_CALL_ID_FIELD_NUMBER: _ClassVar[int]
    role: str
    content: str
    name: str
    tool_calls: _containers.RepeatedCompositeFieldContainer[ToolCall]
    tool_call_id: str
    def __init__(self, role: _Optional[str] = ..., content: _Optional[str] = ..., name: _Optional[str] = ..., tool_calls: _Optional[_Iterable[_Union[ToolCall, _Mapping]]] = ..., tool_call_id: _Optional[str] = ...) -> None: ...

class ToolCall(_message.Message):
    __slots__ = ("id", "type", "function")
    ID_FIELD_NUMBER: _ClassVar[int]
    TYPE_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_FIELD_NUMBER: _ClassVar[int]
    id: str
    type: str
    function: Function
    def __init__(self, id: _Optional[str] = ..., type: _Optional[str] = ..., function: _Optional[_Union[Function, _Mapping]] = ...) -> None: ...

class Function(_message.Message):
    __slots__ = ("name", "arguments")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ARGUMENTS_FIELD_NUMBER: _ClassVar[int]
    name: str
    arguments: str
    def __init__(self, name: _Optional[str] = ..., arguments: _Optional[str] = ...) -> None: ...

class Tool(_message.Message):
    __slots__ = ("type", "function")
    TYPE_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_FIELD_NUMBER: _ClassVar[int]
    type: str
    function: Function
    def __init__(self, type: _Optional[str] = ..., function: _Optional[_Union[Function, _Mapping]] = ...) -> None: ...

class CompletionRequest(_message.Message):
    __slots__ = ("model", "messages", "temperature", "max_tokens", "stream", "tools", "runtime_hint")
    MODEL_FIELD_NUMBER: _ClassVar[int]
    MESSAGES_FIELD_NUMBER: _ClassVar[int]
    TEMPERATURE_FIELD_NUMBER: _ClassVar[int]
    MAX_TOKENS_FIELD_NUMBER: _ClassVar[int]
    STREAM_FIELD_NUMBER: _ClassVar[int]
    TOOLS_FIELD_NUMBER: _ClassVar[int]
    RUNTIME_HINT_FIELD_NUMBER: _ClassVar[int]
    model: str
    messages: _containers.RepeatedCompositeFieldContainer[ChatMessage]
    temperature: float
    max_tokens: int
    stream: bool
    tools: _containers.RepeatedCompositeFieldContainer[Tool]
    runtime_hint: str
    def __init__(self, model: _Optional[str] = ..., messages: _Optional[_Iterable[_Union[ChatMessage, _Mapping]]] = ..., temperature: _Optional[float] = ..., max_tokens: _Optional[int] = ..., stream: _Optional[bool] = ..., tools: _Optional[_Iterable[_Union[Tool, _Mapping]]] = ..., runtime_hint: _Optional[str] = ...) -> None: ...

class Choice(_message.Message):
    __slots__ = ("index", "message", "finish_reason")
    INDEX_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    FINISH_REASON_FIELD_NUMBER: _ClassVar[int]
    index: int
    message: ChatMessage
    finish_reason: str
    def __init__(self, index: _Optional[int] = ..., message: _Optional[_Union[ChatMessage, _Mapping]] = ..., finish_reason: _Optional[str] = ...) -> None: ...

class CompletionResponse(_message.Message):
    __slots__ = ("id", "model", "created", "choices", "usage")
    class UsageEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    ID_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    CREATED_FIELD_NUMBER: _ClassVar[int]
    CHOICES_FIELD_NUMBER: _ClassVar[int]
    USAGE_FIELD_NUMBER: _ClassVar[int]
    id: str
    model: str
    created: int
    choices: _containers.RepeatedCompositeFieldContainer[Choice]
    usage: _containers.ScalarMap[str, str]
    def __init__(self, id: _Optional[str] = ..., model: _Optional[str] = ..., created: _Optional[int] = ..., choices: _Optional[_Iterable[_Union[Choice, _Mapping]]] = ..., usage: _Optional[_Mapping[str, str]] = ...) -> None: ...

class Chunk(_message.Message):
    __slots__ = ("id", "model", "created", "choices")
    ID_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    CREATED_FIELD_NUMBER: _ClassVar[int]
    CHOICES_FIELD_NUMBER: _ClassVar[int]
    id: str
    model: str
    created: int
    choices: _containers.RepeatedCompositeFieldContainer[Choice]
    def __init__(self, id: _Optional[str] = ..., model: _Optional[str] = ..., created: _Optional[int] = ..., choices: _Optional[_Iterable[_Union[Choice, _Mapping]]] = ...) -> None: ...

class RoutingRequest(_message.Message):
    __slots__ = ("task", "required_capabilities", "quality_tier", "privacy", "latency_budget_ms", "cost_budget_per_1k", "context_length_needed")
    TASK_FIELD_NUMBER: _ClassVar[int]
    REQUIRED_CAPABILITIES_FIELD_NUMBER: _ClassVar[int]
    QUALITY_TIER_FIELD_NUMBER: _ClassVar[int]
    PRIVACY_FIELD_NUMBER: _ClassVar[int]
    LATENCY_BUDGET_MS_FIELD_NUMBER: _ClassVar[int]
    COST_BUDGET_PER_1K_FIELD_NUMBER: _ClassVar[int]
    CONTEXT_LENGTH_NEEDED_FIELD_NUMBER: _ClassVar[int]
    task: str
    required_capabilities: _containers.RepeatedScalarFieldContainer[str]
    quality_tier: QualityTier
    privacy: PrivacyLevel
    latency_budget_ms: int
    cost_budget_per_1k: float
    context_length_needed: int
    def __init__(self, task: _Optional[str] = ..., required_capabilities: _Optional[_Iterable[str]] = ..., quality_tier: _Optional[_Union[QualityTier, str]] = ..., privacy: _Optional[_Union[PrivacyLevel, str]] = ..., latency_budget_ms: _Optional[int] = ..., cost_budget_per_1k: _Optional[float] = ..., context_length_needed: _Optional[int] = ...) -> None: ...

class RoutingDecision(_message.Message):
    __slots__ = ("model_id", "provider", "reasoning", "fallback_chain", "estimated_cost_per_1k", "estimated_latency_ms", "required_capabilities")
    MODEL_ID_FIELD_NUMBER: _ClassVar[int]
    PROVIDER_FIELD_NUMBER: _ClassVar[int]
    REASONING_FIELD_NUMBER: _ClassVar[int]
    FALLBACK_CHAIN_FIELD_NUMBER: _ClassVar[int]
    ESTIMATED_COST_PER_1K_FIELD_NUMBER: _ClassVar[int]
    ESTIMATED_LATENCY_MS_FIELD_NUMBER: _ClassVar[int]
    REQUIRED_CAPABILITIES_FIELD_NUMBER: _ClassVar[int]
    model_id: str
    provider: str
    reasoning: str
    fallback_chain: _containers.RepeatedScalarFieldContainer[str]
    estimated_cost_per_1k: float
    estimated_latency_ms: int
    required_capabilities: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, model_id: _Optional[str] = ..., provider: _Optional[str] = ..., reasoning: _Optional[str] = ..., fallback_chain: _Optional[_Iterable[str]] = ..., estimated_cost_per_1k: _Optional[float] = ..., estimated_latency_ms: _Optional[int] = ..., required_capabilities: _Optional[_Iterable[str]] = ...) -> None: ...

class RuntimeRequest(_message.Message):
    __slots__ = ("model_id", "quantization", "offload", "max_latency_ms", "max_vram_gb")
    MODEL_ID_FIELD_NUMBER: _ClassVar[int]
    QUANTIZATION_FIELD_NUMBER: _ClassVar[int]
    OFFLOAD_FIELD_NUMBER: _ClassVar[int]
    MAX_LATENCY_MS_FIELD_NUMBER: _ClassVar[int]
    MAX_VRAM_GB_FIELD_NUMBER: _ClassVar[int]
    model_id: str
    quantization: str
    offload: bool
    max_latency_ms: int
    max_vram_gb: float
    def __init__(self, model_id: _Optional[str] = ..., quantization: _Optional[str] = ..., offload: _Optional[bool] = ..., max_latency_ms: _Optional[int] = ..., max_vram_gb: _Optional[float] = ...) -> None: ...

class RuntimeSelection(_message.Message):
    __slots__ = ("engine_type", "model_id", "quantization", "offload", "estimated_vram_gb", "estimated_latency_ms", "fallback_engines")
    ENGINE_TYPE_FIELD_NUMBER: _ClassVar[int]
    MODEL_ID_FIELD_NUMBER: _ClassVar[int]
    QUANTIZATION_FIELD_NUMBER: _ClassVar[int]
    OFFLOAD_FIELD_NUMBER: _ClassVar[int]
    ESTIMATED_VRAM_GB_FIELD_NUMBER: _ClassVar[int]
    ESTIMATED_LATENCY_MS_FIELD_NUMBER: _ClassVar[int]
    FALLBACK_ENGINES_FIELD_NUMBER: _ClassVar[int]
    engine_type: EngineType
    model_id: str
    quantization: str
    offload: bool
    estimated_vram_gb: float
    estimated_latency_ms: int
    fallback_engines: _containers.RepeatedScalarFieldContainer[EngineType]
    def __init__(self, engine_type: _Optional[_Union[EngineType, str]] = ..., model_id: _Optional[str] = ..., quantization: _Optional[str] = ..., offload: _Optional[bool] = ..., estimated_vram_gb: _Optional[float] = ..., estimated_latency_ms: _Optional[int] = ..., fallback_engines: _Optional[_Iterable[_Union[EngineType, str]]] = ...) -> None: ...

class HardwareProfile(_message.Message):
    __slots__ = ("gpu_name", "vram_gb", "cuda_version", "rocm_version", "metal_support", "cpu_cores", "ram_gb")
    GPU_NAME_FIELD_NUMBER: _ClassVar[int]
    VRAM_GB_FIELD_NUMBER: _ClassVar[int]
    CUDA_VERSION_FIELD_NUMBER: _ClassVar[int]
    ROCM_VERSION_FIELD_NUMBER: _ClassVar[int]
    METAL_SUPPORT_FIELD_NUMBER: _ClassVar[int]
    CPU_CORES_FIELD_NUMBER: _ClassVar[int]
    RAM_GB_FIELD_NUMBER: _ClassVar[int]
    gpu_name: str
    vram_gb: float
    cuda_version: str
    rocm_version: str
    metal_support: bool
    cpu_cores: int
    ram_gb: float
    def __init__(self, gpu_name: _Optional[str] = ..., vram_gb: _Optional[float] = ..., cuda_version: _Optional[str] = ..., rocm_version: _Optional[str] = ..., metal_support: _Optional[bool] = ..., cpu_cores: _Optional[int] = ..., ram_gb: _Optional[float] = ...) -> None: ...

class ModelInfo(_message.Message):
    __slots__ = ("id", "name", "provider", "parameters_b", "max_context", "capabilities", "quality_tier", "quality_score", "quantization", "format")
    ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    PROVIDER_FIELD_NUMBER: _ClassVar[int]
    PARAMETERS_B_FIELD_NUMBER: _ClassVar[int]
    MAX_CONTEXT_FIELD_NUMBER: _ClassVar[int]
    CAPABILITIES_FIELD_NUMBER: _ClassVar[int]
    QUALITY_TIER_FIELD_NUMBER: _ClassVar[int]
    QUALITY_SCORE_FIELD_NUMBER: _ClassVar[int]
    QUANTIZATION_FIELD_NUMBER: _ClassVar[int]
    FORMAT_FIELD_NUMBER: _ClassVar[int]
    id: str
    name: str
    provider: str
    parameters_b: int
    max_context: int
    capabilities: _containers.RepeatedScalarFieldContainer[str]
    quality_tier: str
    quality_score: float
    quantization: str
    format: str
    def __init__(self, id: _Optional[str] = ..., name: _Optional[str] = ..., provider: _Optional[str] = ..., parameters_b: _Optional[int] = ..., max_context: _Optional[int] = ..., capabilities: _Optional[_Iterable[str]] = ..., quality_tier: _Optional[str] = ..., quality_score: _Optional[float] = ..., quantization: _Optional[str] = ..., format: _Optional[str] = ...) -> None: ...

class ModelRegistry(_message.Message):
    __slots__ = ("models",)
    MODELS_FIELD_NUMBER: _ClassVar[int]
    models: _containers.RepeatedCompositeFieldContainer[ModelInfo]
    def __init__(self, models: _Optional[_Iterable[_Union[ModelInfo, _Mapping]]] = ...) -> None: ...

class CostReport(_message.Message):
    __slots__ = ("total_cost_usd", "cost_by_model", "cost_by_engine", "cost_by_agent")
    class CostByModelEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: float
        def __init__(self, key: _Optional[str] = ..., value: _Optional[float] = ...) -> None: ...
    class CostByEngineEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: float
        def __init__(self, key: _Optional[str] = ..., value: _Optional[float] = ...) -> None: ...
    class CostByAgentEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: float
        def __init__(self, key: _Optional[str] = ..., value: _Optional[float] = ...) -> None: ...
    TOTAL_COST_USD_FIELD_NUMBER: _ClassVar[int]
    COST_BY_MODEL_FIELD_NUMBER: _ClassVar[int]
    COST_BY_ENGINE_FIELD_NUMBER: _ClassVar[int]
    COST_BY_AGENT_FIELD_NUMBER: _ClassVar[int]
    total_cost_usd: float
    cost_by_model: _containers.ScalarMap[str, float]
    cost_by_engine: _containers.ScalarMap[str, float]
    cost_by_agent: _containers.ScalarMap[str, float]
    def __init__(self, total_cost_usd: _Optional[float] = ..., cost_by_model: _Optional[_Mapping[str, float]] = ..., cost_by_engine: _Optional[_Mapping[str, float]] = ..., cost_by_agent: _Optional[_Mapping[str, float]] = ...) -> None: ...

class BudgetPolicy(_message.Message):
    __slots__ = ("daily_ai_budget_usd", "monthly_ai_budget_usd", "max_single_request_usd", "approved_models", "approved_providers")
    DAILY_AI_BUDGET_USD_FIELD_NUMBER: _ClassVar[int]
    MONTHLY_AI_BUDGET_USD_FIELD_NUMBER: _ClassVar[int]
    MAX_SINGLE_REQUEST_USD_FIELD_NUMBER: _ClassVar[int]
    APPROVED_MODELS_FIELD_NUMBER: _ClassVar[int]
    APPROVED_PROVIDERS_FIELD_NUMBER: _ClassVar[int]
    daily_ai_budget_usd: float
    monthly_ai_budget_usd: float
    max_single_request_usd: float
    approved_models: _containers.RepeatedScalarFieldContainer[str]
    approved_providers: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, daily_ai_budget_usd: _Optional[float] = ..., monthly_ai_budget_usd: _Optional[float] = ..., max_single_request_usd: _Optional[float] = ..., approved_models: _Optional[_Iterable[str]] = ..., approved_providers: _Optional[_Iterable[str]] = ...) -> None: ...

class BudgetStatus(_message.Message):
    __slots__ = ("daily_spent", "daily_limit", "monthly_spent", "monthly_limit", "daily_exceeded", "monthly_exceeded")
    DAILY_SPENT_FIELD_NUMBER: _ClassVar[int]
    DAILY_LIMIT_FIELD_NUMBER: _ClassVar[int]
    MONTHLY_SPENT_FIELD_NUMBER: _ClassVar[int]
    MONTHLY_LIMIT_FIELD_NUMBER: _ClassVar[int]
    DAILY_EXCEEDED_FIELD_NUMBER: _ClassVar[int]
    MONTHLY_EXCEEDED_FIELD_NUMBER: _ClassVar[int]
    daily_spent: float
    daily_limit: float
    monthly_spent: float
    monthly_limit: float
    daily_exceeded: bool
    monthly_exceeded: bool
    def __init__(self, daily_spent: _Optional[float] = ..., daily_limit: _Optional[float] = ..., monthly_spent: _Optional[float] = ..., monthly_limit: _Optional[float] = ..., daily_exceeded: _Optional[bool] = ..., monthly_exceeded: _Optional[bool] = ...) -> None: ...

class RuntimeEvent(_message.Message):
    __slots__ = ("event_type", "timestamp", "event_id", "correlation_id", "causation_id", "payload", "source", "severity")
    class PayloadEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    EVENT_ID_FIELD_NUMBER: _ClassVar[int]
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    CAUSATION_ID_FIELD_NUMBER: _ClassVar[int]
    PAYLOAD_FIELD_NUMBER: _ClassVar[int]
    SOURCE_FIELD_NUMBER: _ClassVar[int]
    SEVERITY_FIELD_NUMBER: _ClassVar[int]
    event_type: str
    timestamp: int
    event_id: str
    correlation_id: str
    causation_id: str
    payload: _containers.ScalarMap[str, str]
    source: str
    severity: str
    def __init__(self, event_type: _Optional[str] = ..., timestamp: _Optional[int] = ..., event_id: _Optional[str] = ..., correlation_id: _Optional[str] = ..., causation_id: _Optional[str] = ..., payload: _Optional[_Mapping[str, str]] = ..., source: _Optional[str] = ..., severity: _Optional[str] = ...) -> None: ...

class EventFilter(_message.Message):
    __slots__ = ("event_types", "source", "severity")
    EVENT_TYPES_FIELD_NUMBER: _ClassVar[int]
    SOURCE_FIELD_NUMBER: _ClassVar[int]
    SEVERITY_FIELD_NUMBER: _ClassVar[int]
    event_types: _containers.RepeatedScalarFieldContainer[str]
    source: str
    severity: str
    def __init__(self, event_types: _Optional[_Iterable[str]] = ..., source: _Optional[str] = ..., severity: _Optional[str] = ...) -> None: ...

class HealthStatus(_message.Message):
    __slots__ = ("healthy", "model_loaded", "vram_used_gb", "vram_total_gb", "latency_p50_ms", "latency_p99_ms", "error_rate")
    HEALTHY_FIELD_NUMBER: _ClassVar[int]
    MODEL_LOADED_FIELD_NUMBER: _ClassVar[int]
    VRAM_USED_GB_FIELD_NUMBER: _ClassVar[int]
    VRAM_TOTAL_GB_FIELD_NUMBER: _ClassVar[int]
    LATENCY_P50_MS_FIELD_NUMBER: _ClassVar[int]
    LATENCY_P99_MS_FIELD_NUMBER: _ClassVar[int]
    ERROR_RATE_FIELD_NUMBER: _ClassVar[int]
    healthy: bool
    model_loaded: bool
    vram_used_gb: float
    vram_total_gb: float
    latency_p50_ms: float
    latency_p99_ms: float
    error_rate: float
    def __init__(self, healthy: _Optional[bool] = ..., model_loaded: _Optional[bool] = ..., vram_used_gb: _Optional[float] = ..., vram_total_gb: _Optional[float] = ..., latency_p50_ms: _Optional[float] = ..., latency_p99_ms: _Optional[float] = ..., error_rate: _Optional[float] = ...) -> None: ...

class EngineHealth(_message.Message):
    __slots__ = ("engine", "status")
    ENGINE_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    engine: str
    status: HealthStatus
    def __init__(self, engine: _Optional[str] = ..., status: _Optional[_Union[HealthStatus, _Mapping]] = ...) -> None: ...
