from james_runtime.routing.context import ContextConfig, ContextManager


def test_oversized_cache_entry_is_not_retained():
    manager = ContextManager(ContextConfig(max_kv_cache_mb=1))
    manager.set_cache("huge", list(range(300_000)))
    assert manager.get_cache("huge") is None
    assert manager.total_cache_bytes == 0


def test_lru_eviction_respects_budget():
    manager = ContextManager(ContextConfig(max_kv_cache_mb=1))
    manager.set_cache("a", list(range(100_000)))
    manager.set_cache("b", list(range(100_000)))
    manager.set_cache("c", list(range(100_000)))
    assert manager.total_cache_bytes <= 1024 * 1024
    assert manager.get_cache("a") is None
    assert manager.get_cache("c") is not None


def test_context_window_keeps_recent_tokens():
    manager = ContextManager(ContextConfig(sliding_window=True))
    tokens = list(range(100))
    assert manager.get_context_window(10, tokens) == list(range(90, 100))


def test_token_estimator_is_deterministic():
    assert ContextManager.estimate_tokens("") == 0
    assert ContextManager.estimate_tokens("abcd") == 1
    assert ContextManager.estimate_message_tokens(
        [{"role": "user", "content": "abcd"}]
    ) == 5
