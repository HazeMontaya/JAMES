"""Face Server - FastAPI + WebSocket server for visualization"""

import asyncio
import json
import signal
from contextlib import asynccontextmanager, suppress

import nats
import structlog
from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import HTMLResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from nats.aio.client import Client as NatsClient
from nats.aio.msg import Msg
from starlette.requests import Request

from .config import FaceConfig
from .state import CognitiveState, EventEntry, StateSnapshot

logger = structlog.get_logger()


class ConnectionManager:
    """Manage WebSocket connections."""

    def __init__(self):
        self.active_connections: list[WebSocket] = []

    async def connect(self, websocket: WebSocket):
        await websocket.accept()
        self.active_connections.append(websocket)

    def disconnect(self, websocket: WebSocket):
        if websocket in self.active_connections:
            self.active_connections.remove(websocket)

    async def broadcast(self, message: dict):
        for connection in self.active_connections:
            with suppress(Exception):
                await connection.send_json(message)

    async def send_personal(self, websocket: WebSocket, message: dict):
        with suppress(Exception):
            await websocket.send_json(message)


class FaceServer:
    """Face visualization server."""

    def __init__(self, config: FaceConfig):
        self.config = config
        self.nc: NatsClient | None = None
        self.state = CognitiveState()
        self.history: list[StateSnapshot] = []
        self.manager = ConnectionManager()
        self._running = False
        self._tasks: list[asyncio.Task] = []

    @asynccontextmanager
    async def lifespan(self, app: FastAPI):
        """Application lifespan."""
        await self.start()
        yield
        await self.stop()

    def create_app(self) -> FastAPI:
        """Create FastAPI application."""
        app = FastAPI(
            title="JAMES Face",
            description="Cognitive State Visualizer",
            version="0.1.0",
            lifespan=self.lifespan,
        )

        # Static files
        if self.config.web.static_path.exists():
            app.mount("/static", StaticFiles(directory=self.config.web.static_path), name="static")

        # Templates
        templates = Jinja2Templates(directory=str(self.config.web.template_path))

        @app.get("/", response_class=HTMLResponse)
        async def index(request: Request):
            return templates.TemplateResponse("index.html", {"request": request})

        @app.get("/api/state")
        async def get_state():
            return self.state.to_dict()

        @app.get("/api/history")
        async def get_history(limit: int = 100):
            return [s.to_dict() for s in self.history[-limit:]]

        @app.get("/api/events")
        async def get_events(limit: int = 100):
            return [
                {
                    "id": e.id,
                    "event_type": e.event_type,
                    "source": e.source,
                    "timestamp": e.timestamp.isoformat(),
                    "payload": e.payload,
                    "severity": e.severity,
                }
                for e in self.state.recent_events[-limit:]
            ]

        @app.get("/api/skills")
        async def get_skills():
            return self.state.to_dict().get("skills", [])

        @app.get("/api/capabilities")
        async def get_capabilities():
            return self.state.to_dict().get("capabilities", [])

        @app.get("/api/memory")
        async def get_memory():
            return self.state.to_dict().get("memory", {})

        @app.get("/api/goals")
        async def get_goals():
            return self.state.to_dict().get("active_goals", [])

        @app.websocket("/ws")
        async def websocket_endpoint(websocket: WebSocket):
            await self.manager.connect(websocket)
            try:
                # Send initial state
                await websocket.send_json({"type": "state", "data": self.state.to_dict()})
                while True:
                    data = await websocket.receive_json()
                    # Handle client messages if needed
            except WebSocketDisconnect:
                self.manager.disconnect(websocket)
            except Exception as e:
                logger.error("WebSocket error", error=str(e))
                self.manager.disconnect(websocket)

        return app

    async def start(self) -> None:
        """Start the server."""
        await self._connect_nats()
        self._running = True

        # Start NATS subscriptions
        self._tasks.append(asyncio.create_task(self._subscribe_events()))

        # Create and start FastAPI
        app = self.create_app()

        import uvicorn
        config = uvicorn.Config(
            app,
            host=self.config.web.host,
            port=self.config.web.port,
            log_level=self.config.log_level.lower(),
        )
        server = uvicorn.Server(config)
        self._tasks.append(asyncio.create_task(server.serve()))

        logger.info("Face server started",
                    host=self.config.web.host,
                    port=self.config.web.port)

    async def stop(self) -> None:
        """Stop the server."""
        self._running = False
        for task in self._tasks:
            task.cancel()
        await asyncio.gather(*self._tasks, return_exceptions=True)

        if self.nc:
            await self.nc.close()
        logger.info("Face server stopped")

    async def _connect_nats(self) -> None:
        """Connect to NATS."""
        self.nc = await nats.connect(self.config.nats.url)
        logger.info("Face server connected to NATS", url=self.config.nats.url)

    async def _subscribe_events(self) -> None:
        """Subscribe to NATS events and update state."""
        prefix = self.config.nats.subject_prefix

        # Subscribe to system events
        subjects = [
            f"{prefix}.state",
            f"{prefix}.events",
            "james.events.>",  # All JAMES events
        ]

        for subject in subjects:
            await self.nc.subscribe(subject, cb=self._handle_event)

        logger.info("Subscribed to NATS events")

    async def _handle_event(self, msg: Msg) -> None:
        """Handle incoming NATS event and update state."""
        try:
            data = json.loads(msg.data.decode())
            event_type = msg.subject

            # Create event entry
            event = EventEntry(
                event_type=event_type,
                source=data.get("source", "unknown"),
                payload=data,
            )
            self.state.recent_events.insert(0, event)
            # Keep only recent events
            self.state.recent_events = self.state.recent_events[:self.config.nats.event_history]

            # Update state based on event type
            await self._update_state_from_event(event_type, data)

            # Broadcast to WebSocket clients
            await self.manager.broadcast({
                "type": "event",
                "data": {
                    "id": event.id,
                    "event_type": event.event_type,
                    "source": event.source,
                    "timestamp": event.timestamp.isoformat(),
                    "payload": event.payload,
                    "severity": event.severity,
                }
            })

        except Exception as e:
            logger.error("Event handling error", error=str(e))

    async def _update_state_from_event(self, event_type: str, data: dict) -> None:
        """Update cognitive state from event."""
        # This is a simplified version - in reality you'd parse specific event types
        if event_type.endswith(".state"):
            # Full state update
            if "state" in data:
                self.state.state = data["state"]
            if "uptime_seconds" in data:
                self.state.uptime_seconds = data["uptime_seconds"]
        elif event_type.endswith(".goal"):
            # Goal updates
            pass
        elif event_type.endswith(".skill"):
            # Skill updates
            pass
        elif event_type.endswith(".memory"):
            # Memory updates
            pass
        elif event_type.endswith(".capability"):
            # Capability updates
            pass

        # Record snapshot
        snapshot = StateSnapshot(state=self.state)
        self.history.append(snapshot)
        if len(self.history) > 1000:
            self.history = self.history[-1000:]


async def main() -> None:
    """Main entry point for james-face."""
    config = FaceConfig()
    server = FaceServer(config)

    # Setup signal handlers
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, lambda: asyncio.create_task(server.stop()))

    try:
        await server.start()
        logger.info("Face server running. Press Ctrl+C to stop.")

        while server._running:
            await asyncio.sleep(1)

    except KeyboardInterrupt:
        pass
    finally:
        await server.stop()


if __name__ == "__main__":
    asyncio.run(main())