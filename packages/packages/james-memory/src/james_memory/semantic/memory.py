"""Semantic Memory for JAMES - Markdown Vault + Vector Index"""

import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import aiofiles
import structlog

from ..models import MemoryEntry, MemoryType, RetrievalQuery, RetrievalResult
from ..vault import VaultManager

logger = structlog.get_logger()


class SemanticMemory:
    def __init__(self, vault_path: str = "~/.james/vault"):
        self._vault = VaultManager(vault_path)
        self._index_path = Path(vault_path).expanduser() / ".index.json"
        self._index: dict[str, dict[str, Any]] = {}

    async def initialize(self) -> None:
        await self._vault.initialize()
        await self._load_index()

    async def _load_index(self) -> None:
        if self._index_path.exists():
            async with aiofiles.open(self._index_path) as f:
                content = await f.read()
                self._index = json.loads(content) if content else {}
        else:
            await self._rebuild_index()

    async def _rebuild_index(self) -> None:
        self._index = {}
        for file_path in self._vault.list_files():
            try:
                content = await self._vault.read_file(file_path)
                relative = file_path.relative_to(self._vault.root_path)
                self._index[str(relative)] = {
                    "path": str(relative),
                    "modified": datetime.fromtimestamp(file_path.stat().st_mtime).isoformat(),
                    "size": len(content),
                    "tags": self._extract_tags(content),
                }
            except Exception as e:
                logger.warning("Failed to index file", file=str(file_path), error=str(e))
        await self._save_index()

    async def _save_index(self) -> None:
        async with aiofiles.open(self._index_path, "w") as f:
            await f.write(json.dumps(self._index, indent=2))

    def _extract_tags(self, content: str) -> list[str]:
        tags = []
        for line in content.split("\n"):
            line = line.strip()
            if line.startswith("#"):
                tag = line[1:].split()[0].lower()
                if tag:
                    tags.append(tag)
            if line.startswith("tags:") or line.startswith("tags ="):
                parts = line.split(":") if ":" in line else line.split("=")
                if len(parts) > 1:
                    for t in parts[1].split(","):
                        t = t.strip().strip("\"'[]")
                        if t:
                            tags.append(t.lower())
        return list(set(tags))

    async def store(self, entry: MemoryEntry) -> str:
        if entry.memory_type != MemoryType.SEMANTIC:
            entry.memory_type = MemoryType.SEMANTIC

        file_path = self._entry_to_path(entry)
        content = self._entry_to_markdown(entry)

        await self._vault.write_file(file_path, content)

        relative = file_path.relative_to(self._vault.root_path)
        self._index[str(relative)] = {
            "path": str(relative),
            "modified": datetime.now(UTC).isoformat(),
            "size": len(content),
            "tags": entry.tags,
        }
        await self._save_index()

        return entry.id

    def _entry_to_path(self, entry: MemoryEntry) -> Path:
        date_str = entry.created_at.strftime("%Y/%m/%d")
        safe_id = entry.id[:8]
        return self._vault.root_path / "knowledge" / date_str / f"{safe_id}.md"

    def _entry_to_markdown(self, entry: MemoryEntry) -> str:
        lines = [
            f"# {entry.metadata.get('title', 'Knowledge Entry')}",
            "",
            f"**ID:** {entry.id}",
            f"**Source:** {entry.source}",
            f"**Confidence:** {entry.confidence}",
            f"**Created:** {entry.created_at.isoformat()}",
            f"**Tags:** {', '.join(entry.tags)}",
            "",
            "---",
            "",
            entry.content,
        ]
        return "\n".join(lines)

    async def retrieve(self, entry_id: str) -> MemoryEntry | None:
        for rel_path, _info in self._index.items():
            if entry_id[:8] in rel_path:
                content = await self._vault.read_file(self._vault.root_path / rel_path)
                return self._markdown_to_entry(rel_path, content)
        return None

    def _markdown_to_entry(self, rel_path: str, content: str) -> MemoryEntry:
        lines = content.split("\n")
        entry_id = ""
        metadata: dict[str, Any] = {}
        tags = []
        main_content = []
        in_content = False

        for line in lines:
            if line.startswith("**ID:**"):
                entry_id = line.replace("**ID:**", "").strip()
            elif line.startswith("**Source:**"):
                metadata["source"] = line.replace("**Source:**", "").strip()
            elif line.startswith("**Confidence:**"):
                metadata["confidence"] = float(line.replace("**Confidence:**", "").strip())
            elif line.startswith("**Created:**"):
                metadata["created"] = line.replace("**Created:**", "").strip()
            elif line.startswith("**Tags:**"):
                tags = [t.strip().lower() for t in line.replace("**Tags:**", "").split(",")]
            elif line == "---":
                in_content = True
            elif in_content:
                main_content.append(line)

        return MemoryEntry(
            id=entry_id or rel_path,
            memory_type=MemoryType.SEMANTIC,
            content="\n".join(main_content).strip(),
            metadata=metadata,
            tags=tags,
        )

    async def query(self, query: RetrievalQuery) -> RetrievalResult:
        import time
        start = time.time()

        if not query.memory_types or MemoryType.SEMANTIC in query.memory_types:
            results = []

            for rel_path, info in self._index.items():
                if query.tags and not any(tag in info.get("tags", []) for tag in query.tags):
                    continue

                content = await self._vault.read_file(self._vault.root_path / rel_path)
                entry = self._markdown_to_entry(rel_path, content)

                if query.query.lower() in entry.content.lower():
                    results.append(entry)

                if len(results) >= query.limit:
                    break

            return RetrievalResult(
                entries=results,
                total_found=len(results),
                query=query,
                retrieval_time_ms=(time.time() - start) * 1000,
            )

        return RetrievalResult(entries=[], total_found=0, query=query, retrieval_time_ms=(time.time() - start) * 1000)

    async def search_fulltext(self, query: str, limit: int = 10) -> list[MemoryEntry]:
        results = []
        for rel_path, _info in self._index.items():
            content = await self._vault.read_file(self._vault.root_path / rel_path)
            if query.lower() in content.lower():
                entry = self._markdown_to_entry(rel_path, content)
                results.append(entry)
                if len(results) >= limit:
                    break
        return results

    async def close(self) -> None:
        await self._save_index()
