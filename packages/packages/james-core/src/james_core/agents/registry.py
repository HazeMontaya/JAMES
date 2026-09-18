"""Agent Registry for JAMES"""

from ..agents.orchestrator import Agent, AgentStatus, AgentType


class AgentRegistry:
    def __init__(self) -> None:
        self._agents: dict[str, Agent] = {}
        self._type_index: dict[AgentType, list[str]] = {t: [] for t in AgentType}
        self._capability_index: dict[str, list[str]] = {}

    def register(self, agent: Agent) -> None:
        self._agents[agent.id] = agent
        self._type_index[agent.agent_type].append(agent.id)
        for cap in agent.capabilities:
            if cap not in self._capability_index:
                self._capability_index[cap] = []
            self._capability_index[cap].append(agent.id)

    def unregister(self, agent_id: str) -> bool:
        agent = self._agents.get(agent_id)
        if not agent:
            return False

        del self._agents[agent_id]
        self._type_index[agent.agent_type].remove(agent_id)
        for cap in agent.capabilities:
            if cap in self._capability_index:
                self._capability_index[cap].remove(agent_id)
        return True

    def get(self, agent_id: str) -> Agent | None:
        return self._agents.get(agent_id)

    def get_by_type(self, agent_type: AgentType) -> list[Agent]:
        return [self._agents[aid] for aid in self._type_index.get(agent_type, []) if aid in self._agents]

    def get_by_capability(self, capability: str) -> list[Agent]:
        return [self._agents[aid] for aid in self._capability_index.get(capability, []) if aid in self._agents]

    def get_available(self, capability: str | None = None) -> list[Agent]:
        agents = [a for a in self._agents.values() if a.status == AgentStatus.IDLE]
        if capability:
            agents = [a for a in agents if capability in a.capabilities]
        return agents

    def list_all(self) -> list[Agent]:
        return list(self._agents.values())

    def count(self) -> int:
        return len(self._agents)

    def count_by_type(self, agent_type: AgentType) -> int:
        return len(self._type_index.get(agent_type, []))
