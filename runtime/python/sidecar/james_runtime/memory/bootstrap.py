"""Initialize a JAMES memory workspace without overwriting user data."""
from pathlib import Path
from .vault import MemoryVault
from .skills import SkillStore

DEFAULT_INDEX = """# JAMES Memory Index

This is the entry point for persistent JAMES memory.

## Operating Rules

- Markdown notes are authoritative user-facing memory.
- The SQLite index is derived and can always be rebuilt.
- Read only the context needed for the current task.
- Persist important decisions, actions, errors, fixes, and lessons.
- Never claim a state is complete without verification.

## Active Priorities

Keep current priorities here or in project notes.

## Projects

Add project index links here.

## Skills / Jobs

Recurring work belongs in skills/ and accumulates lessons.

## Sessions

Daily continuity is stored in 01 - Daily Notes/ and structured records in sessions/.
"""

def bootstrap(root: str | Path) -> MemoryVault:
    root=Path(root).expanduser().resolve()
    root.mkdir(parents=True,exist_ok=True)
    for directory in ("01 - Daily Notes","projects","skills","sessions","archive","inbox"):
        (root/directory).mkdir(parents=True,exist_ok=True)
    index=root/"JAMES_INDEX.md"
    if not index.exists():
        index.write_text(DEFAULT_INDEX,encoding="utf-8")
    vault=MemoryVault(root)
    vault.rebuild_index()
    SkillStore(root/"skills")
    return vault
