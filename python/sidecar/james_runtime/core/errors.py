"""JAMES Runtime Errors"""
from typing import Optional, List, Dict, Any
from dataclasses import dataclass


class RuntimeError(Exception):
    """Base exception for JAMES Runtime"""
    def __init__(self, message: str, error_code: str = "RUNTIME_ERROR", details: Optional[Dict[str, Any]] = None):
        super().__init__(message)
        self.message = message
        self.error_code = error_code
        self.details = details or {}


class OOMError(RuntimeError):
    """Out of memory error"""
    def __init__(self, message: str, required_gb: float, available_gb: float, engine: str = ""):
        super().__init__(message, "OUT_OF_MEMORY", {
            "required_gb": required_gb,
            "available_gb": available_gb,
            "engine": engine,
        })
        self.required_gb = required_gb
        self.available_gb = available_gb
        self.engine = engine


class ModelUnavailableError(RuntimeError):
    """Model not available"""
    def __init__(self, model_id: str, reason: str = ""):
        super().__init__(f"Model {model_id} unavailable: {reason}", "MODEL_UNAVAILABLE", {
            "model_id": model_id,
            "reason": reason,
        })
        self.model_id = model_id
        self.reason = reason


class NoSuitableModelError(RuntimeError):
    """No model matches routing requirements"""
    def __init__(self, routing_request: Dict[str, Any]):
        super().__init__("No suitable model found for request", "NO_SUITABLE_MODEL", routing_request)
        self.routing_request = routing_request


class NoSuitableEngineError(RuntimeError):
    """No engine can run the model on current hardware"""
    def __init__(self, model_id: str, hardware: Dict[str, Any]):
        super().__init__(f"No engine can run {model_id} on current hardware", "NO_SUITABLE_ENGINE", {
            "model_id": model_id,
            "hardware": hardware,
        })
        self.model_id = model_id
        self.hardware = hardware


class EngineNotFoundError(RuntimeError):
    """Engine type not registered"""
    def __init__(self, engine_type: str):
        super().__init__(f"Engine {engine_type} not found", "ENGINE_NOT_FOUND", {
            "engine_type": engine_type,
        })


class EngineInitializationError(RuntimeError):
    """Engine failed to initialize"""
    def __init__(self, engine_type: str, message: str):
        super().__init__(f"Engine {engine_type} initialization failed: {message}", "ENGINE_INIT_FAILED", {
            "engine_type": engine_type,
            "original_message": message,
        })


class EngineNotReadyError(RuntimeError):
    """Engine not initialized"""
    def __init__(self, engine_type: str):
        super().__init__(f"Engine {engine_type} not initialized", "ENGINE_NOT_READY", {
            "engine_type": engine_type,
        })


class BudgetExceededError(RuntimeError):
    """Budget limit exceeded"""
    def __init__(self, budget_type: str, current: float, limit: float):
        super().__init__(f"{budget_type} budget exceeded: ${current:.4f} / ${limit:.4f}", "BUDGET_EXCEEDED", {
            "budget_type": budget_type,
            "current": current,
            "limit": limit,
        })


class ModelNotApprovedError(RuntimeError):
    """Model not in approved list"""
    def __init__(self, model_id: str, approved: List[str]):
        super().__init__(f"Model {model_id} not in approved list", "MODEL_NOT_APPROVED", {
            "model_id": model_id,
            "approved_models": approved,
        })


class QuantizationNotSupportedError(RuntimeError):
    """Engine doesn't support requested quantization"""
    def __init__(self, engine: str, quantization: str, supported: List[str]):
        super().__init__(f"Engine {engine} doesn't support quantization {quantization}", "QUANTIZATION_NOT_SUPPORTED", {
            "engine": engine,
            "quantization": quantization,
            "supported": supported,
        })


class HardwareDetectionError(RuntimeError):
    """Failed to detect hardware"""
    def __init__(self, message: str):
        super().__init__(f"Hardware detection failed: {message}", "HARDWARE_DETECTION_FAILED", {
            "original_message": message,
        })


class VRAMExhaustedError(RuntimeError):
    """VRAM exhausted during inference"""
    def __init__(self, engine: str, model: str, current_gb: float, total_gb: float):
        super().__init__(f"VRAM exhausted on {engine} for {model}", "VRAM_EXHAUSTED", {
            "engine": engine,
            "model": model,
            "current_gb": current_gb,
            "total_gb": total_gb,
        })


class EngineHealthError(RuntimeError):
    """Engine health check failed"""
    def __init__(self, engine: str, message: str):
        super().__init__(f"Engine {engine} health check failed: {message}", "ENGINE_HEALTH_FAILED", {
            "engine": engine,
            "message": message,
        })


class InferenceTimeoutError(RuntimeError):
    """Inference exceeded timeout"""
    def __init__(self, request_id: str, timeout_ms: int):
        super().__init__(f"Inference timeout after {timeout_ms}ms", "INFERENCE_TIMEOUT", {
            "request_id": request_id,
            "timeout_ms": timeout_ms,
        })


class ToolCallError(RuntimeError):
    """Tool call execution failed"""
    def __init__(self, tool_name: str, message: str):
        super().__init__(f"Tool {tool_name} failed: {message}", "TOOL_CALL_FAILED", {
            "tool_name": tool_name,
            "message": message,
        })


class RoutingError(RuntimeError):
    """Routing decision failed"""
    def __init__(self, message: str, context: Dict[str, Any]):
        super().__init__(f"Routing failed: {message}", "ROUTING_ERROR", context)


class ConfigurationError(RuntimeError):
    """Configuration error"""
    def __init__(self, message: str, config_key: str = ""):
        super().__init__(f"Configuration error: {message}", "CONFIG_ERROR", {
            "config_key": config_key,
        })