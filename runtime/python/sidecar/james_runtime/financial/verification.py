"""Integrity checks for financial forecasts."""
from __future__ import annotations
from pydantic import BaseModel
from .schemas import ForecastResult

class ForecastVerification(BaseModel):
    valid: bool
    checks: dict[str, bool]
    errors: list[str] = []

def verify_forecast(result: ForecastResult) -> ForecastVerification:
    checks: dict[str, bool] = {
        "non_empty": bool(result.forecast),
        "time_ordered": True,
        "ohlc_valid": True,
        "finite_values": True,
    }
    errors: list[str] = []
    previous = None
    for point in result.forecast:
        if previous is not None and point.timestamp <= previous:
            checks["time_ordered"] = False
            errors.append("forecast timestamps must be strictly increasing")
        previous = point.timestamp
        if point.high < point.low or point.close < point.low or point.close > point.high:
            checks["ohlc_valid"] = False
            errors.append("forecast contains invalid OHLC bounds")
        values = [point.open, point.high, point.low, point.close]
        if point.volume is not None: values.append(point.volume)
        if point.amount is not None: values.append(point.amount)
        if not all(__import__("math").isfinite(v) for v in values):
            checks["finite_values"] = False
            errors.append("forecast contains non-finite values")
    return ForecastVerification(valid=all(checks.values()), checks=checks, errors=errors)
