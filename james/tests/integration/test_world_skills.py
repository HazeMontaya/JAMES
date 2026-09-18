"""Integration tests for World Access + web_scraper skill integration (mocked HTTP)."""

import httpx
import pytest


class FakeTransport:
    """Minimal httpx transport returning a canned HTML page."""

    def __init__(self, html: str = "<html><body><h1>Title</h1><p>Hello world content.</p></body></html>"):
        self.html = html
        self.last_url = ""

    def handle_request(self, request, **kwargs):  # type: ignore[no-untyped-def]
        self.last_url = str(request.url)
        return httpx.Response(200, content=self.html.encode("utf-8"), headers={"content-type": "text/html"})


@pytest.mark.asyncio
async def test_world_web_fetch_extracts_text(monkeypatch: pytest.MonkeyPatch) -> None:
    from james_world import WorldAccess

    world = WorldAccess()
    transport = FakeTransport()

    async def fake_world_health(url: str) -> bool:
        return True

    monkeypatch.setattr(world, "_health_http", fake_world_health)
    await world.on_start()

    async def fake_get(self, url: str, **kwargs):  # type: ignore[no-untyped-def]
        return httpx.Response(
            200,
            content=transport.html.encode("utf-8"),
            headers={"content-type": "text/html"},
            request=httpx.Request("GET", url),
        )

    monkeypatch.setattr(httpx.AsyncClient, "get", fake_get)
    resp = await world.web_fetch(url="https://example.com/page")
    assert resp["url"] == "https://example.com/page"
    assert "Title" in resp["content"]
    assert "Hello world content" in resp["content"]


@pytest.mark.asyncio
async def test_web_scraper_skill_uses_world_tool() -> None:
    from james_skills import SkillEngine
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("web_scraper")
    assert spec is not None

    captured: dict[str, str] = {}

    class FakeWorld:
        async def web_fetch(self, url: str = "", **kwargs):  # type: ignore[no-untyped-def]
            captured["url"] = url
            return {"url": url, "content": "Acme plans cost 49 USD per month.", "content_length": 34}

    ctx = SkillContext(inputs={"url": "https://acme.example/pricing", "query": "pricing"}, world=FakeWorld())
    result = await engine.execute(spec, ctx)
    assert result.success is True
    assert captured["url"] == "https://acme.example/pricing"
    out = result.outputs["scrape"]
    assert "Acme plans cost 49 USD" in out["content"]


@pytest.mark.asyncio
async def test_web_scraper_falls_back_offline_no_world() -> None:
    from james_skills import SkillEngine
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("web_scraper")
    assert spec is not None

    # Without world, tool step falls back to the Python entry point which rejects bad URLs
    ctx = SkillContext(inputs={"url": "not-a-url", "query": "x"})
    result = await engine.execute(spec, ctx)
    assert result.success is True
    assert "Invalid or missing URL" in result.outputs["scrape"]["text"]


@pytest.mark.asyncio
async def test_lead_generation_uses_world_web_search() -> None:
    from james_skills import SkillEngine
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("lead_generation")
    assert spec is not None

    captured: dict[str, str] = {}

    class FakeWorld:
        async def web_search(self, query: str = "", **kwargs):  # type: ignore[no-untyped-def]
            captured["query"] = query
            return {"query": query, "content": "Some lead: Acme Industries at acme.example"}

    ctx = SkillContext(inputs={"offer": "AI Buchhaltung", "target_market": "KMU"}, world=FakeWorld())
    result = await engine.execute(spec, ctx)
    assert result.success is True
    assert "Buchhaltung" in captured["query"]
    text = result.outputs["generate_leads"]["text"].strip()
    assert text


@pytest.mark.asyncio
async def test_world_fetch_capability_present() -> None:
    from james_world import WorldAccess

    world = WorldAccess()
    summary = world.capability_summary()
    assert "web_fetch" in summary
    assert "web_search" in summary