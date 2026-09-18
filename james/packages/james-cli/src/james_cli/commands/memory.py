"""JAMES memory query command"""

from typing import Any

import structlog
from james_memory import MemoryType
from rich.console import Console
from rich.table import Table

logger = structlog.get_logger()

console = Console()

_TYPE_MAP: dict[str, list[MemoryType]] = {
    "episodic": [MemoryType.EPISODIC],
    "semantic": [MemoryType.SEMANTIC],
    "procedural": [MemoryType.PROCEDURAL],
    "business": [MemoryType.BUSINESS],
    "user": [MemoryType.USER],
    "all": [MemoryType.EPISODIC, MemoryType.SEMANTIC, MemoryType.PROCEDURAL, MemoryType.BUSINESS, MemoryType.USER],
}


async def query_memory(query: str | None, type: str, limit: int) -> None:
    from james_core.config import JamesConfig
    from james_memory.models import RetrievalQuery

    from ..runner import build_runner_components

    types = _TYPE_MAP.get(type.lower())
    if types is None:
        console.print(f"[red]Unknown memory type: {type}[/red]")
        return

    cfg = JamesConfig()
    components = build_runner_components(cfg)
    memory = components["memory"]

    try:
        if query and query.lower() in ("list", "all", "--all"):
            entries = await _list_recent(memory, types, limit)
        elif query:
            rq = RetrievalQuery(query=query, memory_types=types, limit=limit)
            result = await memory.retrieve(rq)
            entries = result.entries
        else:
            entries = await _list_recent(memory, types, limit)

        if not entries:
            console.print("[yellow]No memory entries found.[/yellow]")
            return

        table = Table(title="JAMES Memory")
        table.add_column("Type", style="cyan")
        table.add_column("Content", max_width=80)
        table.add_column("Confidence")
        for entry in entries:
            content = entry.content[:240].replace("\n", " ")
            table.add_row(entry.memory_type.value, content, f"{entry.confidence:.2f}")
        console.print(table)
    finally:
        await memory.close()


async def _list_recent(memory: Any, types: list[MemoryType], limit: int) -> list[Any]:
    from james_memory.models import RetrievalQuery, RetrievalResult

    rq = RetrievalQuery(query="", memory_types=types, limit=limit)
    result: RetrievalResult = await memory.retrieve(rq)
    return result.entries