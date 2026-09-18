import pytest
from james_runtime.financial import FinancialCortexService, FinancialEvent, ForecastRequest, MarketBar, MarketSeries

class FakeRouter:
    async def forecast(self, request):
        from james_runtime.financial.schemas import ForecastResult
        from datetime import datetime, timezone
        return ForecastResult(
            request_id="test-request", model_id=request.model_id, symbol=request.series.symbol,
            timeframe=request.series.timeframe, context_bars=len(request.series.bars),
            forecast=[], generated_at=datetime.now(timezone.utc), device="test",
            sample_count=request.sample_count,
        )

@pytest.mark.asyncio
async def test_service_emits_runtime_events():
    events = []
    async def sink(event: FinancialEvent):
        events.append(event.event_type)
    bars = [MarketBar(timestamp=f"2026-01-01T00:0{i}:00Z", open=1, high=2, low=0, close=1) for i in range(2)]
    request = ForecastRequest(series=MarketSeries(symbol="TEST", timeframe="1m", bars=bars), pred_len=1)
    result = await FinancialCortexService(FakeRouter(), sink).forecast(request)
    assert result.request_id == "test-request"
    assert events == ["MARKET_DATA_RECEIVED", "KRONOS_INFERENCE", "KRONOS_FORECAST_READY"]
