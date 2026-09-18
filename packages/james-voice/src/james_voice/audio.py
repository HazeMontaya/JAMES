"""Audio I/O - sounddevice wrapper for cross-platform audio capture/playback"""

import asyncio
from dataclasses import dataclass
from typing import AsyncGenerator

import numpy as np
import sounddevice as sd
import structlog

from .config import AudioConfig

logger = structlog.get_logger()


@dataclass
class AudioChunk:
    data: np.ndarray  # int16 or float32
    sample_rate: int
    timestamp: float


class AudioIO:
    """Cross-platform audio input/output using sounddevice."""

    def __init__(self, config: AudioConfig):
        self.config = config
        self._input_stream: sd.InputStream | None = None
        self._output_stream: sd.OutputStream | None = None
        self._input_queue: asyncio.Queue = asyncio.Queue(maxsize=100)
        self._running = False

    async def start_input(self) -> None:
        """Start audio capture."""
        if self._running:
            return

        self._running = True

        def callback(indata, frames, time_info, status):
            if status:
                logger.warning("Audio input status", status=str(status))
            # Put chunk in queue (non-blocking)
            import contextlib
            with contextlib.suppress(asyncio.QueueFull):
                self._input_queue.put_nowait(indata.copy())

        self._input_stream = sd.InputStream(
            samplerate=self.config.input_sample_rate,
            channels=self.config.channels,
            dtype=self.config.dtype,
            blocksize=self.config.chunk_size,
            device=self.config.device_id,
            callback=callback,
        )
        self._input_stream.start()
        logger.info("Audio input started", sample_rate=self.config.input_sample_rate)

    async def stop_input(self) -> None:
        """Stop audio capture."""
        self._running = False
        if self._input_stream:
            self._input_stream.stop()
            self._input_stream.close()
            self._input_stream = None
        logger.info("Audio input stopped")

    async def read_chunks(self) -> AsyncGenerator[np.ndarray, None]:
        """Read audio chunks from input stream."""
        if not self._running:
            await self.start_input()

        while self._running:
            try:
                chunk = await asyncio.wait_for(self._input_queue.get(), timeout=1.0)
                yield chunk
            except TimeoutError:
                continue

    async def play(self, audio: np.ndarray, sample_rate: int) -> None:
        """Play audio array (blocking)."""
        # Ensure correct dtype
        if audio.dtype != np.int16:
            if audio.dtype == np.float32:
                audio = (audio * 32767).astype(np.int16)
            else:
                audio = audio.astype(np.int16)

        # Resample if needed
        if sample_rate != self.config.output_sample_rate:
            import scipy.signal
            num_samples = int(len(audio) * self.config.output_sample_rate / sample_rate)
            audio = scipy.signal.resample(audio, num_samples)

        sd.play(audio, self.config.output_sample_rate, blocking=True)

    async def play_stream(self, audio_chunks: AsyncGenerator[np.ndarray, None]) -> None:
        """Stream audio chunks to output."""
        self._output_stream = sd.OutputStream(
            samplerate=self.config.output_sample_rate,
            channels=self.config.channels,
            dtype="int16",
            blocksize=self.config.chunk_size,
            device=self.config.device_id,
        )
        self._output_stream.start()

        try:
            async for chunk in audio_chunks:
                if chunk.dtype != np.int16:
                    if chunk.dtype == np.float32:
                        chunk = (chunk * 32767).astype(np.int16)
                    else:
                        chunk = chunk.astype(np.int16)

                # Pad if needed
                if len(chunk) < self.config.chunk_size:
                    chunk = np.pad(chunk, (0, self.config.chunk_size - len(chunk)))

                self._output_stream.write(chunk)
        finally:
            if self._output_stream:
                self._output_stream.stop()
                self._output_stream.close()
                self._output_stream = None

    async def close(self) -> None:
        """Close all streams."""
        await self.stop_input()
        if self._output_stream:
            self._output_stream.stop()
            self._output_stream.close()
            self._output_stream = None
        logger.info("Audio I/O closed")


@dataclass
class AudioConfig:
    input_sample_rate: int = 16000
    output_sample_rate: int = 24000
    chunk_size: int = 1024
    channels: int = 1
    dtype: str = "int16"
    device_id: int | None = None