from james_runtime.core.runtime import JamesRuntime

def test_persistent_event_redaction():
    payload = {"prompt_preview": "secret prompt", "token": "secret", "duration_ms": 4, "model_id": "m"}
    safe = JamesRuntime._redact_persistent_event("AGENT_THINKING", payload)
    assert "prompt_preview" not in safe
    assert "token" not in safe
    assert safe["prompt_preview_length"] == len("secret prompt")
    assert safe["token_length"] == len("secret")
    assert safe["duration_ms"] == 4