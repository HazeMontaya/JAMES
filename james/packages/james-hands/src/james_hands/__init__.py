"""JAMES Hands - Gesture Control & Computer Interaction (AGPL-3.0)"""

from .actions import ActionExecutor
from .config import HandsConfig
from .detector import FaceMeshDetector, HandDetector, PoseDetector
from .gestures import Gesture, GestureRecognizer
from .pipeline import HandsPipeline

__version__ = "0.1.0"

__all__ = [
    "ActionExecutor",
    "FaceMeshDetector",
    "Gesture",
    "GestureRecognizer",
    "HandDetector",
    "HandsConfig",
    "HandsPipeline",
    "PoseDetector",
]