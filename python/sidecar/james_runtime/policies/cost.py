"""JAMES Runtime Cost Tracking and Budget Enforcement"""
import logging
import sqlite3
import time
from decimal import Decimal
from dataclasses import dataclass
from typing import Optional, List
from pathlib import Path

logger = logging.getLogger(__name__)


@dataclass
class UsageRecord:
    timestamp: float
    model: str
    tokens_in: int
    tokens_out: int
    cost_usd: Decimal
    engine: str
    agent_id: str
    goal_id: str
    request_id: str


@dataclass
class BudgetPolicy:
    daily_ai_budget_usd: Decimal = Decimal('0.0')
    monthly_ai_budget_usd: Decimal = Decimal('0.0')
    max_single_request_usd: Decimal = Decimal('0.01')
    approved_models: List[str] = None
    approved_providers: List[str] = None
    
    def __post_init__(self):
        if self.approved_models is None:
            self.approved_models = []
        if self.approved_providers is None:
            self.approved_providers = ["local"]


@dataclass
class BudgetCheck:
    allowed: bool
    reason: Optional[str] = None
    remaining_daily: Decimal = Decimal('0')
    remaining_monthly: Decimal = Decimal('0')
    estimated_cost: Decimal = Decimal('0')


class CostTracker:
    """Tracks AI usage costs per model, agent, goal"""
    
    def __init__(self, db_path: str):
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_schema()
    
    def _init_schema(self) -> None:
        with sqlite3.connect(self.db_path) as conn:
            conn.execute("""
                CREATE TABLE IF NOT EXISTS usage (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp REAL NOT NULL,
                    model TEXT NOT NULL,
                    tokens_in INTEGER NOT NULL,
                    tokens_out INTEGER NOT NULL,
                    cost_usd TEXT NOT NULL,
                    engine TEXT NOT NULL,
                    agent_id TEXT,
                    goal_id TEXT,
                    request_id TEXT,
                    created_at REAL DEFAULT (strftime('%s', 'now'))
                )
            """)
            conn.execute("""
                CREATE INDEX IF NOT EXISTS idx_usage_model ON usage(model)
            """)
            conn.execute("""
                CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON usage(timestamp)
            """)
            conn.execute("""
                CREATE INDEX IF NOT EXISTS idx_usage_agent ON usage(agent_id)
            """)
            conn.execute("""
                CREATE INDEX IF NOT EXISTS idx_usage_goal ON usage(goal_id)
            """)
    
    def record_usage(
        self,
        model: str,
        tokens_in: int,
        tokens_out: int,
        cost_usd: Decimal,
        engine: str,
        agent_id: str = "",
        goal_id: str = "",
        request_id: str = "",
    ) -> Decimal:
        """Record a usage event and return the cost"""
        from decimal import Decimal
        cost_usd = Decimal(str(cost_usd))
        
        with sqlite3.connect(self.db_path) as conn:
            conn.execute("""
                INSERT INTO usage (timestamp, model, tokens_in, tokens_out, cost_usd, engine, agent_id, goal_id, request_id)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (
                time.time(),
                model,
                tokens_in,
                tokens_out,
                str(cost_usd),
                engine,
                agent_id or "",
                goal_id or "",
                ""  # request_id
            ))
            conn.commit()
        
        return cost_usd
    
    def daily_spent(self) -> Decimal:
        from decimal import Decimal
        day_start = time.time() - (time.time() % 86400)
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.execute(
                "SELECT SUM(cost_usd) FROM usage WHERE timestamp >= ?",
                (day_start,)
            )
            result = cursor.fetchone()[0]
            return Decimal(result) if result else Decimal('0')
    
    def monthly_spent(self) -> Decimal:
        from decimal import Decimal
        month_start = time.time() - (time.time() % 2592000)
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.execute(
                "SELECT SUM(cost_usd) FROM usage WHERE timestamp >= ?",
                (month_start,)
            )
            result = cursor.fetchone()[0]
            return Decimal(result) if result else Decimal('0')
    
    def total_spent(self) -> Decimal:
        from decimal import Decimal
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.execute("SELECT SUM(cost_usd) FROM usage")
            result = cursor.fetchone()[0]
            return Decimal(result) if result else Decimal('0')
    
    def spent_by(self, column: str) -> dict:
        from decimal import Decimal
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.execute(
                f"SELECT {column}, SUM(cost_usd) FROM usage GROUP BY {column}"
            )
            return {row[0]: Decimal(row[1] or 0) for row in cursor.fetchall()}
    
    def get_model_costs(self, model: str) -> dict:
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.execute(
                "SELECT SUM(tokens_in), SUM(tokens_out), SUM(cost_usd), COUNT(*) FROM usage WHERE model = ?",
                (model,)
            )
            row = cursor.fetchone()
            return {
                "model": model,
                "total_tokens_in": row[0] or 0,
                "total_tokens_out": row[1] or 0,
                "total_cost_usd": float(row[2] or 0),
                "request_count": row[3] or 0,
            }
    
    def get_agent_costs(self, agent_id: str) -> dict:
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.execute(
                "SELECT SUM(tokens_in), SUM(tokens_out), SUM(cost_usd), COUNT(*) FROM usage WHERE agent_id = ?",
                (agent_id,)
            )
            row = cursor.fetchone()
            return {
                "agent_id": agent_id,
                "total_tokens_in": row[0] or 0,
                "total_tokens_out": row[1] or 0,
                "total_cost_usd": float(row[2] or 0),
                "request_count": row[3] or 0,
            }
    
    def get_goal_costs(self, goal_id: str) -> dict:
        with sqlite3.connect(self.db_path) as conn:
            cursor = conn.execute(
                "SELECT SUM(tokens_in), SUM(tokens_out), SUM(cost_usd), COUNT(*) FROM usage WHERE goal_id = ?",
                (goal_id,)
            )
            row = cursor.fetchone()
            return {
                "goal_id": goal_id,
                "total_tokens_in": row[0] or 0,
                "total_tokens_out": row[1] or 0,
                "total_cost_usd": float(row[2] or 0),
                "request_count": row[3] or 0,
            }
    
    def get_recent_usage(self, hours: int = 24) -> List[dict]:
        cutoff = time.time() - (hours * 3600)
        with sqlite3.connect(self.db_path) as conn:
            conn.row_factory = sqlite3.Row
            cursor = conn.execute(
                "SELECT * FROM usage WHERE timestamp >= ? ORDER BY timestamp DESC",
                (time.time() - hours * 3600,)
            )
            return [dict(row) for row in cursor.fetchall()]


class BudgetEnforcer:
    """Enforces budget policies on AI usage"""
    
    def __init__(self, policy, tracker: CostTracker):
        self.policy = policy
        self.tracker = tracker
    
    def check(self, request) -> 'BudgetCheck':
        from decimal import Decimal
        from james_runtime.policies.cost import BudgetCheck

        # Normalize policy amounts to Decimal (pydantic gives floats)
        daily_limit = Decimal(str(self.policy.daily_ai_budget_usd))
        monthly_limit = Decimal(str(self.policy.monthly_ai_budget_usd))
        max_single = Decimal(str(self.policy.max_single_request_usd))

        # Check daily budget
        daily_spent = self.tracker.daily_spent()
        if daily_limit > 0 and daily_spent >= daily_limit:
            return BudgetCheck(
                allowed=False,
                reason=f"Daily budget exceeded: ${daily_spent} >= ${daily_limit}",
                remaining_daily=Decimal('0'),
                remaining_monthly=monthly_limit - self.tracker.monthly_spent(),
            )

        # Check monthly budget
        monthly_spent = self.tracker.monthly_spent()
        if monthly_limit > 0 and monthly_spent >= monthly_limit:
            return BudgetCheck(
                allowed=False,
                reason=f"Monthly budget exceeded: ${monthly_spent} >= ${monthly_limit}",
                remaining_daily=daily_limit - self.tracker.daily_spent(),
                remaining_monthly=Decimal('0'),
            )

        # Check single request limit
        estimated = self._estimate_cost(request)
        if estimated > max_single:
            return BudgetCheck(
                allowed=False,
                reason=f"Single request cost ${estimated} exceeds limit ${max_single}",
                remaining_daily=daily_limit - self.tracker.daily_spent(),
                remaining_monthly=monthly_limit - self.tracker.monthly_spent(),
                estimated_cost=estimated,
            )

        # Check model approval
        requested_model = getattr(request, 'preferred_model', None) or getattr(request, 'model', None)
        if self.policy.approved_models and requested_model:
            if requested_model not in self.policy.approved_models:
                return BudgetCheck(
                    allowed=False,
                    reason=f"Model {requested_model} not in approved list",
                    remaining_daily=daily_limit - self.tracker.daily_spent(),
                    remaining_monthly=monthly_limit - self.tracker.monthly_spent(),
                )

        return BudgetCheck(
            allowed=True,
            remaining_daily=daily_limit - daily_spent,
            remaining_monthly=monthly_limit - monthly_spent,
            estimated_cost=estimated,
        )
    
    def _estimate_cost(self, request) -> 'Decimal':
        from decimal import Decimal
        # Simplified estimation
        tokens_in = getattr(request, 'estimated_tokens', None) or 100  # placeholder
        tokens_out = getattr(request, 'max_tokens', None) or 512
        
        # Would use pricing registry in real implementation
        return Decimal('0.001')  # placeholder