"""Voice pipeline configuration"""

from pathlib import Path
from typing import Literal

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class STTConfig(BaseSettings):
    model_size: Literal["tiny", "base", "small", "medium", "large-v1", "large-v2", "large-v3"] = "large-v3"
    device: Literal["auto", "cpu", "cuda"] = "auto"
    compute_type: Literal["float16", "int8", "int8_float16"] = "float16"
    download_root: Path | None = None
    language: str | None = None  # None = auto-detect
    beam_size: int = 5
    vad_filter: bool = True
    vad_parameters: dict = Field(default_factory=lambda: {
        "threshold": 0.5,
        "min_speech_duration_ms": 250,
        "max_speech_duration_s": 30,
        "min_silence_duration_ms": 500,
    })


class TTSConfig(BaseSettings):
    voice: str = "af_heart"  # kokoro voices: af_heart, af_bella, am_adam, etc.
    speed: float = 1.0
    sample_rate: int = 24000
    model_path: Path | None = None


class AudioConfig(BaseSettings):
    input_sample_rate: int = 16000
    output_sample_rate: int = 24000
    chunk_size: int = 1024
    channels: int = 1
    dtype: str = "int16"
    device_id: int | None = None


class VADConfig(BaseSettings):
    enabled: bool = True
    threshold: float = 0.5
    min_silence_ms: int = 500
    min_speech_ms: int = 250
    max_speech_s: int = 30


class VoiceConfig(BaseSettings):
    model_config = SettingsConfigDict(
        env_prefix="JAMES_VOICE_",
        env_nested_delimiter="__",
    )

    stt: STTConfig = Field(default_factory=STTConfig)
    tts: TTSConfig = Field(default_factory=TTSConfig)
    audio: AudioConfig = Field(default_factory=AudioConfig)
    vad: VADConfig = Field(default_factory=VADConfig)

    # NATS
    nats_url: str = "nats://localhost:4222"
    subject_prefix: str = "james.voice"

    # Runtime
    log_level: str = "info"