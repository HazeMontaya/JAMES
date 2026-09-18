# JAMES Runtime Configuration
from enum import Enum
from pathlib import Path
from typing import List, Optional, Union
from pydantic import BaseModel, Field
import os

DEFAULT_CONFIG_PATH = Path(__file__).resolve().parents[2] / "config" / "runtime.yaml"


class QualityTier(str, Enum):
    UNSPECIFIED = "unspecified"
    FAST = "fast"
    BALANCED = "balanced"
    BEST = "best"


class PrivacyLevel(str, Enum):
    UNSPECIFIED = "unspecified"
    LOCAL_ONLY = "local_only"
    PREFER_LOCAL = "prefer_local"
    ALLOW_CLOUD = "allow_cloud"


class VLLMConfig(BaseModel):
    model_path: str = ".james/models/llama-3.1-8b-instruct"
    tp_size: int = 1
    gpu_mem_util: float = 0.9
    dtype: str = "auto"
    max_context: int = 8192
    quantization: Optional[str] = None
    enforce_eager: bool = False
    trust_remote_code: bool = True
    max_num_seqs: int = 256
    max_num_batched_tokens: int = 8192


class LlamaCppConfig(BaseModel):
    model_path: str = ".james/models/llama-3.1-8b-instruct-Q4_K_M.gguf"
    executable: str = "llama-server"
    working_directory: Optional[str] = None
    auto_start: bool = True
    n_gpu_layers: int = -1
    max_context: int = 8192
    port: int = 8080
    host: str = "127.0.0.1"
    parallel: int = 4
    flash_attn: bool = True
    mlock: bool = True
    n_threads: int = 0
    n_batch: int = 512
    n_ubatch: int = 512


class AirLLMConfig(BaseModel):
    model_path: str = ".james/models/llama-3.3-70b-instruct"
    vram_limit_gb: float = 4.0
    ram_limit_gb: float = 32.0
    max_context: int = 4096
    dtype: str = "float16"
    offload_buffers: bool = True


class RouterConfig(BaseModel):
    quality_weight: float = 0.4
    speed_weight: float = 0.2
    cost_weight: float = 0.2
    privacy_weight: float = 0.2
    default_quality_tier: str = "balanced"
    default_privacy: str = "prefer_local"
    prefer_local: bool = True


class BudgetPolicy(BaseModel):
    daily_ai_budget_usd: float = 0.0
    monthly_ai_budget_usd: float = 0.0
    max_single_request_usd: float = 0.01
    approved_models: List[str] = Field(default_factory=list)
    approved_providers: List[str] = Field(default_factory=lambda: ["local"])


class RuntimeSettings(BaseModel):
    # Server ports
    http_port: int = 38242
    grpc_port: int = 38243
    ws_port: int = 38244

    # Logging
    log_level: str = "INFO"
    log_format: str = "json"

    # Hardware
    vram_headroom: float = 0.1
    vram_poll_interval_ms: int = 5000

    # Paths
    models_dir: str = ".james/models"
    cache_dir: str = ".james/cache"
    db_path: str = ".james/runtime.db"

    # Engine configs
    vllm: VLLMConfig = Field(default_factory=VLLMConfig)
    llamacpp: LlamaCppConfig = Field(default_factory=LlamaCppConfig)
    airllm: AirLLMConfig = Field(default_factory=AirLLMConfig)

    # Router
    router: RouterConfig = Field(default_factory=RouterConfig)

    # Budget
    budget: BudgetPolicy = Field(default_factory=BudgetPolicy)

    @property
    def log_level_lower(self) -> str:
        return self.log_level.lower()


def load_settings_from_yaml(path: Union[str, Path]) -> RuntimeSettings:
    """Load runtime settings from a YAML file"""
    import yaml

    path = Path(path)
    if not path.exists():
        raise FileNotFoundError(f"Config file not found: {path}")

    with open(path, "r", encoding="utf-8") as f:
        data = yaml.safe_load(f) or {}

    return RuntimeSettings(**data)


def get_settings(path: Optional[Union[str, Path]] = None) -> RuntimeSettings:
    """Load settings from explicit path, DEFAULT_CONFIG_PATH, or defaults"""
    config_path = Path(path) if path else DEFAULT_CONFIG_PATH

    if config_path.exists():
        return load_settings_from_yaml(config_path)

    return RuntimeSettings()