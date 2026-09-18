"""Unit tests for security utilities and audit log."""

import pytest
from james_core.audit import AuditLog, AuditOutcome, AuditSeverity
from james_core.security import is_secret_key, redact_secrets, redact_secrets_in_text, sanitize_text


def test_redact_secret_keys():
    payload = {
        "username": "alice",
        "password": "hunter2",
        "api_key": "sk-1234567890abcdef",
        "nested": {"client_secret": "s3cr3t", "ok": "visible"},
        "list": [{"token": "abc123xyz", "safe": 1}],
    }
    redacted = redact_secrets(payload)
    assert redacted["username"] == "alice"
    assert redacted["password"] == "[REDACTED]"
    assert redacted["api_key"] == "[REDACTED]"
    assert redacted["nested"]["client_secret"] == "[REDACTED]"
    assert redacted["nested"]["ok"] == "visible"
    assert redacted["list"][0]["token"] == "[REDACTED]"
    assert redacted["list"][0]["safe"] == 1


def test_is_secret_key():
    assert is_secret_key("api_key")
    assert is_secret_key("API-KEY")
    assert is_secret_key("ClientSecret")
    assert is_secret_key("authorization_header")
    assert not is_secret_key("description")
    assert not is_secret_key("goal")


def test_redact_patterns_in_text():
    cases = [
        ("Authorization: Bearer abcdef1234567890", "Authorization: [REDACTED]"),
        ("key: sk-abcdefghijklmnopqrstuvwxyz123456", "key: [REDACTED]"),
        ("token=supersecretvalue123456", "[REDACTED]"),
        ("ghp_abcdefghijklmnopqrstuvwxyz1234567890", "[REDACTED]"),
    ]
    for raw, expected in cases:
        assert redact_secrets_in_text(raw) == expected


def test_redact_plain_text_left_alone():
    assert redact_secrets_in_text("The market size is growing.") == "The market size is growing."


def test_sanitize_text():
    assert sanitize_text("a\x00b\x01c\nnormal") == "abc\nnormal"
    assert len(sanitize_text("x" * 5000, max_length=100)) == 100


@pytest.mark.anyio
async def test_audit_record_and_query(tmp_path):
    audit = AuditLog(str(tmp_path / "audit.db"))
    await audit.initialize()

    # Record with a secret in details -> must be redacted on read
    await audit.record(
        actor="cli",
        action="skill.run",
        resource="market_research",
        outcome=AuditOutcome.SUCCESS,
        details={"password": "hunter2", "api_key": "sk-1234abcd", "skill": "market_research"},
        duration_ms=12.5,
    )
    await audit.record(
        actor="cli",
        action="goal.run",
        resource="some goal",
        outcome=AuditOutcome.FAILURE,
        severity=AuditSeverity.ERROR,
        details={"reason": "timeout"},
    )

    entries = await audit.query(limit=10)
    assert len(entries) == 2
    first = entries[0]  # most recent first
    assert first["action"] == "goal.run"
    assert first["outcome"] == "failure"
    assert first["severity"] == "error"
    assert first["details"] == {"reason": "timeout"}

    second = entries[1]
    assert second["action"] == "skill.run"
    assert second["duration_ms"] == 12.5
    assert second["details"]["skill"] == "market_research"
    # secrets redacted
    assert second["details"]["password"] == "[REDACTED]"
    assert second["details"]["api_key"] == "[REDACTED]"
    assert "hunter2" not in str(second["details"])

    # Filters
    only_fail = await audit.query(limit=10, outcome=AuditOutcome.FAILURE)
    assert len(only_fail) == 1
    only_cli = await audit.query(limit=10, actor="cli")
    assert len(only_cli) == 2

    await audit.close()


@pytest.mark.anyio
async def test_audit_empty_db(tmp_path):
    audit = AuditLog(str(tmp_path / "audit.db"))
    await audit.initialize()
    entries = await audit.query(limit=5)
    assert entries == []
    await audit.close()