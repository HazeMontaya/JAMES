"""JAMES Runtime Main Entry Point"""
import asyncio
import logging
import signal
import sys
from pathlib import Path

from james_runtime.config import get_settings, load_settings_from_yaml
from james_runtime.core.runtime import JamesRuntime
from james_runtime.server.fastapi_app import app
from james_runtime.server.grpc import start_grpc_server
from james_runtime.integration.nats_capabilities import NatsCapabilityBridge

logger = logging.getLogger(__name__)


async def main():
    """Main entry point"""
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )

    settings_path = Path("config/runtime.yaml")
    if settings_path.exists():
        settings = load_settings_from_yaml(settings_path)
    else:
        settings = get_settings()

    runtime = JamesRuntime(settings)
    await runtime.initialize()

    # The Python ToolRegistry is the authoritative tool surface for the
    # sidecar. Expose it through the NATS capability bridge before Rust
    # starts discovery/synchronization.
    nats_bridge = NatsCapabilityBridge(runtime.agent_system.tool_registry)
    try:
        await nats_bridge.start()
        logger.info(
            "Python capability bridge started with %d tools",
            len(runtime.agent_system.tool_registry.list_tools()),
        )

        grpc_task = await start_grpc_server(runtime, settings.grpc_port)

        import uvicorn
        config = uvicorn.Config(
            "james_runtime.server.fastapi_app:app",
            host="0.0.0.0",
            port=settings.http_port,
            log_level=settings.log_level.lower(),
            lifespan="on",
        )
        server = uvicorn.Server(config)

        shutdown_event = asyncio.Event()

        def signal_handler():
            logger.info("Shutdown signal received")
            shutdown_event.set()

        for sig in (signal.SIGTERM, signal.SIGINT):
            try:
                asyncio.get_event_loop().add_signal_handler(sig, signal_handler)
            except NotImplementedError:
                pass

        server_task = asyncio.create_task(server.serve())
        await shutdown_event.wait()

        logger.info("Shutting down...")
        server.should_exit = True
        await runtime.shutdown()

        if grpc_task:
            grpc_task.cancel()
            try:
                await grpc_task
            except asyncio.CancelledError:
                pass

        if not server_task.done():
            await server_task
    finally:
        await nats_bridge.close()

    logger.info("Shutdown complete")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("Interrupted")
        sys.exit(0)
