"""JAMES Runtime Telemetry and Metrics"""
import logging
import time
from typing import Dict, Optional
from dataclasses import dataclass, field
from collections import defaultdict

logger = logging.getLogger(__name__)

try:
    from prometheus_client import Counter, Histogram, Gauge, CollectorRegistry
    PROMETHEUS_AVAILABLE = True
except ImportError:
    PROMETHEUS_AVAILABLE = False
    Counter = None  # type: ignore
    Histogram = None  # type: ignore
    Gauge = None  # type: ignore
    CollectorRegistry = None  # type: ignore


class _NullMetric:
    """No-op metric for when prometheus_client is unavailable"""
    def __init__(self, *args, **kwargs): pass
    def inc(self, *args, **kwargs): pass
    def set(self, *args, **kwargs): pass
    def observe(self, *args, **kwargs): pass
    def labels(self, *args, **kwargs): return self
    def clear(self): pass


class MetricsCollector:
    """Collects and exports runtime metrics"""

    def __init__(self, registry: Optional[CollectorRegistry] = None):
        if not PROMETHEUS_AVAILABLE:
            self.registry = None
            self._init_null_metrics()
            return

        self.registry = registry or CollectorRegistry()

        # Request metrics
        self.requests_total = Counter(
            'james_runtime_requests_total',
            'Total number of inference requests',
            ['model', 'engine', 'status'],
            registry=self.registry
        )
        
        self.request_latency = Histogram(
            'james_runtime_request_latency_seconds',
            'Request latency in seconds',
            ['model', 'engine'],
            registry=self.registry
        )
        
        self.request_tokens = Histogram(
            'james_runtime_request_tokens',
            'Number of tokens per request',
            ['model', 'engine', 'type'],  # type: input/output
            registry=self.registry
        )
        
        # Engine metrics
        self.engine_healthy = Gauge(
            'james_runtime_engine_healthy',
            'Engine health status (1=healthy, 0=unhealthy)',
            ['engine'],
            registry=self.registry
        )
        
        self.engine_vram_used = Gauge(
            'james_runtime_engine_vram_used_gb',
            'VRAM used by engine in GB',
            ['engine'],
            registry=self.registry
        )
        
        self.engine_vram_total = Gauge(
            'james_runtime_engine_vram_total_gb',
            'Total VRAM available in GB',
            ['engine'],
            registry=self.registry
        )
        
        self.engine_latency_p50 = Gauge(
            'james_runtime_engine_latency_p50_ms',
            'Engine P50 latency in ms',
            ['engine'],
            registry=self.registry
        )
        
        self.engine_latency_p99 = Gauge(
            'james_runtime_engine_latency_p99_ms',
            'Engine P99 latency in ms',
            ['engine'],
            registry=self.registry
        )
        
        self.engine_error_rate = Gauge(
            'james_runtime_engine_error_rate',
            'Engine error rate',
            ['engine'],
            registry=self.registry
        )
        
        # Routing metrics
        self.routing_decisions = Counter(
            'james_runtime_routing_decisions_total',
            'Total routing decisions',
            ['model', 'engine', 'reason'],
            registry=self.registry
        )
        
        self.fallback_total = Counter(
            'james_runtime_fallback_total',
            'Total fallback activations',
            ['from_engine', 'to_engine', 'reason'],
            registry=self.registry
        )
        
        # Cost metrics
        self.cost_total = Counter(
            'james_runtime_cost_usd_total',
            'Total cost in USD',
            ['model', 'engine', 'agent_id'],
            registry=self.registry
        )
        
        self.budget_usage = Gauge(
            'james_runtime_budget_usage_ratio',
            'Budget usage ratio (0-1)',
            ['budget_type'],
            registry=self.registry
        )
        
        # VRAM metrics
        self.vram_used = Gauge(
            'james_runtime_vram_used_gb',
            'VRAM used in GB',
            registry=self.registry
        )
        
        self.vram_available = Gauge(
            'james_runtime_vram_available_gb',
            'VRAM available in GB',
            registry=self.registry
        )
        
        # Quality metrics
        self.quality_score = Gauge(
            'james_runtime_quality_score',
            'Model quality score from benchmarks',
            ['model', 'benchmark'],
            registry=self.registry
        )
        
        # Internal tracking
        self._latency_buckets = defaultdict(list)
        self._request_counts = defaultdict(int)
        self._error_counts = defaultdict(int)

    def _init_null_metrics(self):
        """Wire up no-op metrics when prometheus_client is unavailable"""
        names = [
            'requests_total', 'request_latency', 'request_tokens',
            'routing_decisions', 'fallback_total', 'cost_total',
            'engine_healthy', 'engine_vram_used', 'engine_vram_total',
            'engine_latency_p50', 'engine_latency_p99', 'engine_error_rate',
            'budget_usage', 'vram_used', 'vram_available', 'quality_score',
        ]
        for name in names:
            setattr(self, name, _NullMetric())
        self._latency_buckets = defaultdict(list)
        self._request_counts = defaultdict(int)
        self._error_counts = defaultdict(int)

    def record_request(self, model: str, engine: str, latency_ms: float, tokens_in: int, tokens_out: int, status: str = "success"):
        self.requests_total.labels(model=model, engine=engine, status=status).inc()
        self.request_latency.labels(model=model, engine=engine).observe(latency_ms / 1000)
        self.request_tokens.labels(model=model, engine=engine, type="input").observe(tokens_in)
        self.request_tokens.labels(model=model, engine=engine, type="output").observe(tokens_out)
        
        # Track for latency percentiles
        key = f"{model}:{engine}"
        self._latency_buckets[key].append(latency_ms)
        self._request_counts[key] += 1
        if status != "success":
            self._error_counts[key] += 1
    
    def record_routing_decision(self, model: str, engine: str, reason: str):
        self.routing_decisions.labels(model=model, engine=engine, reason=reason).inc()
    
    def record_fallback(self, from_engine: str, to_engine: str, reason: str):
        self.fallback_total.labels(from_engine=from_engine, to_engine=to_engine, reason=reason).inc()
    
    def record_cost(self, model: str, engine: str, agent_id: str, cost_usd: float):
        self.cost_total.labels(model=model, engine=engine, agent_id=agent_id).inc(cost_usd)
    
    def update_engine_health(self, engine: str, health):
        self.engine_healthy.labels(engine=engine).set(1 if health.healthy else 0)
        self.engine_vram_used.labels(engine=engine).set(health.vram_used_gb)
        self.engine_vram_total.labels(engine=engine).set(health.vram_total_gb)
        self.engine_latency_p50.labels(engine=engine).set(health.latency_p50_ms)
        self.engine_latency_p99.labels(engine=engine).set(health.latency_p99_ms)
        self.engine_error_rate.labels(engine=engine).set(health.error_rate)
    
    def update_budget(self, budget_type: str, ratio: float):
        self.budget_usage.labels(budget_type=budget_type).set(ratio)
    
    def update_vram(self, used_gb: float, available_gb: float):
        self.vram_used.set(used_gb)
        self.vram_available.set(available_gb)
    
    def record_quality(self, model: str, benchmark: str, score: float):
        self.quality_score.labels(model=model, benchmark=benchmark).set(score)
    
    def get_summary(self) -> dict:
        return {
            "requests": dict(self._request_counts),
            "errors": dict(self._error_counts),
            "latency_p50": {
                k: sorted(v)[len(v)//2] if v else 0
                for k, v in self._latency_buckets.items()
            },
        }