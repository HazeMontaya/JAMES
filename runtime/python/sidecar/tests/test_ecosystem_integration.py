from james_runtime.integration import EcosystemRegistry, IntegrationDomain, PROFILES


def test_ecosystem_profiles_cover_requested_platforms():
    keys = {profile.key for profile in PROFILES}
    assert keys == {"livekit", "dify", "firecrawl", "n8n", "crewai"}


def test_registry_requires_explicit_registration():
    registry = EcosystemRegistry()
    assert registry.enabled() == ()
    try:
        registry.get("firecrawl")
    except KeyError as exc:
        assert "not enabled" in str(exc)
    else:
        raise AssertionError("disabled integrations must not be executable")


def test_registry_exposes_capability_matrix():
    registry = EcosystemRegistry()

    class Adapter:
        pass

    registry.register("firecrawl", Adapter())
    assert registry.enabled() == ("firecrawl",)
    assert "search" in registry.capability_matrix()["firecrawl"]
    assert registry.profiles["livekit"].domain is IntegrationDomain.REALTIME_VOICE
