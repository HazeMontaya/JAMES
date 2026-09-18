"""World Access facade - exposes routed async methods for the skill engine"""

from typing import Any, cast

import httpx
import structlog

from .adapters.web_fetch import fetch_readable
from .capability import Backend, BackendType, Capability, CapabilityRegistry, CapabilityRequest

logger = structlog.get_logger()


class WorldAccess:
    """Facade for JAMES World Access Layer."""

    def __init__(self) -> None:
        self._registry = CapabilityRegistry()
        self._register_defaults()

    def _register_defaults(self) -> None:
        self._register_web_search()
        self._register_http_fetch()

    def _register_web_search(self) -> None:
        from .adapters.web_search import build_web_search_capability

        cap = build_web_search_capability()
        for b in cap.backends:
            b.health_check = lambda: self._health_http("https://r.jina.ai/")
            b.execute = self._search_execute
        self._registry.register(cap)

    def _register_http_fetch(self) -> None:
        cap = Capability(name="web_fetch", description="Fetch and read a web page")
        cap.add_backend(
            Backend(
                id="httpx_reader",
                type=BackendType.HTTP,
                priority=1,
                enabled=True,
                health_check=lambda: self._health_http("https://example.com/"),
                execute=self._fetch_execute,
            )
        )
        self._registry.register(cap)

    async def _health_http(self, url: str) -> bool:
        try:
            async with httpx.AsyncClient(timeout=5) as client:
                resp = await client.get(url)
                return resp.status_code < 500
        except Exception:
            return False

    async def _search_execute(self, req: CapabilityRequest) -> dict[str, Any]:
        async with httpx.AsyncClient(timeout=20, follow_redirects=True) as client:
            if req.action == "read":
                url = req.params.get("url", "")
                if not url:
                    raise ValueError("url required for jina_reader")
                resp = await client.get(f"https://r.jina.ai/{url}")
                if resp.status_code >= 500:
                    raise RuntimeError(f"Jina unavailable: {resp.status_code}")
                return {"source": "jina", "url": url, "content": resp.text[:20000]}
            else:
                import urllib.parse

                query = req.params.get("query", "")
                url = f"https://html.duckduckgo.com/html/?q={urllib.parse.quote(query)}"
                resp = await client.get(url)
                resp.raise_for_status()
                return {"source": "ddg", "query": query, "content": resp.text[:20000]}

    async def _fetch_execute(self, req: CapabilityRequest) -> dict[str, Any]:
        url = req.params.get("url", "")
        if not url:
            raise ValueError("url required for web_fetch")
        return await fetch_readable(url)

    @property
    def registry(self) -> CapabilityRegistry:
        return self._registry

    async def on_start(self) -> None:
        await self._registry.check_health()

    # --- Facade methods used by SkillExecutor._call_tool ---

    async def web_search(self, query: str = "", **kwargs: Any) -> dict[str, Any]:
        return cast(dict[str, Any], (await self._registry.execute(CapabilityRequest(capability="web_search", action="search", params={"query": query, **kwargs}))).data)

    async def web_fetch(self, url: str = "", **kwargs: Any) -> dict[str, Any]:
        return cast(dict[str, Any], (await self._registry.execute(CapabilityRequest(capability="web_fetch", action="read", params={"url": url, **kwargs}))).data)

    async def http_fetch(self, url: str = "") -> dict[str, Any]:
        async with httpx.AsyncClient(timeout=20, follow_redirects=True) as client:
            resp = await client.get(url)
            resp.raise_for_status()
            return {"url": url, "status": resp.status_code, "content": resp.text[:50000]}

    def capability_summary(self) -> dict[str, Any]:
        return self._registry.to_dict()
