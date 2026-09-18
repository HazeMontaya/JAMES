"""JAMES Voice Pipeline - Local-First STT/TTS (AGPL-3.0)"""

from .audio import AudioConfig, AudioIO
from .config import VoiceConfig
from .pipeline import VoicePipeline
from .stt import STTEngine, WhisperModel
from .tts import KokoroVoice, TTSEngine

__version__ = "0.1.0"

__all__ = [
    "AudioConfig",
    "AudioIO",
    "KokoroVoice",
    "STTEngine",
    "TTSEngine",
    "VoiceConfig",
    "VoicePipeline",
    "WhisperModel",
]