from james_runtime.integration.bootstrap import configure_ecosystems
from james_runtime.tools.registry import ToolRegistry


def test_bootstrap_is_opt_in():
    registry = ToolRegistry()
    ecosystems = configure_ecosystems(registry, env={})
    assert ecosystems.enabled() == ()
    assert registry.list_tools() == []


def test_bootstrap_registers_only_explicit_providers():
    registry = ToolRegistry()
    ecosystems = configure_ecosystems(
        registry,
        env={
            "JAMES_FIRECRAWL_URL": "https://firecrawl.example",
            "JAMES_FIRECRAWL_API_KEY": "secret",
            "JAMES_DIFY_URL": "https://dify.example",
        },
    )
    assert ecosystems.enabled() == ("dify", "firecrawl")
    assert {"web_search", "web_scrape", "dify_agent"} <= {t.name for t in registry.list_tools()}
    assert "JAMES_FIRECRAWL_API_KEY" not in ecosystems.health_matrix()
