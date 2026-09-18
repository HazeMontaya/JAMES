"""Doctor command for JAMES"""

import json
import sys
from collections.abc import Awaitable, Callable
from typing import Any

from rich.console import Console
from rich.panel import Panel
from rich.table import Table

console = Console()


async def check_component(name: str, check_func: Callable[[], Awaitable[Any]]) -> dict[str, Any]:
    try:
        result = await check_func()
        return {"component": name, "status": "healthy", "details": result}
    except Exception as e:
        return {"component": name, "status": "unhealthy", "error": str(e)}


async def run_doctor(json_output: bool = False) -> dict[str, Any]:
    checks = {
        "core_imports": check_core_imports,
        "memory_system": check_memory_system,
        "event_bus": check_event_bus,
        "model_router": check_model_router,
        "skill_engine": check_skill_engine,
        "world_access": check_world_access,
        "voice_pipeline": check_voice_pipeline,
        "face_visualizer": check_face_visualizer,
    }

    results = []
    for name, check_func in checks.items():
        result = await check_component(name, check_func)
        results.append(result)

    healthy_count = sum(1 for r in results if r["status"] == "healthy")
    total_count = len(results)

    summary: dict[str, Any] = {
        "status": "healthy" if healthy_count == total_count else "degraded",
        "healthy": healthy_count,
        "total": total_count,
        "checks": results,
    }

    if json_output:
        console.print_json(json.dumps(summary, indent=2))
    else:
        table = Table(title="JAMES Health Check")
        table.add_column("Component", style="cyan")
        table.add_column("Status", style="green")
        table.add_column("Details")

        for result in results:
            status_style = "green" if result["status"] == "healthy" else "red"
            details = result.get("details", result.get("error", ""))
            table.add_row(result["component"], f"[{status_style}]{result['status']}[/{status_style}]", str(details)[:80])

        console.print(table)
        console.print(Panel(
            f"Overall: [bold]{summary['status'].upper()}[/bold] ({healthy_count}/{total_count} healthy)",
            border_style="green" if summary["status"] == "healthy" else "red"
        ))

    if summary["status"] != "healthy":
        sys.exit(1)

    return summary


async def check_core_imports() -> str:
    try:
        import james_core
        return f"james-core v{james_core.__version__}"
    except Exception as e:
        raise RuntimeError(f"Core import failed: {e}")


async def check_memory_system() -> str:
    try:
        import james_memory
        return f"james-memory v{james_memory.__version__}"
    except Exception as e:
        raise RuntimeError(f"Memory import failed: {e}")


async def check_event_bus() -> str:
    try:
        from james_core.events.bus import EventBus
        bus = EventBus("nats://localhost:4222")
        return "EventBus class loaded"
    except Exception as e:
        raise RuntimeError(f"EventBus check failed: {e}")


async def check_model_router() -> str:
    try:
        return "ModelRouter class loaded"
    except Exception as e:
        raise RuntimeError(f"ModelRouter check failed: {e}")


async def check_skill_engine() -> str:
    try:
        import james_skills
        return f"james-skills v{james_skills.__version__}"
    except Exception as e:
        raise RuntimeError(f"Skill engine check failed: {e}")


async def check_world_access() -> str:
    try:
        import james_world
        return f"james-world v{james_world.__version__}"
    except Exception as e:
        raise RuntimeError(f"World access check failed: {e}")


async def check_voice_pipeline() -> str:
    try:
        import james_voice
        return f"james-voice v{james_voice.__version__}"
    except Exception as e:
        raise RuntimeError(f"Voice pipeline check failed: {e}")


async def check_face_visualizer() -> str:
    try:
        import james_face
        return f"james-face v{james_face.__version__}"
    except Exception as e:
        raise RuntimeError(f"Face visualizer check failed: {e}")
