"""Web search adapters - DuckDuckGo, Jina, fallback chain"""

from typing import Any

import httpx

from ..capability.models import Backend, BackendType, Capability


async def _ddg_search(params: dict[str, Any]) -> dict[str, Any]:
    import urllib.parse

    query = params.get("query", "")
    url = f"https://html.duckduckgo.com/html/?q={urllib.parse.quote(query)}"
    async with httpx.AsyncClient(timeout=15, follow_redirects=True) as client:
        resp = await client.get(url)
        resp.raise_for_status()
        return {"query": query, "status": resp.status_code, "html_len": len(resp.text)}


async def _jina_read(params: dict[str, Any]) -> dict[str, Any]:
    url = params.get("url", "")
    async with httpx.AsyncClient(timeout=30, follow_redirects=True) as client:
        resp = await client.get(f"https://r.jina.ai/{url}")
        if resp.status_code >= 500:
            raise RuntimeError(f"Jina reader unavailable: {resp.status_code}")
        return {"url": url, "status": resp.status_code, "text_len": len(resp.text)}


def build_web_search_capability() -> Capability:
    cap = Capability(name="web_search", description="Web search and page reading")
    cap.add_backend(
        Backend(
            id="jina_reader",
            type=BackendType.HTTP,
            priority=2,
            enabled=True,
        )
    )
    cap.add_backend(
        Backend(
            id="ddg_html",
            type=BackendType.HTTP,
            priority=1,
            enabled=True,
        )
    )
    return cap
