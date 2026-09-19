from pathlib import Path

from james_runtime.core.events import RuntimeEvent
from james_runtime.financial.events import FinancialEvent
from james_runtime.memory.vault import MemorySyncSnapshot, MemoryVault


def test_financial_events_use_canonical_runtime_event_envelope():
    event = FinancialEvent(event_type="MARKET_TICK", payload={"symbol": "TEST"})
    assert isinstance(event, RuntimeEvent)
    assert event.source == "financial.cortex"
    assert event.correlation_id is None


def test_memory_vault_is_explicitly_sidecar_projection(tmp_path: Path):
    vault = MemoryVault(tmp_path / "memory")
    assert vault.authority == "sidecar_projection"
    assert vault.canonical_authority == "rust_memory_module"


def test_tool_registry_has_one_authoritative_execute_definition():
    from james_runtime.tools.registry import ToolRegistry

    execute = ToolRegistry.execute
    assert execute.__module__ == "james_runtime.tools.registry"
    assert "broker_authorized" in execute.__code__.co_varnames


def test_memory_sync_contract_projects_only_typed_canonical_snapshot(tmp_path: Path):
    vault = MemoryVault(tmp_path / "memory")
    count = vault.project_canonical_snapshot(MemorySyncSnapshot.model_validate({
        "schema_version": 1,
        "authority": "rust_memory_module",
        "entries": [{
            "id": "m1",
            "memory_type": "Semantic",
            "content": "canonical fact",
            "metadata": {"title": "Canonical fact"},
            "importance": 0.9,
            "tags": ["canonical"],
            "updated_at": "2026-09-19T00:00:00Z",
        }],
    }))
    assert count == 1
    assert vault.read_note("rust-m1").find("canonical fact") >= 0
