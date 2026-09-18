"""Episodic Memory for JAMES - SQLite based event storage"""

import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import aiosqlite
import structlog

from ..models import MemoryEntry, MemoryType, RetrievalQuery, RetrievalResult

logger = structlog.get_logger()


class EpisodicMemory:
    def __init__(self, db_path: str = "~/.james/memory/episodic.db"):
        self._db_path = Path(db_path).expanduser()
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn: aiosqlite.Connection | None = None

    async def initialize(self) -> None:
        self._conn = await aiosqlite.connect(str(self._db_path))
        await self._create_tables()

    async def _create_tables(self) -> None:
        assert self._conn is not None
        await self._conn.execute("""
            CREATE TABLE IF NOT EXISTS episodes (
                id TEXT PRIMARY KEY,
                memory_type TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT NOT NULL,
                tags TEXT NOT NULL,
                embedding TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                access_count INTEGER DEFAULT 0,
                last_accessed TEXT,
                source TEXT DEFAULT '',
                confidence REAL DEFAULT 1.0
            )
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_episodes_type ON episodes(memory_type)
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_episodes_created ON episodes(created_at)
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_episodes_tags ON episodes(tags)
        """)
        await self._conn.commit()

    async def store(self, entry: MemoryEntry) -> str:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        entry.updated_at = datetime.now(UTC)
        tags_json = json.dumps(entry.tags)
        metadata_json = json.dumps(entry.metadata)
        embedding_json = json.dumps(entry.embedding) if entry.embedding else None

        await self._conn.execute("""
            INSERT OR REPLACE INTO episodes 
            (id, memory_type, content, metadata, tags, embedding, created_at, updated_at, access_count, last_accessed, source, confidence)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            entry.id,
            entry.memory_type.value,
            entry.content,
            metadata_json,
            tags_json,
            embedding_json,
            entry.created_at.isoformat(),
            entry.updated_at.isoformat(),
            entry.access_count,
            entry.last_accessed.isoformat() if entry.last_accessed else None,
            entry.source,
            entry.confidence,
        ))
        await self._conn.commit()

        logger.debug("Episodic memory stored", entry_id=entry.id, type=entry.memory_type.value)
        return entry.id

    async def retrieve(self, entry_id: str) -> MemoryEntry | None:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        cursor = await self._conn.execute(
            "SELECT * FROM episodes WHERE id = ?", (entry_id,)
        )
        row = await cursor.fetchone()

        if row:
            await self._update_access(entry_id)
            entry = self._row_to_entry(row)
            entry.access_count += 1
            entry.last_accessed = datetime.now(UTC)
            return entry
        return None

    async def query(self, query: RetrievalQuery) -> RetrievalResult:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        start_time = datetime.now(UTC)
        conditions = []
        params: list[Any] = []

        if query.memory_types:
            placeholders = ",".join("?" * len(query.memory_types))
            conditions.append(f"memory_type IN ({placeholders})")
            params.extend([mt.value for mt in query.memory_types])

        if query.tags:
            for tag in query.tags:
                conditions.append("tags LIKE ?")
                params.append(f"%{tag}%")

        if query.filters:
            for key, value in query.filters.items():
                conditions.append(f"json_extract(metadata, '$.{key}') = ?")
                params.append(value)

        if query.min_confidence > 0:
            conditions.append("confidence >= ?")
            params.append(query.min_confidence)

        where_clause = "WHERE " + " AND ".join(conditions) if conditions else ""

        sql = f"""
            SELECT * FROM episodes 
            {where_clause}
            ORDER BY created_at DESC
            LIMIT ?
        """
        params.append(query.limit)

        cursor = await self._conn.execute(sql, params)
        rows = await cursor.fetchall()

        entries = [self._row_to_entry(row) for row in rows]

        for entry in entries:
            await self._update_access(entry.id)

        retrieval_time = (datetime.now(UTC) - start_time).total_seconds() * 1000

        return RetrievalResult(
            entries=entries,
            total_found=len(entries),
            query=query,
            retrieval_time_ms=retrieval_time,
        )

    async def _update_access(self, entry_id: str) -> None:
        assert self._conn is not None
        await self._conn.execute("""
            UPDATE episodes 
            SET access_count = access_count + 1, last_accessed = ?
            WHERE id = ?
        """, (datetime.now(UTC).isoformat(), entry_id))
        await self._conn.commit()

    def _row_to_entry(self, row: aiosqlite.Row) -> MemoryEntry:
        return MemoryEntry(
            id=row[0],
            memory_type=MemoryType(row[1]),
            content=row[2],
            metadata=json.loads(row[3]),
            tags=json.loads(row[4]),
            embedding=json.loads(row[5]) if row[5] else None,
            created_at=datetime.fromisoformat(row[6]),
            updated_at=datetime.fromisoformat(row[7]),
            access_count=row[8],
            last_accessed=datetime.fromisoformat(row[9]) if row[9] else None,
            source=row[10],
            confidence=row[11],
        )

    async def delete(self, entry_id: str) -> bool:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        cursor = await self._conn.execute("DELETE FROM episodes WHERE id = ?", (entry_id,))
        await self._conn.commit()
        return cursor.rowcount > 0

    async def cleanup_old(self, days: int = 90) -> int:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        cutoff = datetime.now(UTC).replace(day=datetime.now(UTC).day - days).isoformat()
        cursor = await self._conn.execute(
            "DELETE FROM episodes WHERE created_at < ? AND access_count = 0", (cutoff,)
        )
        await self._conn.commit()
        return cursor.rowcount

    async def close(self) -> None:
        if self._conn:
            await self._conn.close()
            self._conn = None
