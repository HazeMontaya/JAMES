"""Face Visualizer configuration"""

from pathlib import Path
from typing import Literal

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class WebConfig(BaseSettings):
    host: str = "0.0.0.0"
    port: int = 8080
    static_path: Path = Field(default_factory=lambda: Path(__file__).parent.parent / "static")
    template_path: Path = Field(default_factory=lambda: Path(__file__).parent.parent / "templates")
    cors_origins: list[str] = ["*"]


class NATSConfig(BaseSettings):
    url: str = "nats://localhost:4222"
    subject_prefix: str = "james.face"
    event_history: int = 1000


class VisualizerConfig(BaseSettings):
    update_interval_ms: int = 100
    max_events: int = 500
    enable_3d: bool = False
    theme: Literal["light", "dark", "auto"] = "dark"


class FaceConfig(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="JAMES_FACE_",
        env_nested_delimiter="__",
    )

    web: WebConfig = Field(default_factory=WebConfig)
    nats: NATSConfig = Field(default_factory=NATSConfig)
    visualizer: VisualizerConfig = Field(default_factory=VisualizerConfig)
    log_level: str = "info"