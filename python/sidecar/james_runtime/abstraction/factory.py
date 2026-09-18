"""JAMES Runtime Engine Factory"""
from typing import Optional
from james_runtime.config import RuntimeSettings, VLLMConfig, LlamaCppConfig, AirLLMConfig
from james_runtime.abstraction.base import InferenceEngine, EngineConfig
from james_runtime.models.registry import ModelSpec
from james_runtime.core.errors import EngineInitializationError


class EngineFactory:
    """Factory for creating inference engines"""
    
    def __init__(self, settings: RuntimeSettings):
        self.settings = settings
        self._engine_classes = {
            "vllm": "james_runtime.abstraction.vllm_engine.VLLMEngine",
            "llamacpp": "james_runtime.abstraction.llamacpp_engine.LlamaCppEngine",
            "airllm": "james_runtime.abstraction.airllm_engine.AirLLMEngine",
        }
    
    def create(self, engine_type: str, config) -> InferenceEngine:
        """Create engine instance"""
        if engine_type not in self._engine_classes:
            raise ValueError(f"Unknown engine type: {engine_type}")
        
        class_path = self._engine_classes[engine_type]
        module_path, class_name = class_path.rsplit(".", 1)
        
        try:
            module = __import__(module_path, fromlist=[class_name])
            engine_class = getattr(module, class_name)
            return engine_class(config)
        except ImportError as e:
            raise EngineInitializationError(engine_type, f"Failed to import engine class: {e}")
        except AttributeError as e:
            raise EngineInitializationError(engine_type, f"Engine class not found: {e}")
    
    def get_supported_engines(self) -> list:
        """Get list of supported engine types"""
        return list(self._engine_classes.keys())
    
    def get_config_class(self, engine_type: str):
        """Get config class for engine type"""
        config_map = {
            "vllm": VLLMConfig,
            "llamacpp": "james_runtime.config.LlamaCppConfig",
            "airllm": "james_runtime.config.AirLLMConfig",
        }
        if engine_type not in config_map:
            raise ValueError(f"Unknown engine type: {engine_type}")
        return config_map[engine_type]