"""Gesture recognition from hand landmarks"""

from dataclasses import dataclass, field
from enum import StrEnum

import numpy as np
import structlog

from .detector import HandLandmarks

logger = structlog.get_logger()


class GestureType(StrEnum):
    """Recognized gesture types."""
    NONE = "none"
    POINT = "point"
    PINCH = "pinch"
    GRAB = "grab"
    OPEN = "open"
    FIST = "fist"
    THUMBS_UP = "thumbs_up"
    THUMBS_DOWN = "thumbs_down"
    SWIPE_LEFT = "swipe_left"
    SWIPE_RIGHT = "swipe_right"
    SWIPE_UP = "swipe_up"
    SWIPE_DOWN = "swipe_down"
    TWO_FINGER_PINCH = "two_finger_pinch"
    THREE_FINGER = "three_finger"
    FOUR_FINGER = "four_finger"
    FIVE_FINGER = "five_finger"
    CUSTOM = "custom"


@dataclass
class Gesture:
    """Recognized gesture with metadata."""
    type: GestureType
    confidence: float
    hand: str  # "Left" or "Right"
    landmarks: dict  # Key landmark positions
    timestamp: float
    metadata: dict = field(default_factory=dict)


class GestureRecognizer:
    """Recognize gestures from hand landmarks."""

    # Finger tip indices (MediaPipe)
    THUMB_TIP = 4
    INDEX_TIP = 8
    MIDDLE_TIP = 12
    RING_TIP = 16
    PINKY_TIP = 20

    # Finger MCP joints
    THUMB_MCP = 2
    INDEX_MCP = 5
    MIDDLE_MCP = 9
    RING_MCP = 13
    PINKY_MCP = 17

    def __init__(self, sensitivity: float = 0.8):
        self.sensitivity = sensitivity
        self._prev_landmarks: dict[str, np.ndarray] = {}

    def recognize(self, hand: HandLandmarks) -> Gesture:
        """Recognize gesture from hand landmarks."""
        lm = hand.landmarks

        # Calculate finger states
        fingers = self._get_finger_states(lm)
        thumb_state = self._get_thumb_state(lm)

        # Recognize static gestures
        gesture = self._classify_static(fingers, thumb_state, hand)

        # Add dynamic info
        gesture.hand = hand.handedness
        gesture.landmarks = self._get_key_landmarks(lm)
        gesture.metadata = {"fingers": fingers, "thumb": thumb_state}

        return gesture

    def _get_finger_states(self, lm: np.ndarray) -> dict[str, bool]:
        """Determine which fingers are extended."""
        states = {}

        # Index
        states["index"] = lm[self.INDEX_TIP, 1] < lm[self.INDEX_MCP, 1] - 0.02
        # Middle
        states["middle"] = lm[self.MIDDLE_TIP, 1] < lm[self.MIDDLE_MCP, 1] - 0.02
        # Ring
        states["ring"] = lm[self.RING_TIP, 1] < lm[self.RING_MCP, 1] - 0.02
        # Pinky
        states["pinky"] = lm[self.PINKY_TIP, 1] < lm[self.PINKY_MCP, 1] - 0.02

        return states

    def _get_thumb_state(self, lm: np.ndarray) -> bool:
        """Check if thumb is extended (to the side)."""
        # Thumb tip x vs thumb MCP x (for right hand)
        thumb_tip = lm[self.THUMB_TIP]
        thumb_mcp = lm[self.THUMB_MCP]
        index_mcp = lm[self.INDEX_MCP]

        # Thumb extended if tip is further from palm than MCP
        return abs(thumb_tip[0] - index_mcp[0]) > abs(thumb_mcp[0] - index_mcp[0]) + 0.03

    def _classify_static(self, fingers: dict, thumb: bool, hand: HandLandmarks) -> Gesture:
        """Classify static hand pose."""
        extended = sum(fingers.values()) + (1 if thumb else 0)

        lm = hand.landmarks

        # Calculate distances for pinch detection
        thumb_tip = lm[4]
        index_tip = lm[8]
        middle_tip = lm[12]

        pinch_dist = np.linalg.norm(thumb_tip[:2] - index_tip[:2])
        pinch_dist_middle = np.linalg.norm(thumb_tip[:2] - middle_tip[:2])

        pinch_threshold = 0.05

        if pinch_dist < pinch_threshold and pinch_dist_middle < pinch_threshold:
            return Gesture(GestureType.PINCH, 0.9, hand.handedness, {}, 0)

        if pinch_dist < pinch_threshold:
            return Gesture(GestureType.PINCH, 0.8, hand.handedness, {}, 0)

        # Count extended fingers
        if extended == 0:
            return Gesture(GestureType.FIST, 0.9, hand.handedness, {}, 0)
        elif extended == 1 and fingers.get("index", False):
            return Gesture(GestureType.POINT, 0.9, hand.handedness, {}, 0)
        elif extended == 2 and fingers.get("index", False) and fingers.get("middle", False):
            return Gesture(GestureType.TWO_FINGER_PINCH, 0.8, hand.handedness, {}, 0)
        elif extended == 3 and fingers.get("index", False) and fingers.get("middle", False) and fingers.get("ring", False):
            return Gesture(GestureType.THREE_FINGER, 0.8, hand.handedness, {}, 0)
        elif extended == 4 and thumb:
            return Gesture(GestureType.FOUR_FINGER, 0.8, hand.handedness, {}, 0)
        elif extended == 5:
            return Gesture(GestureType.OPEN, 0.9, hand.handedness, {}, 0)
        elif thumb and not any(fingers.values()):
            return Gesture(GestureType.THUMBS_UP, 0.8, hand.handedness, {}, 0)

        return Gesture(GestureType.NONE, 0.0, hand.handedness, {}, 0)

    def _get_key_landmarks(self, lm: np.ndarray) -> dict:
        """Extract key landmark positions for gesture metadata."""
        return {
            "thumb_tip": lm[4].tolist(),
            "index_tip": lm[8].tolist(),
            "middle_tip": lm[12].tolist(),
            "ring_tip": lm[16].tolist(),
            "pinky_tip": lm[20].tolist(),
            "wrist": lm[0].tolist(),
        }

    def recognize_swipe(self, hand: HandLandmarks, prev_landmarks: np.ndarray | None = None) -> Gesture | None:
        """Recognize swipe gestures from movement."""
        if prev_landmarks is None:
            return None

        curr_wrist = hand.landmarks[0]
        prev_wrist = prev_landmarks[0]

        dx = curr_wrist[0] - prev_wrist[0]
        dy = curr_wrist[1] - prev_wrist[1]

        dist = np.sqrt(dx**2 + dy**2)

        if dist < 0.1:  # Movement threshold
            return None

        # Determine direction
        if abs(dx) > abs(dy):
            if dx > 0:
                return Gesture(GestureType.SWIPE_RIGHT, min(dist * 5, 1.0), hand.handedness, {}, 0)
            else:
                return Gesture(GestureType.SWIPE_LEFT, min(dist * 5, 1.0), hand.handedness, {}, 0)
        elif dy > 0:
            return Gesture(GestureType.SWIPE_DOWN, min(dist * 5, 1.0), hand.handedness, {}, 0)
        else:
            return Gesture(GestureType.SWIPE_UP, min(dist * 5, 1.0), hand.handedness, {}, 0)