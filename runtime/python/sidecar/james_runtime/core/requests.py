"""JAMES Runtime Core Request/Response Types"""
from pydantic import BaseModel, Field
from typing import Optional, List, Dict, Any, AsyncGenerator, Literal
from datetime import datetime
from uuid import UUID, uuid4
from enum import Enum
import json


class ChatMessage(BaseModel):
    role: Literal["system", "user", "assistant", "tool"]
    content: str
    name: Optional[str] = None
    tool_calls: Optional[List[Dict[str, Any]]] = None
    tool_call_id: Optional[str] = None


class Tool(BaseModel):
    type: Literal["function"] = "function"
    function: Dict[str, Any]


class CompletionRequest(BaseModel):
    model: str
    messages: List[ChatMessage]
    temperature: float = Field(0.7, ge=0.0, le=2.0)
    max_tokens: Optional[int] = Field(None, ge=1, le=131072)
    stream: bool = False
    tools: Optional[List[Tool]] = None
    tool_choice: Optional[str] = None
    top_p: float = Field(1.0, ge=0.0, le=1.0)
    top_k: Optional[int] = Field(None, ge=1)
    stop: Optional[List[str]] = None
    presence_penalty: float = Field(0.0, ge=-2.0, le=2.0)
    frequency_penalty: float = Field(0.0, ge=-2.0, le=2.0)
    response_format: Optional[Dict[str, Any]] = None
    runtime_hint: Optional[str] = None  # Engine type hint from router
    request_id: str = Field(default_factory=lambda: str(uuid4()))


class StreamingRequest(CompletionRequest):
    stream: bool = True


class Chunk(BaseModel):
    id: str
    model: str
    created: int
    choices: List[Dict[str, Any]]


class CompletionResponse(BaseModel):
    id: str
    model: str
    created: int
    choices: List[Dict[str, Any]]
    usage: Optional[Dict[str, Any]] = None
    runtime: Optional[str] = None  # Which engine executed
    routing_decision: Optional[Dict[str, Any]] = None  # For tracing


class ToolCallRequest(BaseModel):
    tool_name: str
    arguments: Dict[str, Any]
    tool_call_id: str
    caller: str
    required_capabilities: List[str] = Field(default_factory=list)
    quality_tier: Literal["fast", "balanced", "best"] = "balanced"
    privacy: Literal["local_only", "prefer_local", "allow_cloud"] = "prefer_local"


class ToolCallResult(BaseModel):
    tool_call_id: str
    result: Any
    error: Optional[str] = None
    runtime: Optional[str] = None
    duration_ms: int


class RoutingRequest(BaseModel):
    task: str
    required_capabilities: List[str] = Field(default_factory=list)
    quality_tier: Literal["fast", "balanced", "best"] = "balanced"
    privacy: Literal["local_only", "prefer_local", "allow_cloud"] = "prefer_local"
    latency_budget_ms: Optional[int] = None
    cost_budget_per_1k: Optional[float] = None
    context_length_needed: Optional[int] = None
    preferred_model: Optional[str] = None
    agent_id: Optional[str] = None
    goal_id: Optional[str] = None


class ModelCapability(str, Enum):
    REASONING = "reasoning"
    CODING = "coding"
    PLANNING = "planning"
    TOOL_USE = "tool_use"
    ANALYSIS = "analysis"
    SUMMARIZATION = "summarization"
    TRANSLATION = "translation"
    CLASSIFICATION = "classification"
    EXTRACTION = "extraction"
    CREATIVE_WRITING = "creative_writing"
    MATH = "math"
    MULTILINGUAL = "multilingual"


class RoutingDecision(BaseModel):
    model_id: str
    provider: str
    reasoning: str
    fallback_chain: List[str] = Field(default_factory=list)
    estimated_cost_per_1k: Optional[float] = None
    estimated_latency_ms: Optional[int] = None
    required_capabilities: List[str] = Field(default_factory=list)


class RuntimeRequest(BaseModel):
    model_id: str
    quantization: Optional[str] = None
    offload: bool = False
    max_latency_ms: Optional[int] = None
    max_vram_gb: Optional[float] = None


class EngineType(str, Enum):
    VLLM = "vllm"
    LLAMACPP = "llamacpp"
    AIRLLM = "airllm"


class RoutingConstraints(BaseModel):
    """Hardware / budget constraints for runtime selection"""
    max_vram_gb: Optional[float] = None
    max_latency_ms: Optional[int] = None
    max_cost_per_1k: Optional[float] = None
    min_quality_tier: Literal["fast", "balanced", "best"] = "fast"
    allow_cloud: bool = True
    preferred_engine: Optional[str] = None


class EngineSelection(BaseModel):
    engine_type: EngineType
    model_id: str
    quantization: Optional[str] = None
    offload: bool = False
    estimated_vram_gb: float
    estimated_latency_ms: int
    fallback_engines: List[EngineType] = Field(default_factory=list)


class FitResult(BaseModel):
    fits: bool
    engine: EngineType
    quantization: Optional[str] = None
    offload: bool = False
    required_vram_gb: float
    available_vram_gb: float
    reason: Optional[str] = None