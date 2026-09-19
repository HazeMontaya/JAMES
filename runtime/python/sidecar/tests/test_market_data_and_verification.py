import pytest
from james_runtime.financial.market_data import CsvMarketDataProvider
from james_runtime.financial.schemas import ForecastPoint, ForecastResult
from james_runtime.financial.verification import verify_forecast

@pytest.mark.asyncio
async def test_csv_provider(tmp_path):
    path = tmp_path / "bars.csv"
    path.write_text("timestamp,open,high,low,close,volume\n2026-01-01T00:00:00Z,1,2,0,1.5,10\n2026-01-01T00:01:00Z,1.5,2.5,1,2,12\n", encoding="utf-8")
    series = await CsvMarketDataProvider().load("TEST", "1m", str(path))
    assert len(series.bars) == 2
    assert series.bars[-1].close == 2

def test_forecast_verification():
    result = ForecastResult(
        request_id="r", model_id="m", symbol="TEST", timeframe="1m",
        context_bars=2, forecast=[
            ForecastPoint(timestamp="2026-01-01T00:02:00Z", open=2, high=3, low=1, close=2.5)
        ], generated_at="2026-01-01T00:00:00Z", device="cpu", sample_count=1
    )
    assert verify_forecast(result).valid
