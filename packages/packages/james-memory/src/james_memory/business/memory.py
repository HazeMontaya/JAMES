"""Business Memory for JAMES - Opportunities, Metrics, Decisions"""

import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import aiosqlite
import structlog

from ..models import MemoryEntry, RetrievalQuery, RetrievalResult

logger = structlog.get_logger()


class BusinessMemory:
    def __init__(self, db_path: str = "~/.james/memory/business.db"):
        self._db_path = Path(db_path).expanduser()
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn: aiosqlite.Connection | None = None

    async def initialize(self) -> None:
        self._conn = await aiosqlite.connect(str(self._db_path))
        await self._create_tables()

    async def _create_tables(self) -> None:
        assert self._conn is not None
        await self._conn.execute("""
            CREATE TABLE IF NOT EXISTS opportunities (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                market TEXT,
                problem TEXT,
                solution TEXT,
                target_audience TEXT,
                business_model TEXT,
                validation_status TEXT DEFAULT 'idea',
                validation_score REAL DEFAULT 0.0,
                estimated_revenue REAL,
                estimated_cost REAL,
                confidence REAL DEFAULT 0.5,
                source_data TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
        """)
        await self._conn.execute("""
            CREATE TABLE IF NOT EXISTS metrics (
                id TEXT PRIMARY KEY,
                opportunity_id TEXT,
                name TEXT NOT NULL,
                value REAL,
                unit TEXT,
                period TEXT,
                recorded_at TEXT NOT NULL
            )
        """)
        await self._conn.execute("""
            CREATE TABLE IF NOT EXISTS decisions (
                id TEXT PRIMARY KEY,
                opportunity_id TEXT,
                decision_type TEXT,
                description TEXT,
                rationale TEXT,
                alternatives TEXT,
                outcome TEXT,
                confidence REAL,
                made_at TEXT NOT NULL
            )
        """)
        await self._conn.execute("""
            CREATE TABLE IF NOT EXISTS experiments (
                id TEXT PRIMARY KEY,
                opportunity_id TEXT,
                hypothesis TEXT,
                method TEXT,
                results TEXT,
                conclusion TEXT,
                status TEXT DEFAULT 'planned',
                started_at TEXT,
                completed_at TEXT
            )
        """)
        await self._conn.commit()

    async def store_opportunity(self, opp: dict[str, Any]) -> str:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        opp_id = str(opp.get("id", f"opp_{datetime.now(UTC).timestamp()}"))
        now = datetime.now(UTC).isoformat()

        await self._conn.execute("""
            INSERT OR REPLACE INTO opportunities 
            (id, title, description, market, problem, solution, target_audience, 
             business_model, validation_status, validation_score, estimated_revenue, 
             estimated_cost, confidence, source_data, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            opp_id,
            opp.get("title", ""),
            opp.get("description", ""),
            opp.get("market", ""),
            opp.get("problem", ""),
            opp.get("solution", ""),
            opp.get("target_audience", ""),
            opp.get("business_model", ""),
            opp.get("validation_status", "idea"),
            opp.get("validation_score", 0.0),
            opp.get("estimated_revenue", 0.0),
            opp.get("estimated_cost", 0.0),
            opp.get("confidence", 0.5),
            json.dumps(opp.get("source_data", {})),
            opp.get("created_at", now),
            now,
        ))
        await self._conn.commit()
        return opp_id

    async def get_opportunity(self, opp_id: str) -> dict[str, Any] | None:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        cursor = await self._conn.execute("SELECT * FROM opportunities WHERE id = ?", (opp_id,))
        row = await cursor.fetchone()
        return self._row_to_opportunity(row) if row else None

    async def list_opportunities(self, status: str | None = None) -> list[dict[str, Any]]:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        if status:
            cursor = await self._conn.execute(
                "SELECT * FROM opportunities WHERE validation_status = ? ORDER BY validation_score DESC", (status,)
            )
        else:
            cursor = await self._conn.execute("SELECT * FROM opportunities ORDER BY validation_score DESC")

        rows = await cursor.fetchall()
        return [self._row_to_opportunity(row) for row in rows]

    async def record_metric(self, metric: dict[str, Any]) -> str:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        metric_id = str(metric.get("id", f"metric_{datetime.now(UTC).timestamp()}"))

        await self._conn.execute("""
            INSERT INTO metrics (id, opportunity_id, name, value, unit, period, recorded_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        """, (
            metric_id,
            metric.get("opportunity_id", ""),
            metric.get("name", ""),
            metric.get("value", 0.0),
            metric.get("unit", ""),
            metric.get("period", ""),
            datetime.now(UTC).isoformat(),
        ))
        await self._conn.commit()
        return metric_id

    async def record_decision(self, decision: dict[str, Any]) -> str:
        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        decision_id = str(decision.get("id", f"dec_{datetime.now(UTC).timestamp()}"))

        await self._conn.execute("""
            INSERT INTO decisions (id, opportunity_id, decision_type, description, rationale, 
                                   alternatives, outcome, confidence, made_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            decision_id,
            decision.get("opportunity_id", ""),
            decision.get("decision_type", ""),
            decision.get("description", ""),
            decision.get("rationale", ""),
            json.dumps(decision.get("alternatives", [])),
            decision.get("outcome", ""),
            decision.get("confidence", 0.5),
            datetime.now(UTC).isoformat(),
        ))
        await self._conn.commit()
        return decision_id

    def _row_to_opportunity(self, row: aiosqlite.Row) -> dict[str, Any]:
        return {
            "id": row[0],
            "title": row[1],
            "description": row[2],
            "market": row[3],
            "problem": row[4],
            "solution": row[5],
            "target_audience": row[6],
            "business_model": row[7],
            "validation_status": row[8],
            "validation_score": row[9],
            "estimated_revenue": row[10],
            "estimated_cost": row[11],
            "confidence": row[12],
            "source_data": json.loads(row[13]),
            "created_at": row[14],
            "updated_at": row[15],
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
