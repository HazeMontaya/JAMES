"""Tamper-evident local audit log for Financial Cortex execution."""
from __future__ import annotations
import hashlib
import json
import sqlite3
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from uuid import uuid4

class FinancialAuditLog:
    def __init__(self, path: str | Path = "data/james-financial.sqlite3"):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._init()

    def _connect(self) -> sqlite3.Connection:
        con = sqlite3.connect(self.path)
        con.row_factory = sqlite3.Row
        return con

    def _init(self) -> None:
        with self._connect() as con:
            con.execute("""CREATE TABLE IF NOT EXISTS financial_audit (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                correlation_id TEXT,
                payload_json TEXT NOT NULL,
                previous_hash TEXT NOT NULL,
                record_hash TEXT NOT NULL UNIQUE
            )""")
            con.commit()

    def append(self, event: Any) -> str:
        payload = json.dumps(event.payload, sort_keys=True, separators=(",", ":"), default=str)
        timestamp = event.timestamp.astimezone(timezone.utc).isoformat()
        with self._connect() as con:
            row = con.execute("SELECT record_hash FROM financial_audit ORDER BY sequence DESC LIMIT 1").fetchone()
            previous = row["record_hash"] if row else "GENESIS"
            canonical = "|".join([previous,event.event_id,timestamp,event.event_type,event.correlation_id or "",payload])
            record_hash = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
            con.execute("""INSERT INTO financial_audit
                (event_id,timestamp,event_type,correlation_id,payload_json,previous_hash,record_hash)
                VALUES (?,?,?,?,?,?,?)""",
                (event.event_id,timestamp,event.event_type,event.correlation_id,payload,previous,record_hash))
            con.commit()
        return record_hash

    def verify(self) -> bool:
        with self._connect() as con:
            rows = con.execute("SELECT * FROM financial_audit ORDER BY sequence").fetchall()
        previous = "GENESIS"
        for row in rows:
            canonical = "|".join([previous,row["event_id"],row["timestamp"],row["event_type"],row["correlation_id"] or "",row["payload_json"]])
            if hashlib.sha256(canonical.encode("utf-8")).hexdigest() != row["record_hash"] or row["previous_hash"] != previous:
                return False
            previous = row["record_hash"]
        return True
