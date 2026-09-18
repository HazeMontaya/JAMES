"""Event Bus for JAMES using NATS JetStream"""

import asyncio
import contextlib
import json
from collections.abc import Awaitable, Callable
from dataclasses import dataclass

import nats
import structlog
from nats.aio.client import Client as NatsClient
from nats.aio.msg import Msg
from nats.js import JetStreamContext
from nats.js.api import AckPolicy, ConsumerConfig, RetentionPolicy, StorageType, StreamConfig

from .models import Event

logger = structlog.get_logger()


@dataclass
class Subscription:
    subject: str
    callback: Callable[[Event], Awaitable[None]]
    consumer: str | None = None


class EventBus:
    def __init__(self, nats_url: str = "nats://localhost:4222"):
        self._nats_url = nats_url
        self._nc: NatsClient | None = None
        self._js: JetStreamContext | None = None
        self._subscriptions: list[Subscription] = []
        self._connected = False
        self._lock = asyncio.Lock()

    async def connect(self) -> None:
        async with self._lock:
            if self._connected:
                return

            self._nc = await nats.connect(self._nats_url)
            self._js = self._nc.jetstream()
            await self._ensure_streams()
            self._connected = True
            logger.info("EventBus connected", url=self._nats_url)

    async def _ensure_streams(self) -> None:
        stream_config = StreamConfig(
            name="JAMES_EVENTS",
            subjects=["james.>"],
            retention=RetentionPolicy.LIMITS,
            max_msgs=1000000,
            max_bytes=1024 * 1024 * 1024,
            max_age=86400,
            storage=StorageType.FILE,
            num_replicas=1,
        )
        with contextlib.suppress(Exception):
            assert self._js is not None
            await self._js.add_stream(stream_config)

    async def publish(self, subject: str, event: Event) -> None:
        if not self._connected:
            await self.connect()

        assert self._js is not None
        data = json.dumps(event.to_dict()).encode()
        ack = await self._js.publish(subject, data)
        logger.debug("Event published", subject=subject, event_id=event.id, stream=ack.stream, seq=ack.seq)

    async def subscribe(
        self,
        subject: str,
        callback: Callable[[Event], Awaitable[None]],
        durable: str | None = None,
    ) -> None:
        if not self._connected:
            await self.connect()

        consumer_config = ConsumerConfig(
            durable_name=durable,
            ack_policy=AckPolicy.EXPLICIT,
            max_deliver=3,
            ack_wait=30,
        )

        async def message_handler(msg: Msg) -> None:
            try:
                event_data = json.loads(msg.data.decode())
                event = Event.from_dict(event_data)
                await callback(event)
                await msg.ack()
            except Exception as e:
                logger.error("Event handling failed", subject=subject, error=str(e))
                await msg.nak()

        assert self._js is not None
        sub = await self._js.subscribe(subject, cb=message_handler, config=consumer_config)
        self._subscriptions.append(Subscription(subject=subject, callback=callback, consumer=durable))
        logger.info("Subscribed", subject=subject, durable=durable)

    async def request(self, subject: str, event: Event, timeout: float = 30.0) -> Event | None:
        if not self._connected:
            await self.connect()

        data = json.dumps(event.to_dict()).encode()
        try:
            assert self._nc is not None
            response = await self._nc.request(subject, data, timeout=timeout)
            response_data = json.loads(response.data.decode())
            return Event.from_dict(response_data)
        except Exception as e:
            logger.error("Request failed", subject=subject, error=str(e))
            return None

    async def close(self) -> None:
        async with self._lock:
            if self._js is not None:
                for sub in self._subscriptions:
                    with contextlib.suppress(Exception):
                        await self._js.delete_consumer("JAMES_EVENTS", sub.consumer or "")
            if self._nc:
                await self._nc.close()
            self._connected = False
            logger.info("EventBus closed")

    @property
    def is_connected(self) -> bool:
        return self._connected and self._nc is not None and not self._nc.is_closed
