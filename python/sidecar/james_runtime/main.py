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

logger = logging.getLogger(__name__)


async def main():
    """Main entry point"""
    # Setup logging
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )
    
    # Load settings
    settings_path = Path("config/runtime.yaml")
    if settings_path.exists():
        settings = load_settings_from_yaml(settings_path)
    else:
        from james_runtime.config import get_settings
        settings = get_settings()
    
    # Initialize runtime
    runtime = JamesRuntime(settings)
    await runtime.initialize()
    
    # Start gRPC server
    grpc_task = await start_grpc_server(runtime, settings.grpc_port)
    
    # Run FastAPI
    import uvicorn
    config = uvicorn.Config(
        "james_runtime.server.fastapi_app:app",
        host="0.0.0.0",
        port=settings.http_port,
        log_level=settings.log_level.lower(),
        lifespan="on",
    )
    server = uvicorn.Server(config)
    
    # Handle shutdown signals
    shutdown_event = asyncio.Event()
    
    def signal_handler():
        logger.info("Shutdown signal received")
        shutdown_event.set()
    
    for sig in (signal.SIGTERM, signal.SIGINT):
        try:
            asyncio.get_event_loop().add_signal_handler(sig, signal_handler)
        except NotImplementedError:
            # Windows doesn't support add_signal_handler
            pass
    
    # Run server
    server_task = asyncio.create_task(server.serve())
    
    # Wait for shutdown
    await shutdown_event.wait()
    
    # Graceful shutdown
    logger.info("Shutting down...")
    server.should_exit = True
    await runtime.shutdown()
    
    if grpc_task:
        grpc_task.cancel()
        try:
            await grpc_task
        except asyncio.CancelledError:
            pass
    
    logger.info("Shutdown complete")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("Interrupted")
        sys.exit(0)