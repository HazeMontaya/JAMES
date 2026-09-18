"""Action executor - map gestures to system actions"""

import time
from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any, ClassVar

import pyautogui
import structlog

from .config import ActionConfig
from .gestures import Gesture, GestureType

logger = structlog.get_logger()


class ActionType(StrEnum):
    """System action types."""
    NONE = "none"
    LEFT_CLICK = "left_click"
    RIGHT_CLICK = "right_click"
    DOUBLE_CLICK = "double_click"
    DRAG_START = "drag_start"
    DRAG_END = "drag_end"
    SCROLL_UP = "scroll_up"
    SCROLL_DOWN = "scroll_down"
    SCROLL_LEFT = "scroll_left"
    SCROLL_RIGHT = "scroll_right"
    KEY_PRESS = "key_press"
    TYPE_TEXT = "type_text"
    HOTKEY = "hotkey"
    MOVE_MOUSE = "move_mouse"
    CUSTOM = "custom"


@dataclass
class Action:
    """System action to execute."""
    type: ActionType
    params: dict[str, Any] = field(default_factory=dict)
    gesture: Gesture | None = None


@dataclass
class ActionResult:
    """Result of action execution."""
    success: bool
    action: Action
    error: str = ""
    duration_ms: float = 0.0


class ActionExecutor:
    """Execute system actions from gestures."""

    GESTURE_ACTION_MAP: ClassVar[dict] = {
        GestureType.POINT: ActionType.MOVE_MOUSE,
        GestureType.PINCH: ActionType.LEFT_CLICK,
        GestureType.GRAB: ActionType.DRAG_START,
        GestureType.OPEN: ActionType.DRAG_END,
        GestureType.FIST: ActionType.NONE,
        GestureType.THUMBS_UP: ActionType.SCROLL_UP,
        GestureType.THUMBS_DOWN: ActionType.SCROLL_DOWN,
        GestureType.SWIPE_LEFT: ActionType.SCROLL_LEFT,
        GestureType.SWIPE_RIGHT: ActionType.SCROLL_RIGHT,
        GestureType.SWIPE_UP: ActionType.SCROLL_UP,
        GestureType.SWIPE_DOWN: ActionType.SCROLL_DOWN,
        GestureType.TWO_FINGER_PINCH: ActionType.RIGHT_CLICK,
        GestureType.THREE_FINGER: ActionType.HOTKEY,
        GestureType.FOUR_FINGER: ActionType.DOUBLE_CLICK,
    }

    def __init__(self, config: ActionConfig):
        self.config = config
        self._dragging = False
        self._last_click_time = 0
        self._last_pos = None

        # PyAutoGUI settings
        pyautogui.FAILSAFE = True
        pyautogui.PAUSE = 0.01

    def execute(self, gesture: Gesture, params: dict[str, Any] | None = None) -> ActionResult:
        """Execute action for gesture."""
        import time
        start = time.perf_counter()

        action_type = self.GESTURE_ACTION_MAP.get(gesture.type, ActionType.NONE)

        if action_type == ActionType.NONE:
            return ActionResult(False, Action(ActionType.NONE, gesture=gesture), "No action mapped")

        action = Action(action_type, params=params or {}, gesture=gesture)

        try:
            success = self._execute_action(action, gesture)
            duration = (time.perf_counter() - start) * 1000
            return ActionResult(success, action, duration_ms=duration)
        except Exception as e:
            duration = (time.perf_counter() - start) * 1000
            return ActionResult(False, action, str(e), duration)

    def execute_action_type(self, action_type: ActionType, params: dict[str, Any] | None = None) -> ActionResult:
        """Execute an explicit action type, bypassing gesture mapping."""
        import time
        start = time.perf_counter()

        if action_type == ActionType.NONE:
            return ActionResult(False, Action(ActionType.NONE), "No action mapped")

        action = Action(action_type, params=params or {}, gesture=None)
        synthetic = Gesture(GestureType.CUSTOM, 1.0, "Right", {}, 0)
        try:
            success = self._execute_action(action, synthetic)
            duration = (time.perf_counter() - start) * 1000
            return ActionResult(success, action, duration_ms=duration)
        except Exception as e:
            duration = (time.perf_counter() - start) * 1000
            return ActionResult(False, action, str(e), duration)

    def _execute_action(self, action: Action, gesture: Gesture) -> bool:
        """Execute the actual system action."""
        # Get hand position for mouse movement
        x, y = self._get_cursor_pos(gesture)

        # Dispatch table for action handlers
        handlers = {
            ActionType.MOVE_MOUSE: lambda: pyautogui.moveTo(x, y, duration=0.1),
            ActionType.LEFT_CLICK: lambda: (pyautogui.click(x, y) if self._can_click() else None, setattr(self, '_last_click_time', time.time())),
            ActionType.RIGHT_CLICK: lambda: pyautogui.rightClick(x, y),
            ActionType.DOUBLE_CLICK: lambda: pyautogui.doubleClick(x, y),
            ActionType.DRAG_START: lambda: (pyautogui.mouseDown(x, y), setattr(self, '_dragging', True)) if not self._dragging else None,
            ActionType.DRAG_END: lambda: (pyautogui.mouseUp(x, y), setattr(self, '_dragging', False)) if self._dragging else None,
            ActionType.SCROLL_UP: lambda: pyautogui.scroll(int(100 * self.config.scroll_sensitivity), x, y),
            ActionType.SCROLL_DOWN: lambda: pyautogui.scroll(int(-100 * self.config.scroll_sensitivity), x, y),
            ActionType.SCROLL_LEFT: lambda: pyautogui.hscroll(int(-100 * self.config.scroll_sensitivity), x, y),
            ActionType.SCROLL_RIGHT: lambda: pyautogui.hscroll(int(100 * self.config.scroll_sensitivity), x, y),
            ActionType.HOTKEY: lambda: pyautogui.hotkey('alt', 'tab'),
            ActionType.KEY_PRESS: lambda: pyautogui.press(str(action.params.get('key', 'space'))),
        }

        handler = handlers.get(action.type)
        if handler:
            handler()
            return True
        return False

    def _get_cursor_pos(self, gesture: Gesture) -> tuple[int, int]:
        """Map hand position to screen coordinates."""
        if gesture.landmarks:
            # Use index finger tip or wrist
            wrist = gesture.landmarks.get("wrist", [0.5, 0.5, 0])
            x = int(wrist[0] * pyautogui.size().width)
            y = int(wrist[1] * pyautogui.size().height)
            return (x, y)
        return pyautogui.position()

    def _can_click(self) -> bool:
        """Check click cooldown."""
        import time
        return (time.time() - self._last_click_time) * 1000 >= self.config.click_cooldown_ms


# Convenience function for direct action execution
async def execute_action(action_type: ActionType, params: dict[str, Any] | None = None) -> ActionResult:
    """Execute a specific action directly."""
    config = ActionConfig()
    executor = ActionExecutor(config)
    return executor.execute_action_type(action_type, params=params)