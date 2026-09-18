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
