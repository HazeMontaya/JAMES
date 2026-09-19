"""Optional HTTP adapters for external agent ecosystems.

These adapters use JAMES' existing httpx dependency and are deliberately
thin: external services execute only after a JAMES mission/capability has
authorized the call.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping

import httpx


@dataclass
class HttpIntegrationError(RuntimeError):
    provider: str
    status_code: int | None
    detail: str

    def __str__(self) -> str:
        return f"{self.provider} integration failed ({self.status_code}): {self.detail}"


class _HttpAdapter:
    def __init__(self, *, base_url: str, api_key: str | None = None, timeout: float = 30.0) -> None:
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.timeout = timeout

    def _headers(self) -> dict[str, str]:
        return {"Authorization": f"Bearer {self.api_key}"} if self.api_key else {}

    async def _request(self, method: str, path: str, **kwargs: Any) -> dict[str, Any]:
        headers = self._headers()
        headers.update(kwargs.pop("headers", {}))
        try:
            async with httpx.AsyncClient(base_url=self.base_url, timeout=self.timeout) as client:
                response = await client.request(method, path, headers=headers, **kwargs)
        except httpx.HTTPError as exc:
            raise HttpIntegrationError(self.__class__.__name__, None, str(exc)) from exc
        if response.is_error:
            raise HttpIntegrationError(
                self.__class__.__name__, response.status_code, response.text[:1000]
            )
        if not response.content:
            return {}
        try:
            data = response.json()
        except ValueError as exc:
            raise HttpIntegrationError(self.__class__.__name__, response.status_code, "invalid JSON") from exc
        return data if isinstance(data, dict) else {"data": data}


class FirecrawlAdapter(_HttpAdapter):
    """Firecrawl-compatible web research adapter."""

    async def search(self, query: str, *, limit: int = 10) -> list[dict[str, Any]]:
        payload = await self._request(
            "POST", "/v2/search", json={"query": query, "limit": limit}
        )
        results = payload.get("data", payload.get("results", []))
        return results if isinstance(results, list) else []

    async def scrape(self, url: str) -> dict[str, Any]:
        return await self._request("POST", "/v2/scrape", json={"url": url})

    async def crawl(self, url: str, *, limit: int = 10) -> dict[str, Any]:
        return await self._request("POST", "/v2/crawl", json={"url": url, "limit": limit})


class DifyAdapter(_HttpAdapter):
    async def run_workflow(self, workflow_id: str, inputs: Mapping[str, Any], *, user: str = "james") -> dict[str, Any]:
        return await self._request(
            "POST", "/v1/workflows/run",
            json={"inputs": dict(inputs), "response_mode": "blocking", "user": user, "workflow_id": workflow_id},
        )

    async def run_agent(self, query: str, *, user: str = "james", inputs: Mapping[str, Any] | None = None) -> dict[str, Any]:
        return await self._request(
            "POST", "/v1/chat-messages",
            json={"inputs": dict(inputs or {}), "query": query, "response_mode": "blocking", "user": user},
        )


class N8nAdapter(_HttpAdapter):
    async def trigger_webhook(self, webhook_path: str, payload: Mapping[str, Any]) -> dict[str, Any]:
        return await self._request("POST", f"/webhook/{webhook_path.lstrip('/')}", json=dict(payload))

    async def execute_webhook_test(self, webhook_path: str, payload: Mapping[str, Any]) -> dict[str, Any]:
        return await self._request("POST", f"/webhook-test/{webhook_path.lstrip('/')}", json=dict(payload))


class CrewAIAdapter(_HttpAdapter):
    """Adapter for a deployed CrewAI Flow/Crew HTTP gateway."""

    async def run(self, endpoint: str, payload: Mapping[str, Any]) -> dict[str, Any]:
        return await self._request("POST", endpoint, json=dict(payload))


class LiveKitAdapter(_HttpAdapter):
    """Control-plane adapter; realtime media remains in LiveKit's native SDK."""

    async def health(self) -> dict[str, Any]:
        return await self._request("GET", "/")
