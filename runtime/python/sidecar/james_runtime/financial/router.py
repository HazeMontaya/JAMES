"""Domain-specific financial capability router."""
from __future__ import annotations
from typing import Optional
from .kronos_adapter import KronosAdapter
from .schemas import ForecastRequest, ForecastResult

class FinancialRouter:
    def __init__(self, kronos: Optional[KronosAdapter] = None):
        self.kronos = kronos or KronosAdapter()

    async def forecast(self, request: ForecastRequest) -> ForecastResult:
        if request.model_id.lower().startswith("neoquasar/kronos"):
            return await self.kronos.forecast(request)
        raise ValueError(
            f"No financial forecasting provider registered for model '{request.model_id}'"
        )

    def providers(self) -> list[str]:
        return ["kronos"]
