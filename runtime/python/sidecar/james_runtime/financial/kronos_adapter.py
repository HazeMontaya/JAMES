"""Concrete adapter for the upstream Kronos financial model."""
from __future__ import annotations
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Optional
from uuid import uuid4
import pandas as pd
from .schemas import ForecastRequest, ForecastResult, ForecastPoint

@dataclass
class KronosAdapterConfig:
    model_id: str = "NeoQuasar/Kronos-small"
    tokenizer_id: str = "NeoQuasar/Kronos-Tokenizer-base"
    max_context: int = 512
    device: Optional[str] = None

class KronosAdapter:
    def __init__(self, config: Optional[KronosAdapterConfig] = None):
        self.config = config or KronosAdapterConfig()
        self._predictor: Any = None
        self._loaded_model_id: Optional[str] = None

    @property
    def loaded(self) -> bool:
        return self._predictor is not None

    def _load(self, model_id: str, device: Optional[str], max_context: int) -> None:
        try:
            from model import Kronos, KronosPredictor, KronosTokenizer
        except ImportError as exc:
            raise RuntimeError(
                "Kronos is not installed. Install the upstream Kronos package "
                "and PyTorch before enabling market.forecast."
            ) from exc

        tokenizer_id = self.config.tokenizer_id
        if "mini" in model_id.lower():
            tokenizer_id = "NeoQuasar/Kronos-Tokenizer-2k"
        tokenizer = KronosTokenizer.from_pretrained(tokenizer_id)
        model = Kronos.from_pretrained(model_id)
        self._predictor = KronosPredictor(
            model, tokenizer, device=device, max_context=max_context
        )
        self._loaded_model_id = model_id

    async def forecast(self, request: ForecastRequest) -> ForecastResult:
        if not self.loaded or self._loaded_model_id != request.model_id:
            self._load(request.model_id, request.device, request.max_context)

        bars = request.series.bars[-request.max_context:]
        if len(bars) < 2:
            raise ValueError("Kronos requires at least two historical bars")

        timestamps = pd.DatetimeIndex([b.timestamp for b in bars])
        delta = timestamps[-1] - timestamps[-2]
        if delta <= pd.Timedelta(0):
            raise ValueError("Historical timestamps must be strictly increasing")
        y_timestamp = pd.DatetimeIndex(
            [timestamps[-1] + delta * (i + 1) for i in range(request.pred_len)]
        )

        x_df = pd.DataFrame([{
            "open": b.open, "high": b.high, "low": b.low, "close": b.close,
            "volume": 0.0 if b.volume is None else b.volume,
            "amount": 0.0 if b.amount is None else b.amount,
        } for b in bars])

        pred_df = self._predictor.predict(
            df=x_df,
            x_timestamp=timestamps,
            y_timestamp=y_timestamp,
            pred_len=request.pred_len,
            T=request.temperature,
            top_k=request.top_k,
            top_p=request.top_p,
            sample_count=request.sample_count,
            verbose=False,
        )

        forecast = []
        for i, (_, row) in enumerate(pred_df.iterrows()):
            forecast.append(ForecastPoint(
                timestamp=pd.Timestamp(y_timestamp[i]).to_pydatetime(),
                open=float(row["open"]), high=float(row["high"]),
                low=float(row["low"]), close=float(row["close"]),
                volume=float(row["volume"]) if "volume" in row else None,
                amount=float(row["amount"]) if "amount" in row else None,
            ))

        return ForecastResult(
            request_id=str(uuid4()),
            model_id=request.model_id,
            symbol=request.series.symbol,
            timeframe=request.series.timeframe,
            context_bars=len(bars),
            forecast=forecast,
            generated_at=datetime.now(timezone.utc),
            device=str(getattr(self._predictor, "device", request.device or "auto")),
            sample_count=request.sample_count,
        )
