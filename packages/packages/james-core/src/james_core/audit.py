"""Append-only audit log for JAMES (SQLite-backed, async)."""

from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path
from typing import Any

import aiosqlite
import structlog

from .security import redact_secrets, sanitize_text

logger = structlog.get_logger()

__all__ = ["AuditLog", "AuditOutcome", "AuditSeverity"]


class AuditOutcome(StrEnum):
    SUCCESS = "success"
    FAILURE = "failure"
    DENIED = "denied"


class AuditSeverity(StrEnum):
    INFO = "info"
    WARNING = "warning"
    ERROR = "error"
    CRITICAL = "critical"


class AuditLog:
    """Async append-only audit trail persisted to SQLite.

    Details are redacted through :func:`james_core.security.redact_secrets`
    before storage so credentials never land in the log.
    """

    def __init__(self, db_path: str = "~/.james/audit/audit.db"):
        self._db_path = Path(db_path).expanduser()
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn: aiosqlite.Connection | None = None

    async def initialize(self) -> None:
        self._conn = await aiosqlite.connect(str(self._db_path))
        await self._conn.execute("PRAGMA journal_mode=WAL")
        await self._create_tables()

    async def _create_tables(self) -> None:
        assert self._conn is not None
        await self._conn.execute("""
            CREATE TABLE IF NOT EXISTS audit_entries (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                actor TEXT NOT NULL,
                action TEXT NOT NULL,
                resource TEXT NOT NULL,
                outcome TEXT NOT NULL,
                severity TEXT NOT NULL,
                source TEXT,
                duration_ms REAL,
                details TEXT NOT NULL
            )
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_entries(created_at)
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_entries(actor)
        """)
        await self._conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_entries(action)
        """)
        await self._conn.commit()

    async def record(
        self,
        *,
        actor: str,
        action: str,
        resource: str,
        outcome: AuditOutcome = AuditOutcome.SUCCESS,
        severity: AuditSeverity = AuditSeverity.INFO,
        details: dict[str, Any] | None = None,
        source: str = "core",
        duration_ms: float | None = None,
    ) -> str:
        import hashlib
        import json
        from uuid import uuid4

        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        entry_id = str(uuid4())
        created_at = datetime.now(UTC).isoformat()
        safe_details = redact_secrets(details or {})
        details_json = json.dumps(safe_details, ensure_ascii=False, default=str, sort_keys=True)
        details_json = sanitize_text(details_json)

        await self._conn.execute(
            """INSERT OR REPLACE INTO audit_entries
               (id, created_at, actor, action, resource, outcome, severity, source, duration_ms, details)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (
                entry_id,
                created_at,
                sanitize_text(actor, 256),
                sanitize_text(action, 256),
                sanitize_text(resource, 4096),
                outcome.value,
                severity.value,
                sanitize_text(source, 128),
                duration_ms,
                details_json,
            ),
        )
        await self._conn.commit()

        # Op-level fingerprint for tamper-evidence (append-only chain).
        digest = hashlib.sha256(
            f"{entry_id}|{created_at}|{actor}|{action}|{details_json}".encode()
        ).hexdigest()

        logger.debug(
            "audit recorded",
            actor=actor,
            action=action,
            outcome=outcome.value,
            fingerprint=digest[:16],
        )
        return entry_id

    async def query(
        self,
        limit: int = 50,
        actor: str | None = None,
        action: str | None = None,
        outcome: AuditOutcome | None = None,
        severity: AuditSeverity | None = None,
    ) -> list[dict[str, Any]]:
        import json

        if not self._conn:
            await self.initialize()
        assert self._conn is not None

        clauses: list[str] = []
        params: list[Any] = []
        if actor:
            clauses.append("actor = ?")
            params.append(actor)
        if action:
            clauses.append("action = ?")
            params.append(action)
        if outcome:
            clauses.append("outcome = ?")
            params.append(outcome.value)
        if severity:
            clauses.append("severity = ?")
            params.append(severity.value)
        where = f"WHERE {' AND '.join(clauses)}" if clauses else ""
        params.append(max(1, min(limit, 500)))

        cursor = await self._conn.execute(
            f"""SELECT id, created_at, actor, action, resource, outcome, severity, source, duration_ms, details
                FROM audit_entries {where}
                ORDER BY created_at DESC
                LIMIT ?""",
            params,
        )
        rows = await cursor.fetchall()
        await cursor.close()

        entries: list[dict[str, Any]] = []
        for row in rows:
            try:
                parsed = json.loads(row[9])
            except json.JSONDecodeError:
                parsed = {}
            entries.append(
                {
                    "id": row[0],
                    "created_at": row[1],
                    "actor": row[2],
                    "action": row[3],
                    "resource": row[4],
                    "outcome": row[5],
                    "severity": row[6],
                    "source": row[7],
                    "duration_ms": row[8],
                    "details": parsed,
                }
            )
        return entries

    async def close(self) -> None:
        if self._conn is not None:
            await self._conn.close()
            self._conn = None