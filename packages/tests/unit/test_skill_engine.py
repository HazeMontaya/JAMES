"""Skill engine tests"""

import anyio
import pytest
from james_skills import SkillEngine
from james_skills.engine.models import SkillContext


@pytest.fixture(scope="module")
def engine():
    return SkillEngine()


def test_discover_builtin_skills(engine):
    assert engine.registry().has("market_research")
    names = engine.registry().names()
    assert "market_research" in names


def test_composer_selects_market_skills(engine):
    comp = engine.compose("Finde heraus, ob es einen Markt fuer das neue Produkt gibt")
    assert any(s.skill_name == "market_research" for s in comp.skills)


class DummyModelRouter:
    async def generate(self, prompt, **kwargs):
        result = """# Market Research Report

## Demand
Strong demand signals. Users actively seek better note-taking solutions.

## Competitors
Several established players, but gaps exist in privacy-focused solutions.

## Opportunity
Clear opportunity for a privacy-first AI note-taking product.

## Target Audience
Knowledge workers, students, and independent professionals."""
        from dataclasses import dataclass

        @dataclass
        class R:
            content: str
            model: str = "dummy"

        return R(result)


def test_execute_market_research_with_fallback():
    import os
    from tempfile import TemporaryDirectory

    with TemporaryDirectory() as tmp:
        os.environ.setdefault("JAMES_HOME", tmp)
        engine = SkillEngine()
        spec = engine.get_skill("market_research")
        assert spec is not None

        async def run():
            ctx = SkillContext(inputs={"topic": "AI note-taking apps"}, model_router=DummyModelRouter())
            result = await engine.execute(spec, ctx)
            return result

        result = anyio.run(run)
        assert result.success is True
        assert "text" in result.outputs.get("research", {})


def test_composed_steps_are_ordered(engine):
    comp = engine.compose("Research and analyze market for SaaS pricing tools", ["market_research"])
    assert len(comp.skills) >= 1
    for i, step in enumerate(comp.skills):
        assert step.order == i
        if i > 0:
            assert step.depends_on == [comp.skills[i - 1].skill_name]


def test_discover_lead_generation_skill(engine):
    assert engine.registry().has("lead_generation")


def test_composer_selects_lead_generation(engine):
    comp = engine.compose("Wir brauchen mehr Leads und Outreach fuer unser Produkt")
    assert any(s.skill_name == "lead_generation" for s in comp.skills)


def test_execute_lead_generation_fallback():
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("lead_generation")
    assert spec is not None

    async def run():
        ctx = SkillContext(inputs={"offer": "SaaS Buchhaltungstool"})
        return await engine.execute(spec, ctx)

    result = anyio.run(run)
    assert result.success is True
    text = result.outputs["generate_leads"]["text"]
    assert "SaaS Buchhaltungstool" in text
    assert "Outreach" in text


def test_discover_web_scraper_skill(engine):
    assert engine.registry().has("web_scraper")


def test_execute_web_scraper_fallback():
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("web_scraper")
    assert spec is not None

    async def run():
        ctx = SkillContext(inputs={"url": "example.com", "query": "test"})
        return await engine.execute(spec, ctx)

    result = anyio.run(run)
    assert result.success is True
    text = result.outputs["scrape"].get("text", "")
    assert "Invalid or missing URL" in text


def test_discover_code_generator_skill(engine):
    assert engine.registry().has("code_generator")


def test_execute_code_generator_fallback():
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("code_generator")
    assert spec is not None

    async def run():
        ctx = SkillContext(inputs={"language": "python", "requirements": "Build a todo CLI"})
        return await engine.execute(spec, ctx)

    result = anyio.run(run)
    assert result.success is True
    code = result.outputs["generate"]["text"]
    assert "class Service" in code
    assert "python" in result.outputs["generate"]["language"]


def test_discover_data_analyzer_skill(engine):
    assert engine.registry().has("data_analyzer")


def test_execute_data_analyzer_fallback():
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("data_analyzer")
    assert spec is not None

    data = "name,revenue\nAcme,100\nBeta,200\nGamma,300"

    async def run():
        ctx = SkillContext(inputs={"data": data, "question": "How many records?"})
        return await engine.execute(spec, ctx)

    result = anyio.run(run)
    assert result.success is True
    report = result.outputs["analyze"]["text"]
    assert "records" in report
    assert "100" in report


def test_discover_email_drafter_skill(engine):
    assert engine.registry().has("email_drafter")


def test_execute_email_drafter_fallback():
    from james_skills.engine.models import SkillContext

    engine = SkillEngine()
    spec = engine.get_skill("email_drafter")
    assert spec is not None

    async def run():
        ctx = SkillContext(
            inputs={
                "recipient": "Max",
                "objective": "Propose a partnership",
                "tone": "formal",
            }
        )
        return await engine.execute(spec, ctx)

    result = anyio.run(run)
    assert result.success is True
    email = result.outputs["draft"]["text"]
    assert "Dear Max" in email
    assert "partnership" in email.lower()


def test_composer_selects_data_analyzer(engine):
    comp = engine.compose("Analysiere die Verkaufsdaten und erstelle einen Bericht")
    assert any(s.skill_name == "data_analyzer" for s in comp.skills)


def test_composer_selects_code_generator(engine):
    comp = engine.compose("Generiere Code fuer einen Python CLI Parser")
    assert any(s.skill_name == "code_generator" for s in comp.skills)
