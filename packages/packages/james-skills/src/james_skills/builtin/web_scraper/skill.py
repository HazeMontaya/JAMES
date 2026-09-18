"""Web Scraper skill - Python entry point (world-tool aware, deterministic fallback)"""

import contextlib
import urllib.request
from html.parser import HTMLParser
from typing import Any

TIMEOUT = 10
MAX_CHARS = 6000


class _TextExtractor(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self._parts: list[str] = []
        self._skip = False

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in ("script", "style", "noscript", "svg", "nav", "footer", "head"):
            self._skip = True
        elif tag in ("p", "h1", "h2", "h3", "h4", "li", "br", "div"):
            self._parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if tag in ("script", "style", "noscript", "svg", "nav", "footer", "head"):
            self._skip = False

    def handle_data(self, data: str) -> None:
        if not self._skip:
            text = " ".join(data.split())
            if text:
                self._parts.append(text)

    def text(self) -> str:
        return " ".join(" ".join(self._parts).split())


def _extract_text(html: str) -> str:
    parser = _TextExtractor()
    with contextlib.suppress(Exception):
        parser.feed(html)
    return parser.text()


async def _fetch_via_urlopen(url: str) -> str:
    with urllib.request.urlopen(url, timeout=TIMEOUT) as resp:
        return resp.read(MAX_CHARS * 4).decode("utf-8", errors="replace")


async def execute(inputs: dict[str, Any]) -> dict[str, Any]:
    url = inputs.get("url", "").strip()
    query = inputs.get("query", inputs.get("instructions", "")).strip()
    context = inputs.get("context")

    if not url.lower().startswith(("http://", "https://")):
        return {"text": f"Invalid or missing URL: {url}", "model": "local_fallback", "fallback": True, "success": False}

    content = ""
    error = ""
    source = "local"
    world = getattr(context, "world", None) if context is not None else None
    try:
        if world is not None and hasattr(world, "web_fetch"):
            result = await world.web_fetch(url=url)
            content = str(result.get("content", "")).strip()[:MAX_CHARS]
            source = result.get("source") or result.get("format") or "world"
            error = str(result.get("error", "") or "").strip()
        else:
            raw = await _fetch_via_urlopen(url)
            content = _extract_text(raw)[:MAX_CHARS]
    except Exception as e:
        error = str(e)

    if not content and not error:
        error = "No extractable text content found on page"

    lines = [
        f"# Web Scrape: {url}",
        "",
    ]
    if query:
        lines.append(f"## Query\n{query}")
        lines.append("")
    if error:
        lines.append(f"## Error\n{error}")
    elif content:
        lines.append("## Extracted Content")
        lines.append(content)

    return {
        "text": "\n".join(lines),
        "model": source,
        "fallback": source == "local",
        "url": url,
        "content_length": len(content),
        "error": error or None,
        "success": not error,
    }