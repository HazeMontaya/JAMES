"""Security utilities for JAMES - secret redaction and input sanitization."""

import re
from typing import Any

SECRET_KEYWORDS: tuple[str, ...] = (
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "authorization",
    "auth",
    "credential",
    "private_key",
    "client_secret",
    "access_key",
    "session_key",
    "ssh_key",
    "cookie",
)

_SECRET_PATTERNS: tuple[re.Pattern[str], ...] = (
    re.compile(r"\bBearer\s+[A-Za-z0-9_\-\.=]{8,}", re.IGNORECASE),
    re.compile(r"\bsk-[A-Za-z0-9]{16,}", re.IGNORECASE),
    re.compile(r"\b(?:key|token|secret)\s*[=:]\s*[\"']?[A-Za-z0-9_\-\.]{12,}", re.IGNORECASE),
    re.compile(r"\bghp_[A-Za-z0-9]{20,}\b"),
    re.compile(r"\bxox[baprs]-[A-Za-z0-9\-]{20,}\b"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
)


def is_secret_key(key: str) -> bool:
    norm = re.sub(r"[\s\-_]+", "", key).lower()
    return any(kw.replace("_", "").replace("-", "") in norm for kw in SECRET_KEYWORDS)


def redact_secrets(value: Any, mask: str = "[REDACTED]") -> Any:
    """Recursively mask values in mappings/sequences that look like secrets."""
    if isinstance(value, dict):
        return {
            k: (mask if is_secret_key(str(k)) else redact_secrets(v, mask))
            for k, v in value.items()
        }
    if isinstance(value, list):
        return [redact_secrets(item, mask) for item in value]
    if isinstance(value, tuple):
        return tuple(redact_secrets(item, mask) for item in value)
    if isinstance(value, str):
        return redact_secrets_in_text(value, mask)
    return value


def redact_secrets_in_text(text: str, mask: str = "[REDACTED]") -> str:
    redacted: str = text
    for pattern in _SECRET_PATTERNS:
        redacted = pattern.sub(mask, redacted)
    return redacted


def sanitize_text(text: str, max_length: int = 100_000) -> str:
    """Cap text length and strip control characters for safe storage/logging."""
    cleaned = "".join(ch for ch in text if ch in ("\n", "\t") or ch.isprintable())
    return cleaned[:max_length]