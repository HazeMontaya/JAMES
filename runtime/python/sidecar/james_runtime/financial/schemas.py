"""Stable contracts for JAMES financial forecasting."""
from __future__ import annotations
from datetime import datetime
from typing import List, Optional
from pydantic import BaseModel, Field, field_validator

class MarketBar(BaseModel):
    timestamp: datetime
    open: float
    high: float
    low: float
    close: float
    volume: Optional[float] = None
    amount: Optional[float] = None

    @field_validator("high")
    @classmethod
    def high_not_below_low(cls, value: float, info):
        low = info.data.get("low")
        if low is not None and value < low:
            raise ValueError("high must be >= low")
        return value

    @field_validator("close")
    @classmethod
    def close_inside_bar(cls, value: float, info):
        low, high = info.data.get("low"), info.data.get("high")
        if low is not None and value < low:
            raise ValueError("close must be >= low")
        if high is not None and value > high:
            raise ValueError("close must be <= high")
        return value

class MarketSeries(BaseModel):
    symbol: str = Field(min_length=1)
    timeframe: str = Field(min_length=1)
    bars: List[MarketBar] = Field(min_length=1)

    @field_validator("bars")
    @classmethod
    def timestamps_sorted(cls, value):
        if any(value[i].timestamp >= value[i + 1].timestamp for i in range(len(value) - 1)):
            raise ValueError("bars must be strictly time-ordered")
        return value

class ForecastRequest(BaseModel):
    series: MarketSeries
    pred_len: int = Field(gt=0, le=2048)
    temperature: float = Field(1.0, gt=0.0, le=2.0)
    top_k: int = Field(0, ge=0)
    top_p: float = Field(0.9, gt=0.0, le=1.0)
    sample_count: int = Field(1, ge=1, le=32)
    model_id: str = "NeoQuasar/Kronos-small"
    max_context: int = Field(512, gt=0, le=4096)
    device: Optional[str] = None

class ForecastPoint(BaseModel):
    timestamp: datetime
    open: float
    high: float
    low: float
    close: float
    volume: Optional[float] = None
    amount: Optional[float] = None

class ForecastResult(BaseModel):
    request_id: str
    model_id: str
    symbol: str
    timeframe: str
    context_bars: int
    forecast: List[ForecastPoint]
    generated_at: datetime
    device: str
    sample_count: int
