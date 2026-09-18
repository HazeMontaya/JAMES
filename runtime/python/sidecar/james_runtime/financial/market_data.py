"""Market-data ingestion contracts and a deterministic CSV provider."""
from __future__ import annotations
import csv
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Protocol
from .schemas import MarketBar, MarketSeries

class MarketDataProvider(Protocol):
    async def load(self, symbol: str, timeframe: str, source: str) -> MarketSeries: ...

@dataclass
class CsvMarketDataProvider:
    """Loads OHLCV/amount data from a local CSV file."""
    def __init__(self, timestamp_column: str = "timestamp"):
        self.timestamp_column = timestamp_column

    async def load(self, symbol: str, timeframe: str, source: str) -> MarketSeries:
        path = Path(source).expanduser()
        if not path.is_file():
            raise FileNotFoundError(f"Market data file not found: {path}")
        bars: list[MarketBar] = []
        with path.open("r", encoding="utf-8", newline="") as fh:
            reader = csv.DictReader(fh)
            required = {"open", "high", "low", "close"}
            missing = required - set(reader.fieldnames or [])
            if missing:
                raise ValueError(f"Market CSV missing columns: {sorted(missing)}")
            for row in reader:
                ts = row.get(self.timestamp_column)
                if not ts:
                    raise ValueError("Market CSV contains a row without timestamp")
                bars.append(MarketBar(
                    timestamp=datetime.fromisoformat(ts.replace("Z", "+00:00")),
                    open=float(row["open"]), high=float(row["high"]),
                    low=float(row["low"]), close=float(row["close"]),
                    volume=float(row["volume"]) if row.get("volume") not in (None, "") else None,
                    amount=float(row["amount"]) if row.get("amount") not in (None, "") else None,
                ))
        return MarketSeries(symbol=symbol, timeframe=timeframe, bars=bars)

@dataclass
class MarketDataRegistry:
    providers: dict[str, MarketDataProvider]

    @classmethod
    def default(cls) -> "MarketDataRegistry":
        return cls({"csv": CsvMarketDataProvider()})

    def get(self, provider: str) -> MarketDataProvider:
        try:
            return self.providers[provider]
        except KeyError as exc:
            raise ValueError(f"Unknown market-data provider: {provider}") from exc
