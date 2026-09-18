"""Golden Path Workflow: Wake -> Listening -> Understanding -> Thinking
   -> Planning -> Executing -> Verifying -> Success -> Idle"""
import asyncio
import logging
from abc import ABC
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

logger = logging.getLogger(__name__)


@dataclass
class WorkflowResult:
    """Result of a workflow run"""
    state: str
    answer: str = ""
    steps: List[str] = field(default_factory=list)
    data: Dict[str, Any] = field(default_factory=dict)
    success: bool = False

    def to_dict(self) -> Dict[str, Any]:
        return {
            "state": self.state,
            "answer": self.answer,
            "steps": self.steps,
            "data": self.data,
            "success": self.success,
        }


class Workflow(ABC):
    """Base workflow with a state machine and event emission."""

    name = "base"
    states: List[str] = []

    def __init__(self, runtime):
        self.runtime = runtime
        self.state = ""
        self.visited: List[str] = []

    def _emit(self, state: str, extra: Optional[Dict[str, Any]] = None):
        self.state = state
        self.visited.append(state)
        self.runtime._emit_event(f"GOLDEN_PATH_{state.upper()}", {
            "workflow": self.name,
            "state": state,
            **(extra or {}),
        })
        logger.info("GP state -> %s", state)

    async def run(self, *args, **kwargs) -> WorkflowResult:
        raise NotImplementedError


class GoldenPathWorkflow(Workflow):
    """Runs the canonical JAMES perception->action->success loop."""

    name = "golden-path"
    states = ["WAKE", "LISTENING", "UNDERSTANDING", "THINKING",
              "PLANNING", "EXECUTING", "VERIFYING", "SUCCESS", "IDLE"]

    def __init__(self, runtime, agent_system):
        super().__init__(runtime)
        self.agents = agent_system

    async def run(self, input_text: str, agent_id: str = "default",
                  model: str = "llama-3.1-8b-instruct", max_steps: int = 6) -> WorkflowResult:
        """Execute the golden path for a user input."""
        steps: List[str] = []

        # WAKE
        self._emit("WAKE")
        await asyncio.sleep(0.05)

        # LISTENING
        self._emit("LISTENING", {"input": input_text[:200]})
        steps.append("listening")

        # UNDERSTANDING
        task = self.runtime._classify_task(input_text)
        self._emit("UNDERSTANDING", {"task": task})
        steps.append(f"understood:{task}")

        # THINKING / PLANNING
        plan = [
            {"step": 1, "action": f"classify task ({task})", "status": "done"},
            {"step": 2, "action": "delegate to agent with tools", "status": "pending"},
            {"step": 3, "action": "verify output", "status": "pending"},
        ]
        self._emit("THINKING", {"plan": plan})
        await asyncio.sleep(0.05)
        self._emit("PLANNING", {"plan": plan})
        steps.append(f"planned:{len(plan)}_steps")

        # EXECUTING
        self._emit("EXECUTING", {"agent": agent_id, "model": model})
        result = await self.agents.run(
            input_text, agent_id=agent_id, model=model, max_steps=max_steps,
            goal_id=f"gp-{len(self.visited)}",
        )
        steps.append(f"executed:{result.tool_calls}_tools")

        # VERIFYING
        answer = result.answer.strip()
        terminal_answer = bool(answer) and any(
            step.step_type == "answer" for step in result.steps
        )
        valid = bool(result.success and terminal_answer and answer)
        self._emit("VERIFYING", {
            "valid": valid,
            "answer_length": len(answer),
            "agent_success": result.success,
            "terminal_answer": terminal_answer,
            "tool_calls": result.tool_calls,
            "agent_message": result.message,
        })
        steps.append(f"verified:{valid}")

        # SUCCESS / IDLE
        if valid:
            self._emit("SUCCESS", {"answer": result.answer[:200]})
            final_state = "SUCCESS"
            success = True
        else:
            self._emit("IDLE", {"reason": "no_answer"})
            final_state = "IDLE"
            success = False

        return WorkflowResult(
            state=final_state,
            answer=result.answer,
            steps=steps,
            data={"task": task, "tool_calls": result.tool_calls, "model": result.model,
                  "agent_steps": [vars(s) for s in result.steps]},
            success=success,
        )