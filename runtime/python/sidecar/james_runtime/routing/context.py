"""JAMES Runtime Context Management"""
import logging
import time
from typing import Optional, List, Dict, Any
from dataclasses import dataclass
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
    """Manages bounded KV cache and context windows for inference."""

    def __init__(self, config: Optional[ContextConfig] = None):
        self.config = config or ContextConfig()
        self.kv_caches: OrderedDict[str, KVCacheEntry] = OrderedDict()
        self.total_cache_bytes = 0

    def get_cache(self, model_id: str) -> Optional[Dict[str, Any]]:
        entry = self.kv_caches.get(model_id)
        if entry is None:
            return None
        self.kv_caches.move_to_end(model_id)
        entry.access_count += 1
        return {"tokens": entry.tokens, "embeddings": entry.embeddings}

    def set_cache(self, model_id: str, tokens: List[int], embeddings: Optional[List[float]] = None) -> None:
        size_bytes = len(tokens) * 4
        max_bytes = self.config.max_kv_cache_mb * 1024 * 1024

        old = self.kv_caches.pop(model_id, None)
        if old is not None:
            self.total_cache_bytes -= old.size_bytes

        # Never retain one entry that exceeds the complete cache budget.
        if size_bytes > max_bytes:
            logger.debug("Skipping KV cache for %s: %d > %d bytes", model_id, size_bytes, max_bytes)
            return

        while self.total_cache_bytes + size_bytes > max_bytes and self.kv_caches:
            self._evict_lru()

        entry = KVCacheEntry(
            model_id=model_id,
            tokens=tokens,
            embeddings=embeddings,
            timestamp=time.time(),
            size_bytes=size_bytes,
        )
        self.kv_caches[model_id] = entry
        self.total_cache_bytes += size_bytes

    def _evict_lru(self) -> None:
        if not self.kv_caches:
            return
        model_id, entry = self.kv_caches.popitem(last=False)
        self.total_cache_bytes -= entry.size_bytes
        logger.debug("Evicted KV cache for %s", model_id)

    def get_context_window(self, model_max_context: int, tokens: List[int]) -> List[int]:
        if model_max_context <= 0:
            return []
        if len(tokens) <= model_max_context:
            return tokens
        return tokens[-model_max_context:] if self.config.sliding_window else tokens[:model_max_context]

    def compress_context(self, tokens: List[int], target_length: int) -> List[int]:
        """Lossy fallback used only when semantic compression is unavailable."""
        if target_length <= 0:
            return []
        return tokens if len(tokens) <= target_length else tokens[-target_length:]

    def clear_cache(self, model_id: Optional[str] = None) -> None:
        if model_id:
            entry = self.kv_caches.pop(model_id, None)
            if entry is not None:
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

    @staticmethod
    def estimate_tokens(text: str) -> int:
        """Conservative pre-tokenizer estimate used for routing decisions."""
        if not text:
            return 0
        return max(1, (len(text) + 3) // 4)

    @classmethod
    def estimate_message_tokens(cls, messages: List[Dict[str, Any]]) -> int:
        total = 0
        for message in messages:
            total += 4
            total += cls.estimate_tokens(str(message.get("content", "")))
        return total
