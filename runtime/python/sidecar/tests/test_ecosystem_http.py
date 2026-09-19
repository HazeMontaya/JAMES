import pytest

from james_runtime.integration.http_adapters import (
    CrewAIAdapter, DifyAdapter, FirecrawlAdapter, LiveKitAdapter, N8nAdapter
)


@pytest.mark.asyncio
async def test_firecrawl_adapter_uses_expected_endpoint(monkeypatch):
    adapter = FirecrawlAdapter(base_url="https://example.invalid", api_key="test")
    calls = {}

    async def fake_request(method, path, **kwargs):
        calls.update(method=method, path=path, kwargs=kwargs)
        return {"data": [{"url": "https://example.com"}]}

    monkeypatch.setattr(adapter, "_request", fake_request)
    result = await adapter.search("JAMES", limit=3)
    assert calls["method"] == "POST"
    assert calls["path"] == "/v2/search"
    assert calls["kwargs"]["json"]["limit"] == 3
    assert result[0]["url"].endswith("example.com")


@pytest.mark.asyncio
async def test_provider_adapters_keep_calls_structured(monkeypatch):
    adapters = [
        (DifyAdapter(base_url="https://dify.invalid"), "run_agent", {"query": "hello"}),
        (N8nAdapter(base_url="https://n8n.invalid"), "trigger_webhook", {"webhook_path": "james", "payload": {"event": "test"}}),
        (CrewAIAdapter(base_url="https://crew.invalid"), "run", {"endpoint": "/flow", "payload": {"x": 1}}),
        (LiveKitAdapter(base_url="https://livekit.invalid"), "health", {}),
    ]
    for adapter, method_name, kwargs in adapters:
        called = {}

        async def fake_request(method, path, **request_kwargs):
            called.update(method=method, path=path, kwargs=request_kwargs)
            return {"ok": True}

        monkeypatch.setattr(adapter, "_request", fake_request)
        result = await getattr(adapter, method_name)(**kwargs)
        assert result == {"ok": True}
        assert called["method"] in {"GET", "POST"}
