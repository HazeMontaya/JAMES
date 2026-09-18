"""JAMES Runtime Model Router - Task to Model Selection"""
import logging
from typing import List, Optional, Dict, Any
from dataclasses import dataclass, field
from james_runtime.core.requests import RoutingRequest, RoutingDecision, ModelCapability
from james_runtime.models.registry import ModelRegistry, ModelSpec
from james_runtime.models.pricing import PricingRegistry
from james_runtime.config import RouterConfig, QualityTier

logger = logging.getLogger(__name__)


@dataclass
class ScoredModel:
    model: ModelSpec
    score: float
    reasoning: str
    estimated_cost: float
    estimated_latency_ms: int


class ModelRouter:
    """Routes tasks to appropriate models based on requirements"""
    
    def __init__(
        self, 
        model_registry: ModelRegistry, 
        pricing_registry: PricingRegistry,
        config: RouterConfig
    ):
        self.registry = model_registry
        self.pricing = pricing_registry
        self.config = config
    
    async def route(self, request: RoutingRequest) -> RoutingDecision:
        """Route a task to the best model"""
        candidates = self._filter_candidates(request)
        
        if not candidates:
            raise ValueError(f"No models match requirements: {request.required_capabilities}")
        
        scored = self._score_candidates(candidates, request)
        best = scored[0]
        
        return RoutingDecision(
            model_id=best.model.id,
            provider=best.model.provider,
            reasoning=best.reasoning,
            fallback_chain=[s.model.id for s in scored[1:4]],
            estimated_cost_per_1k=best.estimated_cost,
            estimated_latency_ms=best.estimated_latency_ms,
            required_capabilities=request.required_capabilities,
        )
    
    def _filter_candidates(self, request: RoutingRequest) -> List[ModelSpec]:
        """Filter models by hard requirements"""
        all_models = self.registry.list_available()

        # Explicit model request takes precedence
        if request.preferred_model:
            preferred = self.registry.get_model_spec(request.preferred_model)
            if preferred and preferred.provider == "local":
                return [preferred]

        candidates = []
        for model in all_models:
            # Check capabilities
            if request.required_capabilities:
                if not all(cap in model.capabilities for cap in request.required_capabilities):
                    continue
            
            # Check privacy
            if request.privacy == "local_only" and model.provider != "local":
                continue
            elif request.privacy == "prefer_local" and model.provider != "local":
                # Allow but deprioritize
                pass
            
            # Check context length
            if request.context_length_needed and model.max_context < request.context_length_needed:
                continue
            
            # Check cost budget
            if request.cost_budget_per_1k:
                cost = self.pricing.get_cost(model.id)
                if cost and cost > request.cost_budget_per_1k:
                    continue
            
            # Check quality tier
            if request.quality_tier == QualityTier.BEST and model.quality_tier != QualityTier.BEST:
                continue
            elif request.quality_tier == QualityTier.FAST and model.quality_tier == QualityTier.BEST:
                # Allow but may prefer smaller
                pass
            
            candidates.append(model)
        
        return candidates
    
    def _score_candidates(self, candidates: List[ModelSpec], request: RoutingRequest) -> List[ScoredModel]:
        scored = []
        
        for model in candidates:
            # Quality score (0-1)
            quality_score = self._quality_score(model, request)
            
            # Speed score (0-1, higher = faster)
            speed_score = self._speed_score(model, request)
            
            # Cost efficiency (0-1, higher = cheaper per quality)
            cost_score = self._cost_score(model, request)
            
            # Privacy match (0-1)
            privacy_score = 1.0 if model.provider == "local" else 0.5
            if request.privacy == "local_only" and model.provider != "local":
                privacy_score = 0.0
            
            # Weighted total
            total_score = (
                self.config.quality_weight * quality_score +
                self.config.speed_weight * speed_score +
                self.config.cost_weight * cost_score +
                self.config.privacy_weight * privacy_score
            )
            
            # Estimate cost and latency
            estimated_cost = self.pricing.get_cost(model.id) or 0.0
            estimated_latency = self._estimate_latency(model)
            
            reasoning = self._generate_reasoning(model, request, quality_score, speed_score, cost_score)
            
            scored.append(ScoredModel(
                model=model,
                score=total_score,
                reasoning=reasoning,
                estimated_cost=estimated_cost,
                estimated_latency_ms=estimated_latency,
            ))
        
        # Sort by score descending
        scored.sort(key=lambda s: s.score, reverse=True)
        return scored
    
    def _quality_score(self, model: ModelSpec, request: RoutingRequest) -> float:
        """Score model quality (0-1)"""
        base = model.quality_score if hasattr(model, 'quality_score') else 0.5
        
        # Boost for required capabilities
        if request.required_capabilities:
            matching = sum(1 for cap in request.required_capabilities if cap in model.capabilities)
            cap_bonus = min(0.3, matching * 0.1)
            base += cap_bonus
        
        return min(1.0, base)
    
    def _speed_score(self, model: ModelSpec, request: RoutingRequest) -> float:
        """Score model speed (0-1, higher = faster)"""
        # Smaller models are generally faster
        params_b = getattr(model, 'parameters_b', 7)
        if params_b <= 7:
            return 1.0
        elif params_b <= 14:
            return 0.8
        elif params_b <= 32:
            return 0.6
        elif params_b <= 70:
            return 0.4
        else:
            return 0.2
    
    def _cost_score(self, model: ModelSpec, request: RoutingRequest) -> float:
        """Cost efficiency score"""
        if request.cost_budget_per_1k is None:
            return 0.5
        
        cost = self.pricing.get_cost(model.id) or 0.0
        if cost == 0:
            return 1.0  # Free models get max score
        
        # Normalize by budget
        ratio = cost / request.cost_budget_per_1k if request.cost_budget_per_1k > 0 else 1.0
        return max(0.0, 1.0 - ratio)
    
    def _estimate_latency(self, model: ModelSpec) -> int:
        params_b = getattr(model, 'parameters_b', 7)
        base_latency = 50  # ms for 7B
        return int(50 * (params_b / 7) ** 0.8)
    
    def _generate_reasoning(self, model: ModelSpec, request: RoutingRequest, quality: float, speed: float, cost: float) -> str:
        reasons = []
        if model.provider == "local":
            reasons.append("local execution")
        if request.required_capabilities:
            matching = [c for c in request.required_capabilities if c in model.capabilities]
            if matching:
                reasons.append(f"capabilities: {', '.join(matching)}")
        if model.provider == "local":
            reasons.append("privacy: local execution")
        return f"Selected {model.id} ({model.parameters_b}B params): {', '.join(reasons)}"