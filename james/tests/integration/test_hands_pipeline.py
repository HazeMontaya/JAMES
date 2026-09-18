"""Integration tests for Hands (detection/gesture logic + NATS serialization)."""

import json
from typing import Any, Callable

import numpy as np
import pytest
from james_hands.actions import ActionExecutor, ActionType
from james_hands.config import HandsConfig
from james_hands.detector import DetectionResult, HandLandmarks, PoseLandmarks
from james_hands.gestures import Gesture, GestureRecognizer, GestureType
from james_hands.pipeline import HandsPipeline

FINGER_TIPS = {"index": 8, "middle": 12, "ring": 16, "pinky": 20}
FINGER_MCPS = {"index": 5, "middle": 9, "ring": 13, "pinky": 17}

EXTENDED_Y = 0.30
FOLDED_Y = 0.52
MCP_Y = 0.50


def _hand(extended: dict[str, bool], thumb_extended: bool = False, handedness: str = "Right") -> HandLandmarks:
    """Build deterministic HandLandmarks with explicit finger/thumb states."""
    lm = np.zeros((21, 3), dtype=np.float32)
    # Palm base
    lm[0] = (0.30, 0.60, 0.0)
    # Thumb MCP, then tip
    lm[2] = (0.35, 0.50, 0.0)
    lm[4] = (0.60, EXTENDED_Y, 0.0) if thumb_extended else (0.33, 0.50, 0.0)
    # Fingers: MCP baseline x per finger, tips spread
    base_x = {"index": 0.45, "middle": 0.55, "ring": 0.65, "pinky": 0.75}
    for name, tip in FINGER_TIPS.items():
        mcp = FINGER_MCPS[name]
        lm[mcp] = (base_x[name], MCP_Y, 0.0)
        lm[tip] = (base_x[name], EXTENDED_Y if extended.get(name, False) else FOLDED_Y, 0.0)
    return HandLandmarks(landmarks=lm, handedness=handedness, confidence=0.99)


class FakeNats:
    """Minimal fake NATS client capturing publications."""

    def __init__(self) -> None:
        self.published: list[tuple[str, bytes]] = []
        self.subscriptions: list[tuple[str, Callable]] = []
        self.closed = False

    async def publish(self, subject: str, data: bytes) -> None:
        self.published.append((subject, data))

    async def subscribe(self, subject: str, cb: Callable, **_: Any) -> None:
        self.subscriptions.append((subject, cb))

    async def close(self) -> None:
        self.closed = True


def test_recognizer_open_palm() -> None:
    rec = GestureRecognizer(0.8)
    hand = _hand({"index": True, "middle": True, "ring": True, "pinky": True}, thumb_extended=True)
    gesture = rec.recognize(hand)
    assert gesture.type == GestureType.OPEN
    assert gesture.confidence > 0


def test_recognizer_fist() -> None:
    rec = GestureRecognizer(0.8)
    hand = _hand({})
    gesture = rec.recognize(hand)
    assert gesture.type == GestureType.FIST


def test_recognizer_point() -> None:
    rec = GestureRecognizer(0.8)
    hand = _hand({"index": True})
    gesture = rec.recognize(hand)
    assert gesture.type == GestureType.POINT


def test_recognizer_pinch() -> None:
    rec = GestureRecognizer(0.8)
    hand = _hand({"index": False})
    lm = hand.landmarks
    # Bring thumb tip and index tip together in 2D
    lm[4, 0] = 0.50
    lm[4, 1] = 0.30
    lm[8, 0] = 0.504
    lm[8, 1] = 0.30
    gesture = rec.recognize(hand)
    assert gesture.type == GestureType.PINCH


def test_recognizer_thumbs_up() -> None:
    rec = GestureRecognizer(0.8)
    hand = _hand({}, thumb_extended=True)
    gesture = rec.recognize(hand)
    assert gesture.type == GestureType.THUMBS_UP


