"""JAMES ReAct Reasoning Loop (Think -> Act -> Observe)"""
import json
import logging
import re
from dataclasses import dataclass, field
from typing import Any, Awaitable, Callable, Dict, List, Optional

from james_runtime.core.requests import CompletionRequest
from james_runtime.steering.autotune import compute_autotune
from james_runtime.tools.registry import ToolRegistry

logger = logging.getLogger(__name__)


@dataclass
class AgentStep:
    """One step in the agent loop"""
    step_type: str  # "thought" | "action" | "observation" | "answer"
    content: str
    tool: str = ""
    tool_args: Dict[str, Any] = field(default_factory=dict)


@dataclass
class AgentResult:
    """Final result of an agent run"""
    answer: str
    steps: List[AgentStep] = field(default_factory=list)
    tool_calls: int = 0
    success: bool = False
    message: str = ""
    model: str = ""

    def to_dict(self) -> Dict[str, Any]:
        return {
            "answer": self.answer,
            "steps": [vars(s) for s in self.steps],
            "tool_calls": self.tool_calls,
            "success": self.success,
            "message": self.message,
            "model": self.model,
        }


SYSTEM_PROMPT_TEMPLATE = """You are JAMES, an autonomous AI assistant running on the user's own machine.
You think step by step and use tools to answer or act. Be precise and concise.

Available tools:
{tools}

Format your responses EXACTLY as follows, one per line:

Thought: <your reasoning>
Action: <tool_name>
Action Input: <JSON arguments for the tool>

After the tool result is fed back to you, continue with another Thought/Action, or finish with:

Final Answer: <the complete answer to the user>

Rules:
- Use a tool whenever you need facts, computation, files or actions.
- For ANY calculation, ALWAYS use math_eval or python_exec first. Never compute large or multi-step math in your head.
- Action Input must be valid JSON with only the tool's declared parameters.
- When you have enough information, always end with "Final Answer:".

Example of correct behavior:
User: What is 152 * 37?
Thought: I need to multiply 152 by 37. I will use math_eval.
Action: math_eval
Action Input: {{"expression": "152*37"}}

[Tool result: 5624]

Thought: The calculation gives 5624.
Final Answer: 152 * 37 = 5624.
"""


