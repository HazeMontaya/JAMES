"""JAMES Runtime Events"""
from pydantic import BaseModel, Field
from typing import Optional, Dict, Any, Literal
from datetime import datetime
from uuid import UUID, uuid4


class RuntimeEvent(BaseModel):
    event_type: str
    timestamp: datetime = Field(default_factory=datetime.utcnow)
    event_id: str = Field(default_factory=lambda: str(uuid4()))
    correlation_id: Optional[str] = None
    causation_id: Optional[str] = None
    payload: Dict[str, Any] = Field(default_factory=dict)
    source: str = "james_runtime"
    severity: Literal["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"] = "INFO"


class ModelSelectedEvent(RuntimeEvent):
    event_type: str = "MODEL_SELECTED"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "model_id": "",
        "task": "",
        "quality_tier": "",
        "privacy": "",
        "reasoning": "",
    })


class RuntimeChangedEvent(RuntimeEvent):
    event_type: str = "RUNTIME_CHANGED"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "from_engine": "",
        "to_engine": "",
        "model_id": "",
        "reason": "",
    })


class ModelLoadingEvent(RuntimeEvent):
    event_type: str = "MODEL_LOADING"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "model_id": "",
        "engine": "",
        "estimated_vram_gb": 0.0,
    })


class ModelReadyEvent(RuntimeEvent):
    event_type: str = "MODEL_READY"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "model_id": "",
        "engine": "",
        "actual_vram_gb": 0.0,
        "load_time_ms": 0,
    })


class InferenceStartedEvent(RuntimeEvent):
    event_type: str = "INFERENCE_STARTED"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "request_id": "",
        "model_id": "",
        "engine": "",
        "tokens_estimated": 0,
    })


class TokenStreamEvent(RuntimeEvent):
    event_type: str = "TOKEN_STREAM"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "request_id": "",
        "token": "",
        "token_index": 0,
        "is_final": False,
    })


class ToolCallEvent(RuntimeEvent):
    event_type: str = "TOOL_CALL"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "tool_name": "",
        "arguments": {},
        "tool_call_id": "",
        "caller": "",
    })


class InferenceCompletedEvent(RuntimeEvent):
    event_type: str = "INFERENCE_COMPLETED"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "request_id": "",
        "model_id": "",
        "engine": "",
        "tokens_generated": 0,
        "duration_ms": 0,
        "cost_usd": 0.0,
    })


class ModelFallbackEvent(RuntimeEvent):
    event_type: str = "MODEL_FALLBACK"
    severity: Literal["WARNING", "ERROR"] = "WARNING"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "from_model": "",
        "to_model": "",
        "reason": "",
        "fallback_chain": [],
    })


class VRAMPressureEvent(RuntimeEvent):
    event_type: str = "VRAM_PRESSURE"
    severity: Literal["WARNING", "ERROR", "CRITICAL"] = "WARNING"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "current_vram_gb": 0.0,
        "total_vram_gb": 0.0,
        "utilization_percent": 0.0,
        "action_taken": "",
    })


class OOMRecoveryEvent(RuntimeEvent):
    event_type: str = "OOM_RECOVERY"
    severity: Literal["ERROR", "CRITICAL"] = "ERROR"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "failed_engine": "",
        "failed_model": "",
        "recovery_action": "",
        "success": False,
    })


class CostRecordedEvent(RuntimeEvent):
    event_type: str = "COST_RECORDED"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "model": "",
        "tokens_in": 0,
        "tokens_out": 0,
        "cost_usd": 0.0,
        "engine": "",
        "agent_id": "",
        "goal_id": "",
    })


class BudgetAlertEvent(RuntimeEvent):
    event_type: str = "BUDGET_ALERT"
    severity: Literal["WARNING", "ERROR"] = "WARNING"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "budget_type": "",
        "current_spend_usd": 0.0,
        "limit_usd": 0.0,
        "percentage": 0.0,
    })


class QualityEvaluatedEvent(RuntimeEvent):
    event_type: str = "QUALITY_EVALUATED"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "model_id": "",
        "benchmark": "",
        "score": 0.0,
        "passed": True,
    })


class HardwareDetectedEvent(RuntimeEvent):
    event_type: str = "HARDWARE_DETECTED"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "gpu_name": "",
        "vram_gb": 0.0,
        "cuda_version": "",
        "metal_support": False,
        "cpu_cores": 0,
        "ram_gb": 0.0,
    })


