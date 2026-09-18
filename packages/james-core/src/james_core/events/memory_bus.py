"""In-memory Event Bus for JAMES - offline fallback with NATS-style subject matching."""

import asyncio
import fnmatch
import json
from collections.abc import Awaitable, Callable
from dataclasses import dataclass

import structlog

from .models import Event

logger = structlog.get_logger()


def _matches(subject: str, pattern: str) -> bool:
    """NATS-style subject match: `*` matches one token, `>` matches one or more trailing tokens."""
    if pattern.endswith(">"):
        prefix = pattern[:-1]  # includes trailing dot
        return subject.startswith(prefix)
    return fnmatch.fnmatchcase(subject, pattern)


@dataclass
class InMemorySubscription:
    subject: str
    callback: Callable[[Event], Awaitable[None]]


class InMemoryEventBus:
    """Async pub/sub bus that mimics the NATS EventBus API without a server.

    Supports subject wildcards (`a.*`, `a.>`) and is safe to use in tests and
    fully-offline deployments. Payloads are handed to callbacks directly (not
    serialized), but `to_dict`/`from_dict` round-trips are still validated for
    subscribers during publish to catch serialization regressions.
    """

    def __init__(self) -> None:
        self._subscriptions: list[InMemorySubscription] = []
        self._connected = False
        self._lock = asyncio.Lock()
        self.published: list[tuple[str, Event]] = []

    async def connect(self) -> None:
        async with self._lock:
            self._connected = True

    async def publish(self, subject: str, event: Event) -> None:
        if not self._connected:
            await self.connect()
        self.published.append((subject, event))

        # Serialization round-trip guard: subscribers get the reconstructed event
        data = json.dumps(event.to_dict())
        revived = Event.from_dict(json.loads(data))

        for sub in list(self._subscriptions):
            if _matches(subject, sub.subject):
                try:
                    await sub.callback(revived)
                except Exception as e:  # pragma: no cover - defensive
                    logger.error("In-memory event callback failed", subject=subject, error=str(e))

    async def subscribe(
        self,
        subject: str,
        callback: Callable[[Event], Awaitable[None]],
        durable: str | None = None,  # noqa: ARG001 - kept for API parity with NATS
    ) -> None:
        if not self._connected:
            await self.connect()
        self._subscriptions.append(InMemorySubscription(subject=subject, callback=callback))

    async def request(self, subject: str, event: Event, timeout: float = 30.0) -> Event | None:
        """No responder semantics offline; always returns None."""
        if not self._connected:
            await self.connect()
        return None

    async def close(self) -> None:
        async with self._lock:
            self._subscriptions.clear()
            self._connected = False

    @property
    def is_connected(self) -> bool:
        return self._connected

    @property
    def subscription_count(self) -> int:
        return len(self._subscriptions)