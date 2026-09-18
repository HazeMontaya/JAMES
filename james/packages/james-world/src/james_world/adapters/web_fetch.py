"""Direct web fetch with HTML-to-text extraction."""

from html.parser import HTMLParser
from typing import Any

import httpx

MAX_CHARS = 60000


_BLOCK_TAGS = {
    "p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "li", "ul", "ol", "section",
    "article", "br", "blockquote", "pre", "table", "tr", "td", "th", "header", "footer",
}
_SKIP_TAGS = {"script", "style", "noscript", "svg", "head", "nav", "iframe"}


class _TextExtractor(HTMLParser):
    """Strips scripts/styles/tags and normalizes whitespace to readable text."""

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._parts: list[str] = []
        self._skip_depth = 0

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in _SKIP_TAGS:
            self._skip_depth += 1
        if self._skip_depth == 0 and tag in _BLOCK_TAGS:
            self._parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if tag in _SKIP_TAGS and self._skip_depth:
            self._skip_depth -= 1
        if self._skip_depth == 0 and tag in _BLOCK_TAGS:
            self._parts.append("\n")

    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if self._skip_depth == 0 and tag in ("br", "hr"):
            self._parts.append("\n")

    def handle_data(self, data: str) -> None:
        if self._skip_depth == 0:
            self._parts.append(data)

    def text(self) -> str:
        import re

        return re.sub(r"[ \t\r\f\v]+", " ", "".join(self._parts))


def extract_text(html: str, max_characters: int = MAX_CHARS) -> str:
    """Convert raw HTML to collapsed plain text, capped in length."""
    parser = _TextExtractor()
    try:
        parser.feed(html)
        parser.close()
    except Exception:
        return ""[:max_characters]
    return parser.text().strip()[:max_characters]


async def fetch_readable(url: str, timeout: float = 20, max_characters: int = MAX_CHARS) -> dict[str, Any]:
    """Fetch a URL over HTTP(S) and return extracted text content."""
    async with httpx.AsyncClient(timeout=timeout, follow_redirects=True) as client:
        resp = await client.get(url)
        resp.raise_for_status()
        content_type = resp.headers.get("content-type", "")
        if "json" in content_type:
            return {"url": url, "status": resp.status_code, "format": "json", "content": resp.text[:max_characters]}
        text = extract_text(resp.text, max_characters=max_characters)
        return {"url": url, "status": resp.status_code, "format": "html", "content": text, "content_length": len(text)}