"""Hands configuration"""

from typing import Literal

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class CameraConfig(BaseSettings):
    device_id: int = 0
    width: int = 1280
    height: int = 720
    fps: int = 30
    backend: Literal["auto", "v4l2", "msmf", "dshow"] = "auto"


class DetectionConfig(BaseSettings):
    model_complexity: Literal[0, 1, 2] = 1
    min_detection_confidence: float = 0.7
    min_tracking_confidence: float = 0.5
    max_hands: int = 2
    max_faces: int = 1
    enable_segmentation: bool = False


class GestureConfig(BaseSettings):
    enabled: bool = True
    sensitivity: float = 0.8
    click_threshold: float = 0.05
    swipe_threshold: float = 0.15
    pinch_threshold: float = 0.03
    hold_duration_ms: int = 500
    custom_gestures: dict = Field(default_factory=dict)


class ActionConfig(BaseSettings):
    click_cooldown_ms: int = 300
    scroll_sensitivity: float = 1.0
    drag_threshold: float = 0.02
    enable_system_actions: bool = True


class NATSConfig(BaseSettings):
    url: str = "nats://localhost:4222"
    subject_prefix: str = "james.hands"


class HandsConfig(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="JAMES_HANDS_",
        env_nested_delimiter="__",
    )

    camera: CameraConfig = Field(default_factory=CameraConfig)
    detection: DetectionConfig = Field(default_factory=DetectionConfig)
    gestures: GestureConfig = Field(default_factory=GestureConfig)
    actions: ActionConfig = Field(default_factory=ActionConfig)
    nats: NATSConfig = Field(default_factory=NATSConfig)
    log_level: str = "info"