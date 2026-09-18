"""JAMES Runtime Engine Registry"""
from typing import Dict, List, Optional, Any
from james_runtime.abstraction.base import InferenceEngine, HealthStatus
import logging

logger = logging.getLogger(__name__)


class EngineRegistry:
    """Registry for inference engines"""
    
    def __init__(self):
        self._engines: Dict[str, InferenceEngine] = {}
        self._health_cache: Dict[str, Any] = {}
    
    def register(self, engine: InferenceEngine) -> None:
        """Register an engine"""
        engine_type = engine.engine_type
        if engine_type in self._engines:
            raise ValueError(f"Engine {engine.engine_type} already registered")
        self._engines[engine.engine_type] = engine
        logger.info(f"Registered engine: {engine.engine_type}")
    
    def get(self, engine_type: str) -> Optional[InferenceEngine]:
        """Get engine by type"""
        return self._engines.get(engine_type)
    
    def list_all(self) -> List:
        """Get all registered engines"""
        return list(self._engines.values())
    
    def list_by_type(self, engine_type: str) -> List:
        """Get engines by type (for future multi-engine support)"""
        # Currently single engine per type
        engine = self._engines.get(engine_type)
        return [engine] if engine else []
    
    def list_healthy(self) -> List:
        """Get only healthy engines"""
        healthy = []
        for engine in self._engines.values():
            # Quick health check without full check
            if hasattr(engine, '_initialized') and getattr(engine, '_initialized', False):
                healthy.append(engine)
        return healthy
    
    async def check_all_health(self) -> Dict[str, Any]:
        """Check health of all engines"""
        results = {}
        for engine_type, engine in self._engines.items():
            try:
                health = await engine.health()
                results[engine_type] = health
            except Exception as e:
                logger.error(f"Health check failed for {engine_type}: {e}")
                results[engine_type] = None
        return results
    
    def get_engine_types(self) -> List[str]:
        """Get list of registered engine types"""
        return list(self._engines.keys())
    
    def has_engine(self, engine_type: str) -> bool:
        """Check if engine type is registered"""
        return engine_type in self._engines
    
    def unregister(self, engine_type: str) -> bool:
        """Unregister an engine"""
        if engine_type in self._engines:
            del self._engines[engine_type]
            return True
        return False