class EngineHealthEvent(RuntimeEvent):
    event_type: str = "ENGINE_HEALTH"
    payload: Dict[str, Any] = Field(default_factory=lambda: {
        "engine": "",
        "healthy": True,
        "vram_used_gb": 0.0,
        "vram_total_gb": 0.0,
        "latency_p50_ms": 0.0,
        "error_rate": 0.0,
    })


# Event factory functions
def create_model_selected(model_id: str, task: str, quality_tier: str, privacy: str, reasoning: str) -> ModelSelectedEvent:
    return ModelSelectedEvent(payload={
        "model_id": model_id,
        "task": task,
        "quality_tier": quality_tier,
        "privacy": privacy,
        "reasoning": reasoning,
    })


def create_runtime_changed(from_engine: str, to_engine: str, model_id: str, reason: str) -> RuntimeChangedEvent:
    return RuntimeChangedEvent(payload={
        "from_engine": from_engine,
        "to_engine": to_engine,
        "model_id": model_id,
        "reason": reason,
    })


def create_model_loading(model_id: str, engine: str, estimated_vram_gb: float) -> ModelLoadingEvent:
    return ModelLoadingEvent(payload={
        "model_id": model_id,
        "engine": engine,
        "estimated_vram_gb": estimated_vram_gb,
    })


def create_model_ready(model_id: str, engine: str, actual_vram_gb: float, load_time_ms: int) -> ModelReadyEvent:
    return ModelReadyEvent(payload={
        "model_id": model_id,
        "engine": engine,
        "actual_vram_gb": actual_vram_gb,
        "load_time_ms": load_time_ms,
    })


def create_inference_started(request_id: str, model_id: str, engine: str, tokens_estimated: int = 0) -> InferenceStartedEvent:
    return InferenceStartedEvent(payload={
        "request_id": request_id,
        "model_id": model_id,
        "engine": engine,
        "tokens_estimated": tokens_estimated,
    })


def create_token_stream(request_id: str, token: str, token_index: int, is_final: bool = False) -> TokenStreamEvent:
    return TokenStreamEvent(payload={
        "request_id": request_id,
        "token": token,
        "token_index": token_index,
        "is_final": is_final,
    })


def create_inference_completed(request_id: str, model_id: str, engine: str, tokens_generated: int, duration_ms: int, cost_usd: float = 0.0) -> InferenceCompletedEvent:
    return InferenceCompletedEvent(payload={
        "request_id": request_id,
        "model_id": model_id,
        "engine": engine,
        "tokens_generated": tokens_generated,
        "duration_ms": duration_ms,
        "cost_usd": cost_usd,
    })


def create_vram_pressure(current_vram_gb: float, total_vram_gb: float, action_taken: str) -> VRAMPressureEvent:
    return VRAMPressureEvent(
        severity="CRITICAL" if current_vram_gb / total_vram_gb > 0.95 else "WARNING",
        payload={
            "current_vram_gb": current_vram_gb,
            "total_vram_gb": total_vram_gb,
            "utilization_percent": (current_vram_gb / total_vram_gb) * 100,
            "action_taken": action_taken,
        }
    )


def create_oom_recovery(failed_engine: str, failed_model: str, recovery_action: str, success: bool) -> OOMRecoveryEvent:
    return OOMRecoveryEvent(payload={
        "failed_engine": failed_engine,
        "failed_model": failed_model,
        "recovery_action": recovery_action,
        "success": success,
    })


def create_cost_recorded(model: str, tokens_in: int, tokens_out: int, cost_usd: float, engine: str, agent_id: str = "", goal_id: str = "") -> CostRecordedEvent:
    return CostRecordedEvent(payload={
        "model": model,
        "tokens_in": tokens_in,
        "tokens_out": tokens_out,
        "cost_usd": cost_usd,
        "engine": engine,
        "agent_id": agent_id,
        "goal_id": goal_id,
    })


def create_budget_alert(budget_type: str, current_spend: float, limit: float) -> BudgetAlertEvent:
    return BudgetAlertEvent(
        severity="ERROR" if current_spend >= limit else "WARNING",
        payload={
            "budget_type": budget_type,
            "current_spend_usd": current_spend,
            "limit_usd": limit,
            "percentage": (current_spend / limit) * 100 if limit > 0 else 0,
        }
    )