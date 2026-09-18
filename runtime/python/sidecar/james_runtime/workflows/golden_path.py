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
        goal_id = f"gp-{len(self.visited)}"
        route_request = self.runtime._to_routing_request(
            __import__("james_runtime.core.requests", fromlist=["CompletionRequest"]).CompletionRequest(
                model=model,
                messages=[{"role": "user", "content": input_text}],
                max_tokens=256,
            )
        )
        route_request.agent_id = agent_id
        route_request.goal_id = goal_id
        decision = await self.runtime.route_model(route_request)
        selected_model = decision["model_id"]

        plan = [
            {"step": 1, "action": f"classify task ({task})", "status": "done"},
            {"step": 2, "action": f"route to {selected_model}", "status": "done"},
            {"step": 3, "action": "delegate to agent with tools", "status": "pending"},
            {"step": 4, "action": "verify output", "status": "pending"},
        ]
        self._emit("THINKING", {"plan": plan, "goal_id": goal_id})
        await asyncio.sleep(0.05)
        self._emit("PLANNING", {
            "plan": plan,
            "goal_id": goal_id,
            "routing": decision,
        })
        steps.append(f"planned:{len(plan)}_steps")
        steps.append(f"routed:{selected_model}")

        # EXECUTING
        self._emit("EXECUTING", {
            "agent": agent_id,
            "model": selected_model,
            "requested_model": model,
            "goal_id": goal_id,
        })
        result = await self.agents.run(
            input_text, agent_id=agent_id, model=selected_model, max_steps=max_steps,
            goal_id=goal_id,
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