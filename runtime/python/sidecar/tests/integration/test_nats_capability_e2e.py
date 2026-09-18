import os

import pytest
from nats.aio.client import Client as NATS

from james_runtime.integration.nats_capabilities import NatsCapabilityBridge
from james_runtime.tools.builtins import Calculator
from james_runtime.tools.registry import ToolRegistry


@pytest.mark.asyncio
async def test_nats_calculator_round_trip():
    url = os.getenv("JAMES_TEST_NATS_URL")
    if not url:
        pytest.skip("set JAMES_TEST_NATS_URL to run the live NATS integration test")

    registry = ToolRegistry()
    registry.register(Calculator())
    bridge = NatsCapabilityBridge(
        registry,
        nats_url=url,
        subject_prefix="james.test.bridge",
        service_name="james-python-test",
    )
    client = NATS()

    await bridge.start()
    await client.connect(servers=[url], name="james-python-test-client")

    try:
        response = await client.request(
            "james.test.bridge.capability.execute.calculator",
            b'{"request_id":"e2e-1","capability_id":"calculator","caller":"e2e-test","input":{"operation":"multiply","a":"25","b":"4"}}',
            timeout=5,
        )
        payload = response.data.decode("utf-8")
        assert '"request_id": "e2e-1"' in payload
        assert '"success": true' in payload
        assert '"output": "100.0"' in payload
    finally:
        await client.drain()
        await bridge.close()
