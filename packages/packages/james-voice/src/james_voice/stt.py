"""STT Engine - faster-whisper wrapper"""

from dataclasses import dataclass
from pathlib import Path
from typing import AsyncGenerator

import structlog
from faster_whisper import WhisperModel as FWWhisperModel

from .config import STTConfig

logger = structlog.get_logger()


@dataclass
class TranscriptionSegment:
    start: float
    end: float
    text: str
    probability: float
    no_speech_prob: float


@dataclass
class TranscriptionResult:
    text: str
    segments: list[TranscriptionSegment]
    language: str
    language_probability: float
    duration: float


class STTEngine:
    """Local STT using faster-whisper (CPU/GPU)."""

    def __init__(self, config: STTConfig):
        self.config = config
        self._model: FWWhisperModel | None = None
        self._loaded = False

    async def load(self) -> None:
        """Load the Whisper model."""
        if self._loaded:
            return

        device = self.config.device
        if device == "auto":
            try:
                import torch
                device = "cuda" if torch.cuda.is_available() else "cpu"
            except ImportError:
                device = "cpu"

        logger.info("Loading Whisper model",
                    model=self.config.model_size,
                    device=device,
                    compute_type=self.config.compute_type)

        self._model = FWWhisperModel(
            self.config.model_size,
            device=device,
            compute_type=self.config.compute_type,
            download_root=str(self.config.download_root) if self.config.download_root else None,
        )
        self._loaded = True
        logger.info("Whisper model loaded")

    async def transcribe(
        self,
        audio_path: Path,
        language: str | None = None,
    ) -> TranscriptionResult:
        """Transcribe an audio file."""
        if not self._loaded:
            await self.load()

        assert self._model is not None

        segments, info = self._model.transcribe(
            str(audio_path),
            language=language or self.config.language,
            beam_size=self.config.beam_size,
            vad_filter=self.config.vad_filter,
            vad_parameters=self.config.vad_parameters,
        )

        segs = []
        full_text = []
        for seg in segments:
            tseg = TranscriptionSegment(
                start=seg.start,
                end=seg.end,
                text=seg.text,
                probability=seg.avg_logprob,
                no_speech_prob=seg.no_speech_prob,
            )
            segs.append(tseg)
            full_text.append(seg.text)

        return TranscriptionResult(
            text=" ".join(full_text),
            segments=segs,
            language=info.language,
            language_probability=info.language_probability,
            duration=info.duration,
        )

    async def transcribe_stream(
        self,
        audio_chunks: AsyncGenerator[bytes, None],
        language: str | None = None,
    ) -> AsyncGenerator[TranscriptionSegment, None]:
        """Transcribe streaming audio chunks (VAD-based)."""
        if not self._loaded:
            await self.load()

        assert self._model is not None

        # Buffer for VAD
        from faster_whisper.vad import VadOptions

        vad_options = VadOptions(
            threshold=self.config.vad_parameters["threshold"],
            min_speech_duration_ms=self.config.vad_parameters["min_speech_duration_ms"],
            max_speech_duration_s=self.config.vad_parameters["max_speech_duration_s"],
            min_silence_duration_ms=self.config.vad_parameters["min_silence_duration_ms"],
        )

        # Collect chunks into buffer for transcription
        buffer = bytearray()
        sample_rate = 16000
        bytes_per_sample = 2  # int16
        chunk_samples = 1024
        bytes_per_chunk = chunk_samples * 2  # int16

        async for chunk in audio_chunks:
            buffer.extend(chunk)

            # Process when we have enough data (e.g., 1 second)
            if len(buffer) >= sample_rate * bytes_per_sample * 1:
                audio_data = bytes(buffer)
                buffer.clear()

                # Transcribe chunk
                import tempfile
                with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
                    import wave
                    with wave.open(f.name, "wb") as wf:
                        wf.setnchannels(1)
                        wf.setsampwidth(2)
                        wf.setframerate(16000)
                        wf.writeframes(audio_data)

                    segments, _ = self._model.transcribe(
                        f.name,
                        language=self.config.language,
                        beam_size=self.config.beam_size,
                        vad_filter=self.config.vad_filter,
                        vad_parameters=VadOptions(**self.config.vad_parameters).__dict__,
                    )

                    Path(f.name).unlink()

                    for seg in segments:
                        yield TranscriptionSegment(
                            start=seg.start,
                            end=seg.end,
                            text=seg.text,
                            probability=seg.avg_logprob,
                            no_speech_prob=seg.no_speech_prob,
                        )

        # Flush remaining buffer
        if buffer:
            audio_data = bytes(buffer)
            import tempfile
            with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
                import wave
                with wave.open(f.name, "wb") as wf:
                    wf.setnchannels(1)
                    wf.setsampwidth(2)
                    wf.setframerate(16000)
                    wf.writeframes(audio_data)

                segments, _ = self._model.transcribe(
                    f.name,
                    language=self.config.language,
                    beam_size=self.config.beam_size,
                    vad_filter=self.config.vad_filter,
                )

                Path(f.name).unlink()

                for seg in segments:
                    yield TranscriptionSegment(
                        start=seg.start,
                        end=seg.end,
                        text=seg.text,
                        probability=seg.avg_logprob,
                        no_speech_prob=seg.no_speech_prob,
                    )


class WhisperModel:
    """Alias for backward compatibility."""
    pass