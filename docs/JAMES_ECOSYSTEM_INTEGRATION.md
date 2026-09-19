# JAMES Ecosystem Integration

JAMES uses five mature open-source projects as architectural references. They are not copied wholesale and are not required runtime dependencies.

## Reference systems

| System | JAMES role | Concepts adopted |
|---|---|---|
| LiveKit Agents | realtime interface | realtime sessions, STT/TTS, turn detection, telephony, MCP/tools |
| Dify | agent application platform | visual workflows, agent runs, RAG, model management, observability |
| Firecrawl | web-research substrate | search, scrape, crawl, extract, browser interaction |
| n8n | operations automation | event triggers, workflows, credentials, schedules, human approval |
| CrewAI | multi-agent orchestration | Flows, Crews, delegation, memory, tracing |

## JAMES mapping

```text
User / Event
    -> JAMES Decision
    -> Mission
    -> Workflow / Agent Plan
       -> realtime_voice -> LiveKit adapter
       -> web_research  -> Firecrawl adapter
       -> automation    -> n8n adapter
       -> agent_workflow -> Dify adapter
       -> multi_agent   -> CrewAI adapter
    -> Capability Resolver
    -> Policy / Capability Broker
    -> Executor
    -> Observation -> Evaluation -> Memory
```

Adapters do not become an alternate authorization path.

## Implemented contract layer

`james_runtime.integration.ecosystem` now provides `IntegrationProfile`, `IntegrationDomain`, `EcosystemRegistry`, provider-neutral adapter protocols, explicit registration, enabled-integration discovery, and a capability matrix.

Installing or importing a provider does not enable it. An adapter must be explicitly registered before JAMES can call it.

## Implementation targets

### Firecrawl
First web-research adapter target: `web.search`, `web.scrape`, `web.crawl`, `web.extract`, provenance, and URL/domain policy enforcement.

### LiveKit
Realtime voice gateway: inbound/outbound voice, interruption handling, session lifecycle, telephony, and realtime tool calls.

### n8n
Operations bridge: webhooks, workflow execution, schedules, credential references, and approval-required execution.

### Dify
Application interoperability: invoke workflows/agent runs, exchange structured state, and correlate external runs with JAMES missions.

### CrewAI
Multi-agent substrate: launch bounded Crews/Flows, delegate subtasks, collect structured results, and expose tracing/evaluation metadata.

## Non-negotiable boundary

External systems may provide execution mechanisms. They do not own JAMES identity, capability authorization, security policy, promotion policy, canonical memory, SelfMade promotion, or audit integrity.

The canonical lifecycle remains:

```text
event -> decision -> mission -> plan -> capability resolution
      -> authorization -> execution -> observation -> evaluation
      -> memory -> next decision
```

## Sources

- LiveKit Agents: https://github.com/livekit/agents
- Dify: https://github.com/langgenius/dify
- Firecrawl: https://github.com/firecrawl/firecrawl
- n8n: https://github.com/n8n-io/n8n
- CrewAI: https://github.com/crewAIInc/crewAI

## Operator capability allowlist

Discovery does not grant execution rights. The Rust capability broker remains the authorization boundary. To grant capabilities to the application user explicitly, set `JAMES_USER_CAPABILITIES` to a comma-separated allowlist of capability IDs, for example `web_search,web_scrape`. Capabilities that are not listed remain denied even when their Python provider is connected. High-risk integrations such as `dify_agent` and `n8n_webhook` therefore require deliberate operator configuration.


## Python bridge execution boundary

Capability execution over NATS is authenticated independently of the caller identity
inside the capability request. Configure the same high-entropy secret in both processes:

- Rust bridge: `JAMES_BRIDGE_TOKEN`
- Python sidecar: `JAMES_BRIDGE_TOKEN`

If the token is absent, the Python execution endpoint refuses all capability execution.
The token is transport authentication only; it does **not** grant a caller capability
permissions. The Rust capability broker remains responsible for capability existence,
schema validation, permissions, policy, confirmation, execution, output verification and audit.

Registration, listing and health subjects remain separate from the execution subject and
do not themselves authorize execution.