def test_recognize_swipe_direction() -> None:
    rec = GestureRecognizer(0.8)
    hand = _hand({"index": True})
    prev = hand.landmarks.copy()
    prev[0, 0] = 0.30  # previous wrist x
    hand.landmarks[0, 0] = 0.60  # current wrist x -> right
    gesture = rec.recognize_swipe(hand, prev)
    assert gesture is not None
    assert gesture.type == GestureType.SWIPE_RIGHT


def _noop(*_args: Any, **_kwargs: Any) -> None:
    return None


def test_action_executor_mapping() -> None:
    import james_hands.actions as actions_mod

    executor = ActionExecutor(HandsConfig().actions)
    was_called = {"click": False}

    def fake_click(*_: Any, **__: Any) -> None:
        was_called["click"] = True

    actions_mod.pyautogui.click = fake_click  # type: ignore[method-assign]
    actions_mod.pyautogui.moveTo = _noop  # type: ignore[method-assign]
    actions_mod.pyautogui.position = lambda: (100, 100)  # type: ignore[method-assign]
    actions_mod.pyautogui.size = lambda: (1920, 1080)  # type: ignore[method-assign]
    executor.execute(Gesture(GestureType.PINCH, 0.9, "Right", {}, 0))
    assert was_called["click"] is True


def test_action_executor_none_mapping() -> None:
    executor = ActionExecutor(HandsConfig().actions)
    result = executor.execute(Gesture(GestureType.FIST, 0.9, "Right", {}, 0))
    assert result.success is False
    assert result.error == "No action mapped"


def test_action_executor_key_press_params() -> None:
    import james_hands.actions as actions_mod

    executor = ActionExecutor(HandsConfig().actions)
    pressed: list[str] = []
    actions_mod.pyautogui.press = pressed.append  # type: ignore[method-assign]
    actions_mod.pyautogui.moveTo = _noop  # type: ignore[method-assign]
    actions_mod.pyautogui.position = lambda: (100, 100)  # type: ignore[method-assign]
    actions_mod.pyautogui.size = lambda: (1920, 1080)  # type: ignore[method-assign]
    result = executor.execute_action_type(ActionType.KEY_PRESS, params={"key": "a"})
    assert result.success is True
    assert pressed == ["a"]


@pytest.mark.asyncio
async def test_publish_detection_serializes_numpy() -> None:
    """Numpy floats/arrays must serialize without error."""
    pipeline = HandsPipeline(HandsConfig())
    pipeline.nc = FakeNats()

    result = DetectionResult(
        hands=[_hand({})],
        pose=PoseLandmarks(landmarks=np.zeros((33, 4), dtype=np.float32), confidence=np.float32(0.9)),
    )
    await pipeline._publish_detection(result)

    assert pipeline.nc is not None
    assert len(pipeline.nc.published) >= 2
    for subject, data in pipeline.nc.published:
        assert subject in {"james.hands.gesture", "james.hands.pose", "james.hands.face"}
        json.loads(data.decode())  # must not raise


@pytest.mark.asyncio
async def test_handle_action_reply() -> None:
    import james_hands.actions as actions_mod
    from james_hands.pipeline import Msg

    # Patch pyautogui so executing a real action is safe
    actions_mod.pyautogui.moveTo = _noop  # type: ignore[method-assign]
    actions_mod.pyautogui.click = _noop  # type: ignore[method-assign]
    actions_mod.pyautogui.position = lambda: (100, 100)  # type: ignore[method-assign]
    actions_mod.pyautogui.size = lambda: (1920, 1080)  # type: ignore[method-assign]

    pipeline = HandsPipeline(HandsConfig())
    pipeline.nc = FakeNats()

    raw = json.dumps({"action": "left_click", "params": {}}).encode()
    msg = Msg(None, subject="james.hands.action", reply="james.hands.action.reply", data=raw)
    await pipeline._handle_action(msg)

    assert pipeline.nc is not None
    assert pipeline.nc.published
    payload = json.loads(pipeline.nc.published[-1][1].decode())
    assert "success" in payload