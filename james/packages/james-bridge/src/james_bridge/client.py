"""Bridge client - main entry point for Python bridge"""

import asyncio
import contextlib
import signal

import structlog

from .config import BridgeConfig
from .handler import CapabilityHandler

logger = structlog.get_logger()


class BridgeClient:
    """Main bridge client that manages the connection and request handling"""

    def __init__(self, config: BridgeConfig | None = None):
        self.config = config or BridgeConfig()
        self.handler = CapabilityHandler(self.config)
        self._running = False

    async def start(self) -> None:
        """Start the bridge client"""
        logger.info("Starting JAMES Python Bridge client")
        await self.handler.start_listening()
        self._running = True
        logger.info("Bridge client started")

    async def run_forever(self) -> None:
        """Run until shutdown signal"""
        await self.start()

        # Set up signal handlers
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            with contextlib.suppress(NotImplementedError):
                # Windows doesn't support add_signal_handler
                loop.add_signal_handler(sig, lambda: asyncio.create_task(self.shutdown()))

        # Keep running
        while self._running:
            await asyncio.sleep(1)

    async def shutdown(self) -> None:
        """Graceful shutdown"""
        if not self._running:
            return

        logger.info("Shutting down bridge client")
        self._running = False
        await self.handler.close()
        logger.info("Bridge client stopped")


async def main() -> None:
    """Main entry point"""
    client = BridgeClient()
    try:
        await client.run_forever()
    except KeyboardInterrupt:
        pass
    except Exception as e:
        logger.error("Bridge client error", error=str(e))
        raise


if __name__ == "__main__":
    asyncio.run(main())