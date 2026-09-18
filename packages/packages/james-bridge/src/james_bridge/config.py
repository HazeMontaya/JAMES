"""Bridge configuration for Python side"""

from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class BridgeConfig(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="JAMES_BRIDGE_",
        env_nested_delimiter="__",
    )

    # NATS
    nats_url: str = "nats://localhost:4222"
    service_name: str = "james-python"

    # Subjects
    subject_prefix: str = "james.bridge"
    capability_execute_subject: str = "james.bridge.capability.execute.>"
    capability_register_subject: str = "james.bridge.capability.register"
    capability_list_subject: str = "james.bridge.capability.list"
    health_subject: str = "james.bridge.health"

    # Behavior
    auto_register: bool = True
    request_timeout: float = 60.0
    health_interval: float = 30.0

    # Skills
    skills_path: Path = Field(
        default_factory=lambda: Path.home() / ".james" / "skills"
    )