from datetime import datetime, timedelta, timezone
import pytest

from james_runtime.financial.schemas import MarketBar, MarketSeries, ForecastRequest
from james_runtime.financial.router import FinancialRouter


def make_series(count=4):
    start = datetime(2026, 1, 1, tzinfo=timezone.utc)
    bars = []
    for i in range(count):
        close = 100.0 + i
        bars.append(MarketBar(
            timestamp=start + timedelta(minutes=5 * i),
            open=close - 0.5,
            high=close + 1.0,
            low=close - 1.0,
            close=close,
            volume=1000.0 + i,
            amount=100000.0 + i,
        ))
    return MarketSeries(symbol="TEST", timeframe="5m", bars=bars)


def test_market_series_requires_strict_time_order():
    series = make_series()
    assert series.bars[0].timestamp < series.bars[-1].timestamp


def test_forecast_request_has_explicit_model_and_context():
    request = ForecastRequest(
        series=make_series(),
        pred_len=3,
        model_id="NeoQuasar/Kronos-small",
        max_context=4,
    )
    assert request.model_id == "NeoQuasar/Kronos-small"
    assert request.max_context == 4


@pytest.mark.asyncio
async def test_router_rejects_unknown_financial_provider():
    router = FinancialRouter()
    request = ForecastRequest(
        series=make_series(),
        pred_len=1,
        model_id="unknown/provider",
    )
    with pytest.raises(ValueError):
        await router.forecast(request)
