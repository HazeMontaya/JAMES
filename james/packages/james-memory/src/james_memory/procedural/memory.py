"""Procedural Memory for JAMES - Skills, Procedures, Patterns"""

import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import aiosqlite
import structlog

from ..models import MemoryEntry, RetrievalQuery, RetrievalResult

logger = structlog.get_logger()


class ProceduralMemory:
    def __init__(self, db_path: str = "~/.james/memory/procedural.db"):
        self._db_path = Path(db_path).expanduser()
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn: aiosqlite.Connection | None = None

    async def initialize(self) -> None:
        self._conn = await aiosqlite.connect(str(self._db_path))
        await self._create_tables()

    async def _create_tables(self) -> None:
        assert self._conn is not None
        await self._conn.execute("""
            CREATE TABLE IF NOT EXISTS procedures (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                category TEXT,
                description TEXT,
                steps TEXT NOT NULL,
                prerequisites TEXT,
                tools TEXT,
                inputs TEXT,
                outputs TEXT,
                metrics TEXT,
                verification TEXT,
                failure_modes TEXT,
                examples TEXT,
                learned_patterns TEXT,
                version TEXT DEFAULT '1.0.0',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                usage_count INTEGER DEFAULT 0,
                success_rate REAL DEFAULT 0.0
            )
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_procedures_category ON procedures(category)
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_procedures_name ON procedures(name)
        """)
        await self._conn.commit()

    async def store_skill(self, skill_data: dict[str, Any]) -> str:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        skill_id = str(skill_data.get("name", "")).lower().replace(" ", "_")
        now = datetime.now(UTC).isoformat()

        await self._conn.execute("""
            INSERT OR REPLACE INTO procedures 
            (id, name, category, description, steps, prerequisites, tools, inputs, outputs, 
             metrics, verification, failure_modes, examples, learned_patterns, version, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            skill_id,
            skill_data.get("name", ""),
            skill_data.get("category", ""),
            skill_data.get("description", ""),
            json.dumps(skill_data.get("steps", [])),
            json.dumps(skill_data.get("prerequisites", [])),
            json.dumps(skill_data.get("tools", [])),
            json.dumps(skill_data.get("inputs", [])),
            json.dumps(skill_data.get("outputs", [])),
            json.dumps(skill_data.get("metrics", [])),
            json.dumps(skill_data.get("verification", [])),
            json.dumps(skill_data.get("failure_modes", [])),
            json.dumps(skill_data.get("examples", [])),
            json.dumps(skill_data.get("learned_patterns", [])),
            skill_data.get("version", "1.0.0"),
            skill_data.get("created_at", now),
            now,
        ))
        await self._conn.commit()
        return skill_id

    async def get_skill(self, skill_name: str) -> dict[str, Any] | None:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        skill_id = skill_name.lower().replace(" ", "_")
        cursor = await self._conn.execute("SELECT * FROM procedures WHERE id = ?", (skill_id,))
        row = await cursor.fetchone()

        if row:
            return self._row_to_skill(row)
        return None

    async def list_skills(self, category: str | None = None) -> list[dict[str, Any]]:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        if category:
            cursor = await self._conn.execute("SELECT * FROM procedures WHERE category = ? ORDER BY name", (category,))
        else:
            cursor = await self._conn.execute("SELECT * FROM procedures ORDER BY category, name")

        rows = await cursor.fetchall()
        return [self._row_to_skill(row) for row in rows]

    async def update_usage(self, skill_name: str, success: bool) -> None:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        skill_id = skill_name.lower().replace(" ", "_")
        await self._conn.execute("""
            UPDATE procedures 
            SET usage_count = usage_count + 1,
                success_rate = (success_rate * usage_count + ?) / (usage_count + 1),
                updated_at = ?
            WHERE id = ?
        """, (1.0 if success else 0.0, datetime.now(UTC).isoformat(), skill_id))
        await self._conn.commit()

    async def add_learned_pattern(self, skill_name: str, pattern: dict[str, Any]) -> None:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        skill_id = skill_name.lower().replace(" ", "_")
        cursor = await self._conn.execute("SELECT learned_patterns FROM procedures WHERE id = ?", (skill_id,))
        row = await cursor.fetchone()

        patterns = json.loads(row[0]) if row and row[0] else []
        patterns.append(pattern)

        await self._conn.execute("""
            UPDATE procedures 
            SET learned_patterns = ?, updated_at = ?
            WHERE id = ?
        """, (json.dumps(patterns), datetime.now(UTC).isoformat(), skill_id))
        await self._conn.commit()

    def _row_to_skill(self, row: aiosqlite.Row) -> dict[str, Any]:
        return {
            "id": row[0],
            "name": row[1],
            "category": row[2],
            "description": row[3],
            "steps": json.loads(row[4]),
            "prerequisites": json.loads(row[5]),
            "tools": json.loads(row[6]),
            "inputs": json.loads(row[7]),
            "outputs": json.loads(row[8]),
            "metrics": json.loads(row[9]),
            "verification": json.loads(row[10]),
            "failure_modes": json.loads(row[11]),
            "examples": json.loads(row[12]),
            "learned_patterns": json.loads(row[13]),
            "version": row[14],
            "created_at": row[15],
            "updated_at": row[16],
            "usage_count": row[17],
            "success_rate": row[18],
        }

    async def store(self, entry: MemoryEntry) -> str:
        return entry.id

    async def retrieve(self, entry_id: str) -> MemoryEntry | None:
        return None

    async def query(self, query: RetrievalQuery) -> RetrievalResult:
        return RetrievalResult(entries=[], total_found=0, query=query, retrieval_time_ms=0)

    async def close(self) -> None:
        if self._conn:
            await self._conn.close()
            self._conn = None
