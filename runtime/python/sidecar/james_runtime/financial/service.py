"""Executable Financial Cortex service boundary."""
from __future__ import annotations
from typing import Awaitable, Callable, Optional
from uuid import uuid4
from .events import FinancialEvent
from .router import FinancialRouter
from .schemas import ForecastRequest, ForecastResult
from .verification import verify_forecast

EventSink = Callable[[FinancialEvent], Awaitable[None]]

class FinancialCortexService:
    def __init__(self, router: Optional[FinancialRouter] = None, event_sink: Optional[EventSink] = None):
        self.router = router or FinancialRouter()
        self.event_sink = event_sink

    async def _emit(self, event_type: str, correlation_id: str, payload: dict) -> None:
        if self.event_sink:
            await self.event_sink(FinancialEvent(
                event_type=event_type, correlation_id=correlation_id, payload=payload
            ))

    async def forecast(self, request: ForecastRequest) -> ForecastResult:
        correlation_id = str(uuid4())
        await self._emit("MARKET_DATA_RECEIVED", correlation_id, {
            "symbol": request.series.symbol,
            "timeframe": request.series.timeframe,
            "bars": len(request.series.bars),
        })
        await self._emit("KRONOS_INFERENCE", correlation_id, {
            "model_id": request.model_id,
            "pred_len": request.pred_len,
        })
        result = await self.router.forecast(request)
        verification = verify_forecast(result)
        await self._emit("FORECAST_VERIFIED", correlation_id, {"
            "request_id": result.request_id, "valid": verification.valid,
            "checks": verification.checks, "errors": verification.errors,
        })
        if not verification.valid:
            raise ValueError("Financial forecast failed integrity verification")
        await self._emit("KRONOS_FORECAST_READY", correlation_id, {
            "request_id": result.request_id,
            "model_id": result.model_id,
            "symbol": result.symbol,
            "forecast_points": len(result.forecast),
        })
        return result
