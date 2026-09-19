"""Durable Financial Cortex state store backed by SQLite."""
from __future__ import annotations
import json
import sqlite3
from pathlib import Path
from typing import Any, Optional
from .schemas import ForecastResult

class FinancialStateStore:
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
            con.execute("""CREATE TABLE IF NOT EXISTS forecasts (
                request_id TEXT PRIMARY KEY,
                symbol TEXT NOT NULL,
                timeframe TEXT NOT NULL,
                model_id TEXT NOT NULL,
                generated_at TEXT NOT NULL,
                device TEXT NOT NULL,
                sample_count INTEGER NOT NULL,
                context_bars INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            )""")
            con.commit()

    def save_forecast(self, result: ForecastResult) -> None:
        payload = result.model_dump_json()
        with self._connect() as con:
            con.execute("""INSERT OR REPLACE INTO forecasts
                (request_id,symbol,timeframe,model_id,generated_at,device,sample_count,context_bars,payload_json)
                VALUES (?,?,?,?,?,?,?,?,?)""",
                (result.request_id,result.symbol,result.timeframe,result.model_id,
                 result.generated_at.isoformat(),result.device,result.sample_count,
                 result.context_bars,payload))
            con.commit()

    def get_forecast(self, request_id: str) -> Optional[ForecastResult]:
        with self._connect() as con:
            row = con.execute("SELECT payload_json FROM forecasts WHERE request_id=?", (request_id,)).fetchone()
        return ForecastResult.model_validate_json(row["payload_json"]) if row else None

    def latest(self, symbol: str, limit: int = 20) -> list[ForecastResult]:
        with self._connect() as con:
            rows = con.execute(
                "SELECT payload_json FROM forecasts WHERE symbol=? ORDER BY generated_at DESC LIMIT ?",
                (symbol, limit),
            ).fetchall()
        return [ForecastResult.model_validate_json(r["payload_json"]) for r in rows]
