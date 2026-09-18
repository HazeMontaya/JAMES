"""JAMES Runtime Context Management"""
import logging
from typing import Optional, List, Dict, Any
from dataclasses import dataclass, field
from collections import OrderedDict

logger = logging.getLogger(__name__)


@dataclass
class KVCacheEntry:
    model_id: str
    tokens: List[int]
    embeddings: Optional[List[float]] = None
    timestamp: float = 0.0
    access_count: int = 0
    size_bytes: int = 0


@dataclass
class ContextConfig:
    max_context_length: int = 8192
    max_kv_cache_mb: int = 512
    sliding_window: bool = True
    compression_enabled: bool = False
    compression_ratio: float = 0.5


class ContextManager:
    """Manages KV cache and context for inference"""
    
    def __init__(self, config: Optional[ContextConfig] = None):
        self.config = config or ContextConfig()
        self.kv_caches: OrderedDict[str, KVCacheEntry] = OrderedDict()
        self.total_cache_bytes = 0
    
    def get_cache(self, model_id: str) -> Optional[Dict[str, Any]]:
        """Get KV cache for a model"""
        if model_id in self.kv_caches:
            entry = self.kv_caches.pop(model_id)
            entry.access_count += 1
            self.kv_caches[model_id] = entry
            return {
                "tokens": entry.tokens,
                "embeddings": entry.embeddings,
            }
        return None
    
    def set_cache(self, model_id: str, tokens: List[int], embeddings: Optional[List[float]] = None) -> None:
        """Set KV cache for a model"""
        # Calculate size
        size_bytes = len(tokens) * 4  # rough estimate
        
        # Evict if needed
        while self.total_cache_bytes + len(tokens) * 4 > self.config.max_kv_cache_mb * 1024 * 1024:
            self._evict_lru()
        
        entry = KVCacheEntry(
            model_id=model_id,
            tokens=tokens,
            embeddings=embeddings,
            timestamp=time.time(),
            size_bytes=len(tokens) * 4,
        )
        
        if model_id in self.kv_caches:
            old_entry = self.kv_caches.pop(model_id)
            self.total_cache_bytes -= old_entry.size_bytes
        
        self.kv_caches[model_id] = entry
        self.total_cache_bytes += entry.size_bytes
    
    def _evict_lru(self) -> None:
        """Evict least recently used cache entry"""
        if not self.kv_caches:
            return
        model_id, entry = self.kv_caches.popitem(last=False)
        self.total_cache_bytes -= entry.size_bytes
        logger.debug(f"Evicted KV cache for {model_id}")
    
    def get_context_window(self, model_max_context: int, tokens: List[int]) -> List[int]:
        """Apply sliding window to fit context"""
        if len(tokens) <= model_max_context:
            return tokens
        
        if self.config.sliding_window:
            # Keep recent tokens
            return tokens[-model_max_context:]
        else:
            # Truncate from start
            return tokens[:model_max_context]
    
    def compress_context(self, tokens: List[int], target_length: int) -> List[int]:
        """Compress context (placeholder for future implementation)"""
        if len(tokens) <= target_length:
            return tokens
        # Future: implement semantic compression
        return tokens[-target_length:]
    
    def clear_cache(self, model_id: Optional[str] = None) -> None:
        """Clear cache for specific model or all"""
        if model_id:
            if model_id in self.kv_caches:
                entry = self.kv_caches.pop(model_id)
                self.total_cache_bytes -= entry.size_bytes
        else:
            self.kv_caches.clear()
            self.total_cache_bytes = 0
    
    def get_stats(self) -> dict:
        return {
            "num_cached_models": len(self.kv_caches),
            "total_cache_mb": self.total_cache_bytes / (1024 * 1024),
            "max_cache_mb": self.config.max_kv_cache_mb,
            "models": list(self.kv_caches.keys()),
        }


# Import time at module level
import time