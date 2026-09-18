"""Memory CRUD integration tests"""


import anyio
import pytest
from james_memory import (
    BusinessMemory,
    EpisodicMemory,
    MemoryRouter,
    ProceduralMemory,
    SemanticMemory,
    UserMemory,
    VaultManager,
)
from james_memory.models import MemoryEntry, MemoryType


@pytest.fixture
def memory_home(tmp_path):
    return str(tmp_path / "jameshome")


def test_vault_manager_creates_structure(tmp_path):
    vault = VaultManager(str(tmp_path / "vault"))

    async def run():
        await vault.initialize()
        files = vault.list_files()
        assert len(files) >= 4
        profile = await vault.read_file(vault.root_path / "profile" / "user.md")
        assert "User Profile" in profile
        daily = await vault.get_daily_note()
        assert "Daily Note" in daily

    anyio.run(run)


def test_episodic_memory_crud(tmp_path):
    memory = EpisodicMemory(str(tmp_path / "episodic.db"))

    async def run():
        await memory.initialize()
        entry = MemoryEntry(
            memory_type=MemoryType.EPISODIC,
            content="Test event happened",
            tags=["test", "event"],
            metadata={"source": "test"},
        )
        entry_id = await memory.store(entry)
        assert entry_id is not None

        fetched = await memory.retrieve(entry_id)
        assert fetched is not None
        assert fetched.content == "Test event happened"
        assert fetched.access_count == 1

        deleted = await memory.delete(entry_id)
        assert deleted is True
        await memory.close()

    anyio.run(run)


def test_router_stores_and_retrieves(tmp_path):
    base = str(tmp_path / "james")
    episodic = EpisodicMemory(f"{base}/memory/episodic.db")
    semantic = SemanticMemory(f"{base}/vault")
    procedural = ProceduralMemory(f"{base}/memory/procedural.db")
    business = BusinessMemory(f"{base}/memory/business.db")
    user = UserMemory(f"{base}/vault")

    router = MemoryRouter(episodic, semantic, procedural, business, user)

    async def run():
        await episodic.initialize()
        await semantic.initialize()
        await procedural.initialize()
        await business.initialize()
        await user.initialize()

        await router.store("Episodic memory test", MemoryType.EPISODIC, tags=["test"], source="test")

        from james_memory.models import RetrievalQuery
        results = await router.retrieve(RetrievalQuery(query="test", memory_types=[MemoryType.EPISODIC]))
        assert results.total_found >= 1

        await user.set_preference("language", "de")
        lang = await user.get_preference("language")
        assert lang == "de"

        skill_data = {
            "name": "test_skill",
            "category": "test",
            "description": "A test skill",
            "steps": [{"id": "s1"}],
        }
        await procedural.store_skill(skill_data)
        loaded = await procedural.get_skill("test_skill")
        assert loaded is not None
        assert loaded["description"] == "A test skill"

        await router.close()

    anyio.run(run)


def test_credential_vault(tmp_path):
    from james_world import CredentialVault

    vault = CredentialVault(str(tmp_path / "creds" / "vault.key"))
    vault.initialize()
    vault.set("github", token="secret-token-123")
    assert vault.has("github")
    creds = vault.get("github")
    assert creds["token"] == "secret-token-123"
    assert vault.delete("github") is True
    assert vault.has("github") is False


def test_world_access_health(tmp_path):
    from james_world import WorldAccess

    world = WorldAccess()

    async def run():
        await world.on_start()
        summary = world.capability_summary()
        assert "web_search" in summary
        assert "web_fetch" in summary

    anyio.run(run)
