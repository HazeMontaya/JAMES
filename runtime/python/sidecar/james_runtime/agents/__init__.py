"""JAMES Agent System - wires runtime, tools, memory and reasoners"""
import logging
from pathlib import Path
from typing import Any, Dict, List, Optional

from james_runtime.agents.memory import MemoryStore, MemoryTools
from james_runtime.agents.reasoner import AgentResult, ReActReasoner
from james_runtime.memory.bootstrap import bootstrap
from james_runtime.memory.resolver import ContextResolver
from james_runtime.memory.sessions import SessionRecorder
from james_runtime.memory.skills import SkillStore
from james_runtime.tools.builtins import default_tools
from james_runtime.tools.registry import ToolRegistry

logger = logging.getLogger(__name__)


class AgentSystem:
    """Manager for agents, tools and memory across the runtime."""

    def __init__(self, runtime, base_dir: Optional[Path] = None):
        self.runtime = runtime
        self.base_dir = base_dir or Path(".james")
        self.tool_registry = ToolRegistry()

        # Memory
        self.memory = MemoryStore(self.base_dir / "memory" / "memory.jsonl")
        self.vault = bootstrap(self.base_dir / "vault")
        self.skills = SkillStore(self.base_dir / "vault" / "skills")
        self.context_resolver = ContextResolver(self.vault, self.skills)
        self.sessions = SessionRecorder(self.base_dir / "vault")

        # Register built-in + memory tools
        self.tool_registry.register_many(default_tools())
        MemoryTools(self.memory).register(self.tool_registry)

        self._agents: Dict[str, ReActReasoner] = {}

    def get_reasoner(self, agent_id: str = "default", model: str = "llama-3.1-8b-instruct", goal_id: str = "") -> ReActReasoner:
        key = f"{agent_id}:{model}:{goal_id}"
        if key not in self._agents:
            agent = ReActReasoner(
                runtime=self.runtime,
                tool_registry=self.tool_registry,
                model=model,
                agent_id=agent_id,
                goal_id=goal_id,
            )
            self._agents[key] = agent
        return self._agents[key]

    async def run(self, query: str, agent_id: str = "default", model: str = "llama-3.1-8b-instruct",
                  max_steps: int = 6, goal_id: str = "") -> AgentResult:
        """Run an agent with persistent memory context and automatic interaction capture."""
        agent = self.get_reasoner(agent_id, model, goal_id)
        agent.max_steps = max_steps

        # Inject only a small, relevant memory window so long-running sessions
        # do not grow the model context without bound.
        memories = self.memory.recall(query=query, namespace=agent_id, limit=5)
        resolved = self.context_resolver.resolve(query, skill_limit=3, memory_limit=5)
        durable_context = []
        for match in resolved:
            if match.kind == "memory":
                try:
                    durable_context.append(self.vault.read_note(match.name)[:2500])
                except KeyError:
                    continue
            else:
                try:
                    skill = self.skills.load(match.name)
                    durable_context.append(
                        f"Skill: {skill.name}\nDescription: {skill.description}\n"
                        f"Procedure: {'; '.join(skill.procedure)}\nLessons: {'; '.join(skill.lessons[-5:])}"
                    )
                except KeyError:
                    continue
        history = []
        if memories or durable_context:
            history.append({
                "role": "system",
                "content": "Relevant persistent context:\n"
                + "\n\n".join([*(f"- {m['content']}" for m in reversed(memories)), *durable_context]),
            })
        session_id = __import__("uuid").uuid4().hex
        self.sessions.start(query, session_id)
        self.sessions.record(
            action=f"Context primed: {len(resolved)} matches",
            decision="; ".join(f"{m.kind}:{m.name} ({m.score:.2f})" for m in resolved[:5]) or "No durable context match",
        )
        try:
            result = await agent.run(query, conversation_history=history)
        except Exception as exc:
            self.sessions.record(error=str(exc))
            self.sessions.finish("failed")
            raise
        if result.success:
            # Persist a compact interaction record. The full ReAct trace stays
            # in the returned result rather than being duplicated in memory.
            answer = result.answer.strip()
            if answer:
                self.memory.remember(
                    f"User: {query[:1000]}\\nJAMES: {answer[:2000]}",
                    namespace=agent_id,
                    key=goal_id or None,
                )

        self.sessions.record(action=f"Agent completed: success={result.success}, tool_calls={result.tool_calls}")
        if result.success:
            self.sessions.record(lesson="Persist only verified successful outcomes; retain failures for diagnosis.")
            self.sessions.finish("success")
        else:
            self.sessions.record(error=result.message or "agent run did not complete")
            self.sessions.finish("incomplete")
        return result

    def status(self) -> Dict[str, Any]:
        return {
            "agents": list(self._agents.keys()),
            "active_goals": sorted({agent.goal_id for agent in self._agents.values() if agent.goal_id}),
            "tools": [t.name for t in self.tool_registry.list_tools()],
            "memories": self.memory.count(),
        }