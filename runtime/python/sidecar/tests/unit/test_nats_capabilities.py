from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
from james_runtime.tools.builtins import Calculator, GetTime


def test_capability_metadata():
    bridge = NatsCapabilityBridge.__new__(NatsCapabilityBridge)
    assert bridge._category("calculator") == "system"
    assert bridge._category("file_read") == "file"
    assert bridge._category("http_get") == "network"


def test_capability_info_matches_rust_contract():
    bridge = NatsCapabilityBridge.__new__(NatsCapabilityBridge)
    info = bridge._capability_info(Calculator())
    assert info["id"] == "calculator"
    assert info["version"] == "1.0.0"
    assert info["input_schema"]["type"] == "object"
    assert "operation" in info["input_schema"]["properties"]
    assert info["output_schema"] is None


def test_execute_subject_contract():
    bridge = NatsCapabilityBridge.__new__(NatsCapabilityBridge)
    bridge.subject_prefix = "james.bridge"
    assert bridge.execute_subject == "james.bridge.capability.execute.*"


def test_list_subject_contract():
    bridge = NatsCapabilityBridge.__new__(NatsCapabilityBridge)
    bridge.subject_prefix = "james.bridge"
    assert bridge.list_subject == "james.bridge.capability.list"


import json
import pytest
from james_runtime.tools.registry import ToolRegistry


@pytest.mark.asyncio
async def test_execute_request_dispatches_calculator():
    registry = ToolRegistry()
    registry.register(Calculator())
    bridge = NatsCapabilityBridge.__new__(NatsCapabilityBridge)
    bridge.registry = registry

    response = await bridge._execute_request(json.dumps({
        "request_id": "test-1",
        "capability_id": "calculator",
        "caller": "agent-runtime",
        "input": {"operation": "multiply", "a": "25", "b": "4"},
    }).encode("utf-8"))

    assert response["request_id"] == "test-1"
    assert response["success"] is True
    assert response["output"] == "100.0"
    assert response["error"] is None
    assert response["duration_ms"] >= 0


@pytest.mark.asyncio
async def test_execute_request_rejects_unknown_capability():
    registry = ToolRegistry()
    registry.register(Calculator())
    bridge = NatsCapabilityBridge.__new__(NatsCapabilityBridge)
    bridge.registry = registry

    response = await bridge._execute_request(json.dumps({
        "request_id": "test-2",
        "capability_id": "does_not_exist",
        "caller": "agent-runtime",
        "input": {},
    }).encode("utf-8"))

    assert response["request_id"] == "test-2"
    assert response["success"] is False
    assert "capability not found" in response["error"]


@pytest.mark.asyncio
async def test_execute_request_rejects_non_object_input():
    registry = ToolRegistry()
    registry.register(Calculator())
    bridge = NatsCapabilityBridge.__new__(NatsCapabilityBridge)
    bridge.registry = registry

    response = await bridge._execute_request(json.dumps({
        "request_id": "test-3",
        "capability_id": "calculator",
        "caller": "agent-runtime",
        "input": ["not", "an", "object"],
    }).encode("utf-8"))

    assert response["request_id"] == "test-3"
    assert response["success"] is False
    assert "JSON object" in response["error"]
