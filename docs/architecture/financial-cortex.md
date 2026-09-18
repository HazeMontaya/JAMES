# JAMES Financial Cortex

Status: executable first implementation
Date: 2026-09-19

Financial forecasting is a specialist capability. It is not part of the generic
chat ModelRouter and is not a JAMES Core subsystem.

Capability Broker -> market.forecast -> FinancialRouter -> KronosAdapter ->
Kronos Predictor -> ForecastResult -> verification/audit/state -> Void.

The adapter follows the upstream Kronos API:
- KronosTokenizer.from_pretrained(...)
- Kronos.from_pretrained(...)
- KronosPredictor(...)
- predictor.predict(...)

Initial model targets:
- NeoQuasar/Kronos-mini
- NeoQuasar/Kronos-small
- NeoQuasar/Kronos-base

The adapter fails explicitly if Kronos is not installed. It never returns
synthetic market data or simulated forecasts.

Runtime separation:
- ai.inference -> ModelRouter -> RuntimeRouter -> EngineAdapter
- market.forecast -> FinancialRouter -> KronosAdapter -> PyTorch/Kronos

Verification requirements:
1. Validate OHLC input.
2. Require strictly increasing timestamps.
3. Record model/provider identity.
4. Make context length explicit.
5. Validate output schema.
6. Tag forecasts as model output, not execution instructions.
7. Record runtime/audit events.
8. Keep portfolio/order execution as a separate future capability.

Expected Brain/Void events:
MARKET_DATA_RECEIVED
KRONOS_LOADING
KRONOS_INFERENCE
KRONOS_FORECAST_READY
FORECAST_VALIDATED

No UI animation is allowed to manufacture runtime state.
