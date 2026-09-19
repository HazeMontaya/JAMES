[object Object]

def test_ecosystem_capability_metadata_has_explicit_risk_permissions():
    from james_runtime.integration.nats_capabilities import NatsCapabilityBridge

    registry = __import__("james_runtime.tools.registry", fromlist=["ToolRegistry"]).ToolRegistry()
    registry.register("firecrawl", FakeFirecrawl())
    registry.register("dify", FakeDify())
    registry.register("n8n", FakeN8n())

    bridge = NatsCapabilityBridge(registry)
    assert bridge._capability_info(next(t for t in registry.list_tools() if t.name == "web_search"))["required_permissions"] == ["network.public_web"]
    assert bridge._capability_info(next(t for t in registry.list_tools() if t.name == "dify_agent"))["risk_level"] == "high"
    assert bridge._capability_info(next(t for t in registry.list_tools() if t.name == "n8n_webhook"))["required_permissions"] == ["integration.n8n.execute", "integration.n8n.communicate"]
