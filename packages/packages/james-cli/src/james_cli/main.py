"""JAMES CLI - Main Entry Point"""

import typer
from rich.console import Console
from rich.markdown import Markdown

app = typer.Typer(
    name="james",
    help="JAMES - Autonomous Business Agent System",
    add_completion=False,
)

console = Console()


@app.command()
def run(
    goal: str = typer.Argument(..., help="Goal description"),
    priority: str = typer.Option("normal", help="Goal priority: low, normal, high, critical"),
    skills: str = typer.Option("", help="Comma-separated list of skills to use"),
) -> None:
    """Run JAMES with a goal"""
    import anyio

    from .runner import VALID_PRIORITIES, run_goal

    if priority not in VALID_PRIORITIES:
        console.print(f"[red]Invalid priority '{priority}'. Expected one of: {', '.join(VALID_PRIORITIES)}[/red]")
        raise typer.Exit(code=1)

    skill_list = [s.strip() for s in skills.split(",") if s.strip()] if skills else None

    console.print(f"[bold green]JAMES[/bold green] Starting goal: {goal}")
    console.print(f"Priority: {priority}")
    if skill_list:
        console.print(f"Skills: {', '.join(skill_list)}")

    result = anyio.run(run_goal, goal, None, priority, skill_list)

    if not result["success"]:
        console.print(f"[yellow]No skills matched the goal: {result.get('message', 'unknown')}[/yellow]")
        raise typer.Exit(code=1)

    console.print(Markdown(result["report"]))


@app.command()
def doctor(
    json_output: bool = typer.Option(False, "--json", help="Output as JSON"),
) -> None:
    """Run system health checks"""
    import anyio

    from .commands.doctor import run_doctor
    anyio.run(run_doctor, json_output)


@app.command()
def config(
    show: bool = typer.Option(False, "--show", help="Show current configuration"),
    set_key: str = typer.Option(None, "--set", help="Set configuration key"),
    value: str = typer.Option(None, "--value", help="Value for --set"),
) -> None:
    """Manage JAMES configuration"""
    from james_core.config import JamesConfig

    cfg = JamesConfig()
    if set_key:
        if not value:
            console.print("[red]Value required for --set[/red]")
            raise typer.Exit(code=1)
        try:
            setattr(cfg.settings, set_key, value)
            cfg.save()
            console.print(f"[green]Set {set_key} = {value}[/green]")
        except Exception as e:
            console.print(f"[red]Failed to set {set_key}: {e}[/red]")
            raise typer.Exit(code=1)
    elif show:
        import json

        console.print(json.dumps(cfg.settings.model_dump(), indent=2, default=str))
    else:
        console.print("Use --show to display, --set KEY --value V to set a top-level setting.")


@app.command()
def skill(
    name: str = typer.Argument(None, help="Skill name"),
    list_all: bool = typer.Option(False, "--list", "-l", help="List all skills"),
    info: bool = typer.Option(False, "--info", "-i", help="Show skill info"),
) -> None:
    """Manage skills"""
    from james_skills import SkillEngine

    engine = SkillEngine()
    if list_all:
        skills = engine.list_skills()
        for s in skills:
            console.print(f"[bold]{s.name}[/bold] v{s.version} [{s.category}] - {s.description}")
    elif name and info:
        spec = engine.get_skill(name)
        if spec is None:
            console.print(f"[red]Skill not found: {name}[/red]")
            raise typer.Exit(code=1)
        console.print(f"[bold]{spec.name}[/bold] v{spec.version} [{spec.status.value}]")
        console.print(f"Category: {spec.category}")
        console.print(f"Purpose: {spec.purpose}")
        console.print(f"Prerequisites: {', '.join(spec.prerequisites) or 'none'}")
        console.print(f"Tags: {', '.join(spec.tags) or 'none'}")
    else:
        console.print("Use --list to list skills, or NAME --info for details.")


@app.command()
def memory(
    query: str = typer.Argument(None, help="Search query"),
    type: str = typer.Option("all", help="Memory type: episodic, semantic, procedural, business, user"),
    limit: int = typer.Option(10, help="Max results"),
) -> None:
    """Query memory"""
    import anyio

    from .commands.memory import query_memory

    anyio.run(query_memory, query, type, limit)


@app.command()
def audit(
    limit: int = typer.Option(20, help="Max entries"),
    actor: str = typer.Option(None, help="Filter by actor"),
    action: str = typer.Option(None, help="Filter by action"),
    outcome: str = typer.Option(None, help="Filter by outcome: success, failure, warning, info"),
    severity: str = typer.Option(None, help="Filter by severity: info, warning, error, critical"),
) -> None:
    """Show the audit log"""
    import anyio

    from .commands.audit import show_audit

    anyio.run(show_audit, limit, actor, action, outcome, severity)


@app.command()
def voice(
    start: bool = typer.Option(False, "--start", help="Start voice pipeline"),
    stop: bool = typer.Option(False, "--stop", help="Stop voice pipeline"),
    test: bool = typer.Option(False, "--test", help="Test voice pipeline"),
) -> None:
    """Manage voice pipeline"""
    console.print("[yellow]Voice pipeline not yet implemented[/yellow]")


@app.command()
def bootstrap(
    interactive: bool = typer.Option(True, "--interactive/--non-interactive", help="Run interactive wizard"),
) -> None:
    """Run JAMES bootstrap wizard"""
    console.print("[yellow]Bootstrap wizard not yet implemented[/yellow]")


@app.command()
def version() -> None:
    """Show JAMES version"""
    console.print("JAMES v0.1.0")


def main() -> None:
    app()


if __name__ == "__main__":
    main()
