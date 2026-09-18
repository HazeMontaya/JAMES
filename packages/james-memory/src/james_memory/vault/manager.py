"""Vault Manager for JAMES - Markdown Vault Operations"""

from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import aiofiles
import aiofiles.os
import structlog

logger = structlog.get_logger()


class VaultManager:
    def __init__(self, vault_path: str = "~/.james/vault"):
        self.root_path = Path(vault_path).expanduser()
        self._structure: dict[str, list[str]] = {
            "profile": [],
            "projects": [],
            "daily": [],
            "jobs": [],
            "knowledge": [],
            "business": [],
            "rules": [],
        }

    async def initialize(self) -> None:
        self.root_path.mkdir(parents=True, exist_ok=True)
        for dir_name in self._structure:
            (self.root_path / dir_name).mkdir(exist_ok=True)

        await self._ensure_default_files()

    async def _ensure_default_files(self) -> None:
        defaults = {
            "profile/user.md": "# User Profile\n\n<!-- JAMES user profile -->\n",
            "profile/preferences.json": "{}",
            "profile/context.json": "{}",
            "rules/core.md": "# Core Rules\n\n<!-- Core operating rules for JAMES -->\n",
            "knowledge/README.md": "# Knowledge Base\n\n<!-- Semantic knowledge entries -->\n",
            "business/README.md": "# Business\n\n<!-- Business knowledge and opportunities -->\n",
        }

        for rel_path, content in defaults.items():
            file_path = self.root_path / rel_path
            if not file_path.exists():
                file_path.parent.mkdir(parents=True, exist_ok=True)
                async with aiofiles.open(file_path, "w") as f:
                    await f.write(content)

    async def read_file(self, path: Path) -> str:
        try:
            async with aiofiles.open(path) as f:
                return await f.read()
        except Exception as e:
            logger.error("Failed to read vault file", path=str(path), error=str(e))
            raise

    async def write_file(self, rel_path: str | Path, content: str) -> None:
        file_path = self.root_path / rel_path if not isinstance(rel_path, Path) else rel_path
        file_path.parent.mkdir(parents=True, exist_ok=True)

        async with aiofiles.open(file_path, "w") as f:
            await f.write(content)

        logger.debug("Vault file written", path=str(file_path.relative_to(self.root_path)))

    async def append_file(self, rel_path: str | Path, content: str) -> None:
        file_path = self.root_path / rel_path if not isinstance(rel_path, Path) else rel_path
        file_path.parent.mkdir(parents=True, exist_ok=True)

        existing = ""
        if file_path.exists():
            async with aiofiles.open(file_path) as f:
                existing = await f.read()

        async with aiofiles.open(file_path, "w") as f:
            await f.write(existing + "\n" + content)

    async def delete_file(self, rel_path: str | Path) -> bool:
        file_path = self.root_path / rel_path if not isinstance(rel_path, Path) else rel_path
        try:
            await aiofiles.os.remove(file_path)
            return True
        except FileNotFoundError:
            return False

    def list_files(self, subdir: str = "") -> list[Path]:
        search_path = self.root_path / subdir if subdir else self.root_path
        if not search_path.exists():
            return []
        return list(search_path.rglob("*.md"))

    async def search(self, query: str, subdir: str = "") -> list[dict[str, Any]]:
        results = []
        for file_path in self.list_files(subdir):
            try:
                content = await self.read_file(file_path)
                if query.lower() in content.lower():
                    results.append({
                        "path": file_path.relative_to(self.root_path),
                        "content": content[:500],
                    })
            except Exception:
                pass
        return results

    async def get_daily_note(self, date: datetime | None = None) -> str:
        date = date or datetime.now(UTC)
        path = f"daily/{date.strftime('%Y-%m-%d')}.md"
        file_path = self.root_path / path

        if file_path.exists():
            return await self.read_file(file_path)

        template = f"""# Daily Note - {date.strftime('%Y-%m-%d')}

## Goals
-

## Notes
-

## Decisions
-

## Learnings
-

"""
        await self.write_file(path, template)
        return template

    async def create_job_context(self, job_id: str, context: dict[str, Any]) -> None:
        path = f"jobs/{job_id}.md"
        content = f"""# Job Context: {job_id}

**Created:** {datetime.now(UTC).isoformat()}
**Context:**
```json
{json.dumps(context, indent=2)}
```

## Notes
-

## Results
-
"""
        await self.write_file(path, content)

    async def read_job_context(self, job_id: str) -> str | None:
        path = f"jobs/{job_id}.md"
        file_path = self.root_path / path
        if file_path.exists():
            return await self.read_file(file_path)
        return None

import json
