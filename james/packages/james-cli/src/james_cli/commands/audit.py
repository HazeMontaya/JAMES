"""Audit log CLI command."""

from james_core.audit import AuditLog, AuditOutcome, AuditSeverity
from james_core.config import JamesConfig
from rich.console import Console

console = Console()


async def show_audit(limit: int, actor: str | None, action: str | None, outcome: str | None, severity: str | None) -> None:
    config = JamesConfig()
    audit = AuditLog(str(config.paths.memory / "audit.db"))
    outcome_enum = AuditOutcome(outcome) if outcome else None
    severity_enum = AuditSeverity(severity) if severity else None
    try:
        entries = await audit.query(limit=limit, actor=actor, action=action, outcome=outcome_enum, severity=severity_enum)
    finally:
        await audit.close()

    if not entries:
        console.print("[yellow]Audit log is empty.[/yellow]")
        return

    for entry in entries:
        line = (
            f"[bold]{entry['created_at']}[/bold] "
            f"[cyan]{entry['actor']}[/cyan] "
            f"[green]{entry['action']}[/green] "
            f"{entry['outcome']}/{entry['severity']} "
            f"resource={entry['resource'][:60]!r}"
        )
        if entry.get("duration_ms") is not None:
            line += f" ({entry['duration_ms']:.0f} ms)"
        console.print(line)
        details = entry.get("details") or {}
        if details:
            console.print(f"  details={details}")