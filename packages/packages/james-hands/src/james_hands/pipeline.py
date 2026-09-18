"""Hands Pipeline - Main orchestrator for gesture detection and action execution"""

import asyncio
import json
import signal
from dataclasses import dataclass, field

import cv2
import nats
import numpy as np
import structlog
from nats.aio.client import Client as NatsClient
from nats.aio.msg import Msg

from .actions import ActionExecutor, ActionResult
from .config import HandsConfig
from .detector import DetectionResult, FaceMeshDetector, HandDetector, PoseDetector
from .gestures import Gesture, GestureRecognizer, GestureType

logger = structlog.get_logger()


@dataclass
class HandsStatus:
    camera_active: bool = False
    detectors_loaded: bool = False
    current_gesture: str = "none"
    actions_executed: int = 0
    errors: list[str] = field(default_factory=list)


class HandsPipeline:
    """Main hands pipeline coordinating detection, gestures, actions, and NATS."""

    def __init__(self, config: HandsConfig):
        self.config = config
        self.nc: NatsClient | None = None
        self.cap: cv2.VideoCapture | None = None
        self.hand_detector = HandDetector(config.detection)
        self.pose_detector = PoseDetector(config.detection)
        self.face_detector = FaceMeshDetector(config.detection)
        self.gesture_recognizer = GestureRecognizer(config.gestures.sensitivity)
        self.action_executor = ActionExecutor(config.actions)
        self.status = HandsStatus()
        self._running = False
        self._tasks: list[asyncio.Task] = []
        self._prev_landmarks: dict[str, np.ndarray] = {}

    async def connect(self) -> None:
        """Connect to NATS."""
        self.nc = await nats.connect(self.config.nats.url)
        logger.info("Hands pipeline connected to NATS", url=self.config.nats.url)

    async def initialize(self) -> None:
        """Initialize detectors and camera."""
        await self.connect()

        # Initialize detectors
        self.hand_detector.initialize()
        self.pose_detector.initialize()
        self.face_detector.initialize()
        self.status.detectors_loaded = True

        # Open camera
        await self._open_camera()
        self.status.camera_active = True

        logger.info("Hands pipeline initialized")

    async def _open_camera(self) -> None:
        """Open video capture device."""
        cam = self.config.camera
        self.cap = cv2.VideoCapture(cam.device_id, getattr(cv2, f"CAP_{cam.backend.upper()}", cv2.CAP_ANY))
        self.cap.set(cv2.CAP_PROP_FRAME_WIDTH, cam.width)
        self.cap.set(cv2.CAP_PROP_FRAME_HEIGHT, cam.height)
        self.cap.set(cv2.CAP_PROP_FPS, cam.fps)

        if not self.cap.isOpened():
            raise RuntimeError(f"Failed to open camera {cam.device_id}")

        logger.info("Camera opened", width=cam.width, height=cam.height, fps=cam.fps)

    async def start(self) -> None:
        """Start the hands pipeline."""
        self._running = True

        # Subscribe to NATS
        prefix = self.config.nats.subject_prefix
        await self.nc.subscribe(f"{prefix}.config", cb=self._handle_config)
        await self.nc.subscribe(f"{prefix}.control", cb=self._handle_control)
        await self.nc.subscribe(f"{prefix}.action", cb=self._handle_action)

        # Start processing loop
        self._tasks.append(asyncio.create_task(self._process_loop()))
        self._tasks.append(asyncio.create_task(self._publish_status()))

        logger.info("Hands pipeline started")

    async def stop(self) -> None:
        """Stop the pipeline."""
        self._running = False
        for task in self._tasks:
            task.cancel()
        await asyncio.gather(*self._tasks, return_exceptions=True)

        if self.cap:
            self.cap.release()
        self.hand_detector.close()
        self.pose_detector.close()
        self.face_detector.close()

        if self.nc:
            await self.nc.close()
        logger.info("Hands pipeline stopped")

    async def _process_loop(self) -> None:
        """Main processing loop: capture -> detect -> recognize -> act."""
        while self._running:
            try:
                # Read frame
                ret, frame = self.cap.read()
                if not ret:
                    await asyncio.sleep(0.01)
                    continue

                # Detect
                hands = self.hand_detector.detect(frame)
                pose = self.pose_detector.detect(frame)
                face = self.face_detector.detect(frame)

                result = DetectionResult(
                    hands=hands,
                    pose=pose,
                    face=face,
                    timestamp=asyncio.get_event_loop().time(),
                    frame_shape=frame.shape[:2],
                )

                # Process gestures
                for hand in hands:
                    gesture = self.gesture_recognizer.recognize(hand)

                    # Check for swipe
                    hand_key = f"{hand.handedness}_{id(hand)}"
                    swipe = self.gesture_recognizer.recognize_swipe(hand, self._prev_landmarks.get(hand_key))
                    if swipe and swipe.type != GestureType.NONE:
                        gesture = swipe

                    self.status.current_gesture = gesture.type.value

                    # Execute action
                    if gesture.type != GestureType.NONE:
                        result = self.action_executor.execute(gesture)
                        if result.success:
                            self.status.actions_executed += 1
                            await self._publish_action_result(result, gesture)

                    self._prev_landmarks[hand_key] = hand.landmarks

                # Publish detection results
                await self._publish_detection(result)

                # Small delay to prevent CPU hogging
                await asyncio.sleep(0.001)

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("Processing loop error", error=str(e))
                await asyncio.sleep(0.1)

    async def _publish_detection(self, result: DetectionResult) -> None:
        """Publish detection results to NATS."""
        if not self.nc:
            return

        prefix = self.config.nats.subject_prefix

        # Publish hands
        for hand in result.hands:
            await self.nc.publish(
                f"{prefix}.gesture",
                json.dumps({
                    "type": "hand",
                    "handedness": hand.handedness,
                    "confidence": float(hand.confidence),
                    "landmarks": hand.landmarks.tolist(),
                }).encode()
            )

        # Publish pose
        if result.pose:
            await self.nc.publish(
                f"{prefix}.pose",
                json.dumps({
                    "landmarks": result.pose.landmarks.tolist(),
                    "confidence": float(result.pose.confidence),
                }).encode()
            )

        # Publish face
        if result.face:
            await self.nc.publish(
                f"{prefix}.face",
                json.dumps({
                    "landmarks": result.face.landmarks.tolist(),
                    "confidence": float(result.face.confidence),
                }).encode()
            )

    async def _publish_action_result(self, result: ActionResult, gesture: Gesture) -> None:
        """Publish action execution result."""
        if not self.nc:
            return

        await self.nc.publish(
            f"{self.config.nats.subject_prefix}.feedback",
            json.dumps({
                "action": result.action.type.value,
                "gesture": gesture.type.value,
                "success": result.success,
                "duration_ms": result.duration_ms,
                "error": result.error,
            }).encode()
        )

    async def _publish_status(self) -> None:
        """Periodically publish status."""
        while self._running:
            try:
                await self.nc.publish(
                    f"{self.config.nats.subject_prefix}.status",
                    json.dumps({
                        "camera_active": self.status.camera_active,
                        "detectors_loaded": self.status.detectors_loaded,
                        "current_gesture": self.status.current_gesture,
                        "actions_executed": self.status.actions_executed,
                        "errors": self.status.errors or [],
                    }).encode()
                )
                await asyncio.sleep(5)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("Status publish error", error=str(e))
                await asyncio.sleep(5)

    async def _handle_config(self, msg: Msg) -> None:
        """Handle config updates."""
        try:
            data = json.loads(msg.data.decode())
            if "sensitivity" in data:
                self.config.gestures.sensitivity = data["sensitivity"]
                self.gesture_recognizer.sensitivity = data["sensitivity"]
            logger.info("Hands config updated", data=data)
        except Exception as e:
            logger.error("Config error", error=str(e))

    async def _handle_control(self, msg: Msg) -> None:
        """Handle control commands."""
        try:
            data = json.loads(msg.data.decode())
            command = data.get("command")

            if command == "status":
                await self._publish_status()
            elif command == "reload_detectors":
                self.hand_detector.close()
                self.pose_detector.close()
                self.face_detector.close()
                self.hand_detector.initialize()
                self.pose_detector.initialize()
                self.face_detector.initialize()
                self.status.detectors_loaded = True
        except Exception as e:
            logger.error("Control error", error=str(e))

    async def _handle_action(self, msg: Msg) -> None:
        """Handle direct action requests."""
        try:
            data = json.loads(msg.data.decode())
            action_type = data.get("action")
            params = data.get("params", {})

            if action_type:
                from .actions import ActionExecutor, ActionType

                executor = ActionExecutor(self.config.actions)
                result = executor.execute_action_type(ActionType(action_type), params=params)

                if msg.reply:
                    await self.nc.publish(msg.reply, json.dumps({
                        "action": action_type,
                        "success": result.success,
                        "error": result.error,
                    }).encode())
        except Exception as e:
            logger.error("Action error", error=str(e))


async def main() -> None:
    """Main entry point for james-hands."""
    config = HandsConfig()
    pipeline = HandsPipeline(config)

    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, lambda: asyncio.create_task(pipeline.stop()))

    try:
        await pipeline.initialize()
        await pipeline.start()
        logger.info("Hands pipeline running. Press Ctrl+C to stop.")

        while pipeline._running:
            await asyncio.sleep(1)

    except KeyboardInterrupt:
        pass
    finally:
        await pipeline.stop()


if __name__ == "__main__":
    asyncio.run(main())