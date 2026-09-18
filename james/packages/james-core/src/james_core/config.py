"""Configuration module for JAMES"""

from pathlib import Path

from pydantic import BaseModel, Field
from pydantic_settings import BaseSettings, SettingsConfigDict
from pydantic_settings.sources import PydanticBaseSettingsSource, YamlConfigSettingsSource


class ConfigPaths(BaseModel):
    root: Path = Path("~/.james").expanduser()
    config: Path = Path("~/.james/config.yaml").expanduser()
    credentials: Path = Path("~/.james/credentials").expanduser()
    tools: Path = Path("~/.james/tools").expanduser()
    runtimes: Path = Path("~/.james/runtimes").expanduser()
    models: Path = Path("~/.james/models").expanduser()
    cache: Path = Path("~/.james/cache").expanduser()
    logs: Path = Path("~/.james/logs").expanduser()
    memory: Path = Path("~/.james/memory").expanduser()
    browser: Path = Path("~/.james/browser").expanduser()
    sandboxes: Path = Path("~/.james/sandboxes").expanduser()
    vault: Path = Path("~/.james/vault").expanduser()
    skills: Path = Path("~/.james/skills").expanduser()


class ModelEndpoint(BaseModel):
    enabled: bool = False
    base_url: str = ""
    api_key: str = ""
    model: str = ""
    max_tokens: int = 4096
    timeout: float = 120.0
    priority: int = 0


class NatsConfig(BaseModel):
    url: str = "nats://localhost:4222"
    stream_name: str = "JAMES_EVENTS"
    enabled: bool = False


class JamesSettings(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="JAMES_",
        env_nested_delimiter="__",
        yaml_file="~/.james/config.yaml",
    )

    name: str = "JAMES"
    log_level: str = "info"
    debug: bool = False

    nats: NatsConfig = NatsConfig()
    paths: ConfigPaths = ConfigPaths()

    # Model providers
    vllm: ModelEndpoint = ModelEndpoint()
    llama_cpp: ModelEndpoint = ModelEndpoint()
    ollama: ModelEndpoint = ModelEndpoint(base_url="http://localhost:11434", model="llama3")
    openai: ModelEndpoint = ModelEndpoint(enabled=False)
    anthropic: ModelEndpoint = ModelEndpoint(enabled=False)

    # Defaults for goal engine
    default_skill_depth: int = 3
    max_parallel_agents: int = 5

    # Security
    require_approval_for: list[str] = Field(
        default_factory=lambda: ["send_email", "spend_money", "publish", "delete", "contract", "deploy"]
    )
    auto_approve_observe: bool = True

    @classmethod
    def settings_customise_sources(
        cls,
        settings_cls: type[BaseSettings],
        init_settings: PydanticBaseSettingsSource,
        env_settings: PydanticBaseSettingsSource,
        dotenv_settings: PydanticBaseSettingsSource,
        file_secret_settings: PydanticBaseSettingsSource,
    ) -> tuple[PydanticBaseSettingsSource, ...]:
        """Load YAML config (highest precedence after env) without warnings."""
        return (
            init_settings,
            env_settings,
            YamlConfigSettingsSource(settings_cls),
            dotenv_settings,
            file_secret_settings,
        )


class JamesConfig:
    def __init__(self, settings: JamesSettings | None = None):
        self._settings = settings or self._load_settings()
        self.ensure_paths()

    @staticmethod
    def _load_settings() -> JamesSettings:
        return JamesSettings()

    def ensure_paths(self) -> None:
        paths = self._settings.paths.model_dump().values()
        for path in paths:
            if isinstance(path, Path):
                path.mkdir(parents=True, exist_ok=True)

    @property
    def settings(self) -> JamesSettings:
        return self._settings

    @property
    def paths(self) -> ConfigPaths:
        return self._settings.paths

    def get_model_endpoints(self) -> dict[str, ModelEndpoint]:
        return {
            "vllm": self._settings.vllm,
            "llama_cpp": self._settings.llama_cpp,
            "ollama": self._settings.ollama,
            "openai": self._settings.openai,
            "anthropic": self._settings.anthropic,
        }

    def enabled_model_endpoints(self) -> dict[str, ModelEndpoint]:
        return {name: ep for name, ep in self.get_model_endpoints().items() if ep.enabled}

    def save(self) -> None:
        import yaml

        config_path = self._settings.paths.config
        config_path.parent.mkdir(parents=True, exist_ok=True)
        data = self._settings.model_dump()
        with config_path.open("w") as f:
            yaml.dump(data, f, default_flow_style=False, sort_keys=False)