class ReActReasoner:
    """ReAct loop driving a model with tools."""

    def __init__(
        self,
        runtime,
        tool_registry: ToolRegistry,
        model: str = "llama-3.1-8b-instruct",
        max_steps: int = 6,
        system_prompt: str = "",
        agent_id: str = "default",
        goal_id: str = "",
        tool_executor: Callable[..., Awaitable[Dict[str, Any]]] | None = None,
    ):
        self.runtime = runtime
        self.tools = tool_registry
        self.model = model
        self.max_steps = max_steps
        self.system_prompt = system_prompt
        self.agent_id = agent_id
        self.goal_id = goal_id
        self.tool_executor = tool_executor

    def _tool_docs(self) -> str:
        lines = []
        for t in self.tools.list_tools():
            params = ", ".join(f"{p}: {spec.get('type', 'string')}" for p, spec in t.parameters.items())
            lines.append(f"- {t.name} ({params}): {t.description}")
        return "\n".join(lines)

    def _system(self) -> str:
        base = self.system_prompt or SYSTEM_PROMPT_TEMPLATE.format(tools=self._tool_docs())
        return base

    def _parse(self, text: str) -> AgentStep:
        """Parse the model output into a thought/action or answer step."""
        # Final answer first
        final_match = re.search(r"Final Answer:\s*(.+)", text, re.DOTALL)
        if final_match:
            return AgentStep(step_type="answer", content=final_match.group(1).strip())

        action_match = re.search(r"Action:\s*(\w+)", text)
        input_match = re.search(r"Action Input:\s*(\{.*\}|\[.*\])", text, re.DOTALL)
        thought_match = re.search(r"Thought:\s*(.+)", text, re.DOTALL)

        if action_match:
            tool = action_match.group(1)
            args: Dict[str, Any] = {}
            if input_match:
                raw = input_match.group(1).strip()
                try:
                    args = json.loads(raw) if raw else {}
                except json.JSONDecodeError:
                    args = {"raw": raw}
            content = f"Call {tool}"
            if thought_match:
                content = thought_match.group(1).strip() + " -> " + content
            return AgentStep(step_type="action", content=content, tool=tool, tool_args=args)

        # No action, no final -> treat as answer if it looks final-ish, else as continuation
        stripped = text.strip()
        if stripped:
            return AgentStep(step_type="answer", content=stripped[-2000:])
        return AgentStep(step_type="thought", content="(empty response)")

    async def run(self, query: str, conversation_history: Optional[List[Dict]] = None) -> AgentResult:
        """Run the ReAct loop for a query."""
        history = list(conversation_history or [])
        messages = self._build_messages(query, history)
        steps: List[AgentStep] = []
        tool_calls = 0
        last_text = ""

        for step_index in range(self.max_steps):
            self.runtime._emit_event("AGENT_THINKING", {
                "agent_id": self.agent_id,
                "goal_id": self.goal_id,
                "step": step_index,
                "prompt_preview": messages[-1]["content"][-400:] if messages else "",
            })

            tune = compute_autotune(query, history=[m.get("content", "") for m in messages[-4:]])
            response = await self.runtime.generate(CompletionRequest(
                model=self.model,
                messages=messages,
                temperature=tune.profile.temperature,
                top_p=tune.profile.top_p,
                frequency_penalty=tune.profile.frequency_penalty,
                presence_penalty=tune.profile.presence_penalty,
                max_tokens=256,
                stream=False,
            ))
            last_text = response.choices[0]["message"]["content"]

            if self.runtime:
                self.runtime._emit_event("AGENT_ACTION", {
                    "agent_id": self.agent_id,
                    "goal_id": self.goal_id,
                    "step": step_index,
                })

            step = self._parse(last_text)

            if step.step_type == "answer":
                steps.append(step)
                self.runtime._emit_event("AGENT_DONE", {
                    "agent_id": self.agent_id,
                    "goal_id": self.goal_id,
                    "steps": step_index + 1,
                    "tool_calls": tool_calls,
                })
                return AgentResult(
                    answer=step.content,
                    steps=steps,
                    tool_calls=tool_calls,
                    success=True,
                    model=self.model,
                )

            if step.step_type == "action":
                steps.append(step)
                tool_calls += 1
                self.runtime._emit_event("AGENT_TOOL_CALL", {
                    "agent_id": self.agent_id,
                    "goal_id": self.goal_id,
                    "tool": step.tool,
                    "args": json.dumps(step.tool_args, ensure_ascii=False)[:300],
                })
                if self.tool_executor is not None:
                    result = await self.tool_executor(step.tool, step.tool_args, caller=f"agent:{self.agent_id}")
                else:
                    result = await self.tools.execute(step.tool, step.tool_args)
                self.runtime._emit_event("AGENT_TOOL_RESULT", {
                    "agent_id": self.agent_id,
                    "goal_id": self.goal_id,
                    "tool": step.tool,
                })

                observation = result["output"][:1500] if result["success"] else f"ERROR: {result['error']}"
                messages.append({"role": "assistant", "content": last_text})
                messages.append({"role": "system", "content": f"Tool result (success={result['success']}):\n{observation or '(empty)'}"})
                steps.append(AgentStep(step_type="observation", content=observation, tool=step.tool))
                continue

            # Should not happen (thought-only output); force continuation
            messages.append({"role": "assistant", "content": last_text})
            messages.append({"role": "system", "content": "Please provide either an Action or a Final Answer."})

        self.runtime._emit_event("AGENT_EXHAUSTED", {
            "agent_id": self.agent_id,
            "goal_id": self.goal_id,
            "steps": len(steps),
            "tool_calls": tool_calls,
        })
        return AgentResult(
            answer=last_text,
            steps=steps,
            tool_calls=tool_calls,
            success=False,
            message="Max steps reached",
            model=self.model,
        )

    def _build_messages(self, query: str, history: List[Dict]) -> List[Dict]:
        messages = [{"role": "system", "content": self._system()}]
        for msg in history[-8:]:
            messages.append({"role": msg.get("role", "user"), "content": msg.get("content", "")})
        messages.append({"role": "user", "content": query})
        return messages