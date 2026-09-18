"""Unit tests for the in-memory event bus."""

import pytest

from james_core.events import Event, InMemoryEventBus


@pytest.mark.asyncio
async def test_publish_subscribe() -> None:
    bus = InMemoryEventBus()
    await bus.connect()
    received: list[Event] = []

    async def handler(event: Event) -> None:
        received.append(event)

    await bus.subscribe("james.test", handler)
    await bus.publish("james.test", Event(type="test", payload={"key": "value"}))
    assert len(received) == 1
    assert received[0].payload["key"] == "value"


@pytest.mark.asyncio
async def test_subject_wildcard_single_token() -> None:
    bus = InMemoryEventBus()
    received: list[str] = []

    async def handler(event: Event) -> None:
        received.append(event.type)

    await bus.subscribe("james.*.ran", handler)
    await bus.publish("james.skill.ran", Event(type="skill.ran"))
    await bus.publish("james.goal.ran", Event(type="goal.ran"))
    assert received == ["skill.ran", "goal.ran"]


@pytest.mark.asyncio
async def test_subject_wildcard_trailing() -> None:
    bus = InMemoryEventBus()
    received: list[str] = []

    async def handler(event: Event) -> None:
        received.append(event.type)

    await bus.subscribe("james.goal.>", handler)
    await bus.publish("james.goal.started", Event(type="goal.started"))
    await bus.publish("james.goal.finished.again", Event(type="goal.finished.again"))
    await bus.publish("james.other", Event(type="other"))
    assert received == ["goal.started", "goal.finished.again"]


@pytest.mark.asyncio
async def test_multiple_subscribers() -> None:
    bus = InMemoryEventBus()
    a: list[str] = []
    b: list[str] = []

    async def ha(event: Event) -> None:
        a.append(event.type)

    async def hb(event: Event) -> None:
        b.append(event.type)

    await bus.subscribe("james.multi", ha)
    await bus.subscribe("james.multi", hb)
    await bus.publish("james.multi", Event(type="multi"))
    assert a == ["multi"]
    assert b == ["multi"]


@pytest.mark.asyncio
async def test_close_clears_subscriptions() -> None:
    bus = InMemoryEventBus()
    received: list[str] = []

    async def handler(event: Event) -> None:
        received.append(event.type)

    await bus.subscribe("james.test", handler)
    await bus.close()
    assert bus.is_connected is False
    assert bus.subscription_count == 0
    # auto-reconnect on publish, but no subscribers remain
    await bus.publish("james.test", Event(type="test"))
    assert received == []


@pytest.mark.asyncio
async def test_closed_bus_reconnects_on_publish() -> None:
    bus = InMemoryEventBus()
    await bus.close()
    received: list[str] = []

    async def handler(event: Event) -> None:
        received.append(event.type)

    await bus.subscribe("james.test", handler)
    await bus.publish("james.test", Event(type="test"))
    assert received == ["test"]


@pytest.mark.asyncio
async def test_payload_round_trip_on_publish() -> None:
    bus = InMemoryEventBus()
    received: list[dict] = []

    async def handler(event: Event) -> None:
        received.append(event.payload)

    await bus.subscribe("james.rt", handler)
    original = Event(type="rt", payload={"a": 1, "b": [1, 2], "c": {"d": "e"}})
    await bus.publish("james.rt", original)

    # Subscribers receive a deserialized copy, not the original dict object
    revived = received[0]
    assert revived == original.payload
    assert revived["b"] is not original.payload["b"]