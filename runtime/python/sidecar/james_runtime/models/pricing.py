"""Pricing registry for model routing and cost accounting."""
from dataclasses import dataclass
from decimal import Decimal
from typing import Dict, Optional


@dataclass(frozen=True)
class ModelPricing:
    model_id: str
    input_per_1k: Decimal = Decimal("0")
    output_per_1k: Decimal = Decimal("0")


class PricingRegistry:
    def __init__(self):
        self._pricing: Dict[str, ModelPricing] = {}
        self.load_defaults()

    def load_defaults(self) -> None:
        # Local inference is zero-cost from the API perspective.
        for model_id in (
            "llama-3.1-8b-instruct", "llama-3.1-8b-instruct-q4",
            "llama-3.2-3b-instruct", "qwen2.5-coder-7b-instruct",
            "qwen2.5-3b-instruct", "mistral-7b-instruct",
            "llama-3.3-70b-instruct", "deepseek-r1-distill-qwen-7b",
        ):
            self._pricing.setdefault(model_id, ModelPricing(model_id))
        self._pricing.setdefault(
            "gpt-4o",
            ModelPricing("gpt-4o", Decimal("0.005"), Decimal("0.015")),
        )

    def get_pricing(self, model_id: str) -> Optional[ModelPricing]:
        return self._pricing.get(model_id)

    def get_cost(self, model_id: str) -> Optional[float]:
        pricing = self._pricing.get(model_id)
        if pricing is None:
            return None
        return float(pricing.input_per_1k + pricing.output_per_1k)

    def calculate_cost(self, model_id: str, tokens_in: int, tokens_out: int) -> Decimal:
        pricing = self._pricing.get(model_id)
        if pricing is None:
            return Decimal("0")
        return (
            Decimal(tokens_in) / Decimal(1000) * pricing.input_per_1k
            + Decimal(tokens_out) / Decimal(1000) * pricing.output_per_1k
        )
