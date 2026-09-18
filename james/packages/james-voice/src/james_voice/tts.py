"""TTS Engine - kokoro-onnx wrapper"""

import asyncio
from dataclasses import dataclass
from pathlib import Path
from typing import AsyncGenerator, ClassVar

import numpy as np
import structlog

try:
    from kokoro_onnx import Kokoro
except ImportError:
    Kokoro = None

from .config import TTSConfig

logger = structlog.get_logger()


@dataclass
class TTSResult:
    audio: np.ndarray  # float32, mono, 24kHz
    sample_rate: int
    duration: float
    text: str


class TTSEngine:
    """Local TTS using kokoro-onnx."""

    AVAILABLE_VOICES: ClassVar[list[str]] = [
        "af_heart", "af_bella", "af_nicole", "af_aoede", "af_kore",
        "am_adam", "am_michael", "am_fenrir", "am_puck", "am_echo",
        "bf_emma", "bf_isabella", "bm_george", "bm_lewis",
    ]

    def __init__(self, config: TTSConfig):
        self.config = config
        self._model: Kokoro | None = None
        self._loaded = False

    async def load(self) -> None:
        """Load the kokoro model."""
        if self._loaded:
            return

        if Kokoro is None:
            raise RuntimeError("kokoro-onnx not installed. Install with: uv pip install kokoro-onnx")

        model_path = self.config.model_path
        if model_path is None:
            # Try to find model in common locations
            for path in [
                Path.cwd() / "models" / "kokoro-v1.0.onnx",
                Path.home() / ".james" / "models" / "kokoro-v1.0.onnx",
                Path("/usr/local/share/kokoro/kokoro-v1.0.onnx"),
            ]:
                if path.exists():
                    model_path = path
                    break

        if model_path is None or not model_path.exists():
            raise FileNotFoundError(
                "kokoro model not found. Download from https://github.com/thewh1teagle/kokoro-onnx"
            )

        voices_path = model_path.parent / "voices-v1.0.bin"
        if not voices_path.exists():
            raise FileNotFoundError("kokoro voices file not found")

        logger.info("Loading kokoro TTS model", model=str(model_path), voice=self.config.voice)

        self._model = Kokoro(str(model_path), str(voices_path))
        self._loaded = True
        logger.info("kokoro TTS model loaded")

    def list_voices(self) -> list[str]:
        """Get available voices."""
        if not self._loaded:
            return self.AVAILABLE_VOICES
        assert self._model is not None
        return list(self._model.get_voices())

    def is_voice_available(self, voice: str) -> bool:
        """Check if a voice is available."""
        if not self._loaded:
            return voice in self.AVAILABLE_VOICES
        return voice in self.list_voices()

    async def synthesize(self, text: str, voice: str | None = None, speed: float | None = None) -> TTSResult:
        """Synthesize text to speech."""
        if not self._loaded:
            await self.load()

        assert self._model is not None

        voice = voice or self.config.voice
        speed = speed or self.config.speed

        if not self.is_voice_available(voice):
            raise ValueError(f"Voice '{voice}' not available. Available: {self.list_voices()}")

        logger.debug("Synthesizing TTS", text_len=len(text), voice=voice, speed=speed)

        # kokoro-onnx is synchronous, run in thread pool
        loop = asyncio.get_event_loop()
        audio, sample_rate = await loop.run_in_executor(
            None,
            lambda: self._model.create(text, voice=voice, speed=speed, lang="en-us"),
        )

        # audio is float32 numpy array, sample_rate is int
        audio = np.asarray(audio, dtype=np.float32)
        duration = len(audio) / sample_rate

        logger.debug("TTS synthesized", duration=duration, sample_rate=sample_rate)

        return TTSResult(
            audio=audio,
            sample_rate=sample_rate,
            duration=duration,
            text=text,
        )

    async def synthesize_stream(
        self,
        text: str,
        voice: str | None = None,
        speed: float | None = None,
        chunk_samples: int = 1024,
    ) -> AsyncGenerator[np.ndarray, None]:
        """Synthesize and stream audio chunks."""
        result = await self.synthesize(text, voice, speed)

        # Yield chunks
        audio = result.audio
        for i in range(0, len(audio), chunk_samples):
            chunk = audio[i:i + chunk_samples]
            if len(chunk) < chunk_samples:
                # Pad last chunk
                chunk = np.pad(chunk, (0, chunk_samples - len(chunk)))
            yield chunk
            await asyncio.sleep(0)  # yield control


class KokoroVoice:
    """Voice metadata."""
    pass