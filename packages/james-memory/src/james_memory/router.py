"""Memory Router for JAMES - Context-aware Retrieval"""

from typing import Any

import structlog

from .business import BusinessMemory
from .episodic import EpisodicMemory
from .models import MemoryEntry, MemoryType, RetrievalQuery, RetrievalResult
from .procedural import ProceduralMemory
from .semantic import SemanticMemory
from .user import UserMemory

logger = structlog.get_logger()


class MemoryRouter:
    def __init__(
        self,
        episodic: EpisodicMemory,
        semantic: SemanticMemory,
        procedural: ProceduralMemory,
        business: BusinessMemory,
        user: UserMemory,
    ):
        self._episodic = episodic
        self._semantic = semantic
        self._procedural = procedural
        self._business = business
        self._user = user

    async def store(
        self,
        content: str,
        memory_type: MemoryType,
        metadata: dict[str, Any] | None = None,
        tags: list[str] | None = None,
        source: str = "",
        confidence: float = 1.0,
    ) -> MemoryEntry:
        entry = MemoryEntry(
            content=content,
            memory_type=memory_type,
            metadata=metadata or {},
            tags=tags or [],
            source=source,
            confidence=confidence,
        )

        if memory_type == MemoryType.EPISODIC:
            await self._episodic.store(entry)
        elif memory_type == MemoryType.SEMANTIC:
            await self._semantic.store(entry)
        elif memory_type == MemoryType.PROCEDURAL:
            await self._procedural.store(entry)
        elif memory_type == MemoryType.BUSINESS:
            await self._business.store(entry)
        elif memory_type == MemoryType.USER:
            await self._user.store(entry)

        return entry

    async def retrieve(self, query: RetrievalQuery) -> RetrievalResult:
        results = RetrievalResult(entries=[], total_found=0, query=query, retrieval_time_ms=0)

        memories_to_search = query.memory_types or [
            MemoryType.EPISODIC,
            MemoryType.SEMANTIC,
            MemoryType.PROCEDURAL,
            MemoryType.BUSINESS,
            MemoryType.USER,
        ]

        for mem_type in memories_to_search:
            if mem_type == MemoryType.EPISODIC:
                res = await self._episodic.query(query)
            elif mem_type == MemoryType.SEMANTIC:
                res = await self._semantic.query(query)
            elif mem_type == MemoryType.PROCEDURAL:
                res = await self._procedural.query(query)
            elif mem_type == MemoryType.BUSINESS:
                res = await self._business.query(query)
            elif mem_type == MemoryType.USER:
                res = await self._user.query(query)
            else:
                continue

            results.entries.extend(res.entries)
            results.total_found += res.total_found
            results.retrieval_time_ms += res.retrieval_time_ms

        results.entries.sort(key=lambda e: e.confidence, reverse=True)
        if query.limit:
            results.entries = results.entries[:query.limit]

        return results

    async def get_skill(self, skill_name: str) -> dict[str, Any] | None:
        return await self._procedural.get_skill(skill_name)

    async def list_skills(self, category: str | None = None) -> list[dict[str, Any]]:
        return await self._procedural.list_skills(category)

    async def record_skill_usage(self, skill_name: str, success: bool) -> None:
        await self._procedural.update_usage(skill_name, success)

    async def add_learned_pattern(self, skill_name: str, pattern: dict[str, Any]) -> None:
        await self._procedural.add_learned_pattern(skill_name, pattern)

    async def get_user_preference(self, key: str, default: Any = None) -> Any:
        return await self._user.get_preference(key, default)

    async def set_user_preference(self, key: str, value: Any) -> None:
        await self._user.set_preference(key, value)

    async def get_user_context(self, key: str, default: Any = None) -> Any:
        return await self._user.get_context(key, default)

    async def set_user_context(self, key: str, value: Any) -> None:
        await self._user.set_context(key, value)

    async def search_knowledge(self, query: str, limit: int = 10) -> list[MemoryEntry]:
        return await self._semantic.search_fulltext(query, limit)

    async def get_daily_note(self, date: Any = None) -> str:
        return await self._user._vault.get_daily_note(date)

    async def close(self) -> None:
        await self._episodic.close()
        await self._semantic.close()
        await self._procedural.close()
        await self._business.close()
        await self._user.close()
