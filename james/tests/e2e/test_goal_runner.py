"""E2E tests for the goal-to-report pipeline"""

import anyio


def test_run_goal_produces_report(tmp_path):
    from james_cli.runner import GoalRunner
    from james_core.audit import AuditLog
    from james_core.config import ConfigPaths, JamesConfig, JamesSettings, ModelEndpoint

    settings = JamesSettings(
        paths=ConfigPaths(root=tmp_path, config=tmp_path / "config.yaml", vault=tmp_path / "vault",
                          memory=tmp_path / "memory", skills=tmp_path / "skills", credentials=tmp_path / "credentials",
                          tools=tmp_path / "tools", runtimes=tmp_path / "runtimes", models=tmp_path / "models",
                          cache=tmp_path / "cache", logs=tmp_path / "logs", browser=tmp_path / "browser",
                          sandboxes=tmp_path / "sandboxes"),
        ollama=ModelEndpoint(enabled=False),
    )
    cfg = JamesConfig(settings)
    runner = GoalRunner(cfg)

    async def run() -> dict:
        await runner.initialize()
        try:
            return await runner.run("Markt fuer ein neues Produkt analysieren", priority="high")
        finally:
            await runner.components["memory"].close()
            await runner.components["audit"].close()
            await runner.components["event_bus"].close()

    result = anyio.run(run)

    assert result["success"] is True
    assert "market_research" in result["skills"]
    assert "Markt fuer ein neues Produkt analysieren" in result["report"]
    assert "# JAMES Report" in result["report"]
    assert "**Priority:** high" in result["report"]

    # Verify events were published during the run
    bus = runner.components["event_bus"]
    subjects = [s for s, _ in bus.published]
    assert any("goal.started" in s for s in subjects)
    assert any("skill.started" in s for s in subjects)
    assert any("goal.finished" in s for s in subjects)

    # Audit trail was written
    async def check_audit() -> list:
        audit = AuditLog(str(cfg.paths.memory / "audit.db"))
        await audit.initialize()
        try:
            return await audit.query(limit=10, action="goal.run")
        finally:
            await audit.close()

    entries = anyio.run(check_audit)
    assert len(entries) >= 1
    assert entries[0]["resource"] == "Markt fuer ein neues Produkt analysieren"
    assert entries[0]["outcome"] == "success"


def test_run_goal_restricted_skills(tmp_path):
    from james_cli.runner import GoalRunner
    from james_core.config import ConfigPaths, JamesConfig, JamesSettings

    settings = JamesSettings(
        paths=ConfigPaths(root=tmp_path, config=tmp_path / "config.yaml", vault=tmp_path / "vault",
                          memory=tmp_path / "memory", skills=tmp_path / "skills", credentials=tmp_path / "credentials",
                          tools=tmp_path / "tools", runtimes=tmp_path / "runtimes", models=tmp_path / "models",
                          cache=tmp_path / "cache", logs=tmp_path / "logs", browser=tmp_path / "browser",
                          sandboxes=tmp_path / "sandboxes"),
    )
    cfg = JamesConfig(settings)
    runner = GoalRunner(cfg)

    async def run() -> dict:
        await runner.initialize()
        try:
            return await runner.run("Research the market and generate leads", skills=["market_research"])
        finally:
            await runner.components["memory"].close()
            await runner.components["audit"].close()
            await runner.components["event_bus"].close()

    result = anyio.run(run)
    assert result["success"] is True
    assert result["skills"] == ["market_research"]


def test_run_goal_no_match(tmp_path):
    from james_cli.runner import GoalRunner
    from james_core.config import ConfigPaths, JamesConfig, JamesSettings

    settings = JamesSettings(
        paths=ConfigPaths(root=tmp_path, config=tmp_path / "config.yaml", vault=tmp_path / "vault",
                          memory=tmp_path / "memory", skills=tmp_path / "skills", credentials=tmp_path / "credentials",
                          tools=tmp_path / "tools", runtimes=tmp_path / "runtimes", models=tmp_path / "models",
                          cache=tmp_path / "cache", logs=tmp_path / "logs", browser=tmp_path / "browser",
                          sandboxes=tmp_path / "sandboxes"),
    )
    cfg = JamesConfig(settings)
    runner = GoalRunner(cfg)

    async def run() -> dict:
        await runner.initialize()
        try:
            return await runner.run("qqqqqqqqqqqqzzzzzzzzzzz", priority="low")
        finally:
            await runner.components["memory"].close()
            await runner.components["audit"].close()
            await runner.components["event_bus"].close()

    result = anyio.run(run)
    assert result["success"] is False
    assert result["skills"] == []