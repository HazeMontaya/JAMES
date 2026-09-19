from james_runtime.financial.audit import FinancialAuditLog
from james_runtime.financial.events import FinancialEvent
from james_runtime.financial.state import FinancialStateStore
from james_runtime.financial.schemas import ForecastPoint, ForecastResult

def result():
    return ForecastResult(
        request_id="r1", model_id="kronos", symbol="TEST", timeframe="1m",
        context_bars=2, forecast=[ForecastPoint(
            timestamp="2026-01-01T00:02:00Z", open=2, high=3, low=1, close=2.5
        )], generated_at="2026-01-01T00:00:00Z", device="cpu", sample_count=1
    )

def test_state_roundtrip(tmp_path):
    store=FinancialStateStore(tmp_path/"state.sqlite3")
    store.save_forecast(result())
    assert store.get_forecast("r1").symbol == "TEST"
    assert len(store.latest("TEST")) == 1

def test_audit_chain(tmp_path):
    audit=FinancialAuditLog(tmp_path/"audit.sqlite3")
    audit.append(FinancialEvent(event_type="A", correlation_id="c", payload={"x":1}))
    audit.append(FinancialEvent(event_type="B", correlation_id="c", payload={"x":2}))
    assert audit.verify()
