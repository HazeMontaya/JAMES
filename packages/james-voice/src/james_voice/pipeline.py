"""Voice Pipeline - Main orchestrator for STT/TTS with NATS integration"""

import asyncio
import base64
import json
import signal
from dataclasses import dataclass, field
from pathlib import Path

import nats
import numpy as np
import structlog
from nats.aio.client import Client as NatsClient
from nats.aio.msg import Msg

from .audio import AudioIO
from .config import VoiceConfig
from .stt import STTEngine
from .tts import TTSEngine

logger = structlog.get_logger()


@dataclass
class VoiceStatus:
    stt_loaded: bool = False
    tts_loaded: bool = False
    audio_active: bool = False
    current_voice: str = ""
    errors: list[str] = field(default_factory=list)


class VoicePipeline:
    """Main voice pipeline coordinating STT, TTS, Audio I/O, and NATS."""

    def __init__(self, config: VoiceConfig):
        self.config = config
        self.nc: NatsClient | None = None
        self.stt = STTEngine(config.stt)
        self.tts = TTSEngine(config.tts)
        self.audio = AudioIO(config.audio)
        self.status = VoiceStatus()
        self._running = False
        self._tasks: list[asyncio.Task] = []

    async def connect(self) -> None:
        """Connect to NATS."""
        self.nc = await nats.connect(self.config.nats_url)
        logger.info("Voice pipeline connected to NATS", url=self.config.nats_url)

    async def initialize(self) -> None:
        """Initialize STT and TTS models."""
        await self.connect()

        # Load models in parallel
        await asyncio.gather(
            self.stt.load(),
            self.tts.load(),
        )
        self.status.stt_loaded = True
        self.status.tts_loaded = True
        self.status.current_voice = self.config.tts.voice
        logger.info("Voice models loaded")

    async def start(self) -> None:
        """Start the voice pipeline (listen for NATS messages)."""
        self._running = True

        # Subscribe to subjects
        prefix = self.config.subject_prefix

        await self.nc.subscribe(f"{prefix}.text.in", cb=self._handle_tts_request)
        await self.nc.subscribe(f"{prefix}.audio.in", cb=self._handle_stt_request)
        await self.nc.subscribe(f"{prefix}.config", cb=self._handle_config)
        await self.nc.subscribe(f"{prefix}.control", cb=self._handle_control)

        # Start status publisher
        self._tasks.append(asyncio.create_task(self._publish_status()))

        logger.info("Voice pipeline started", prefix=prefix)

    async def stop(self) -> None:
        """Stop the pipeline."""
        self._running = False
        for task in self._tasks:
            task.cancel()
        await asyncio.gather(*self._tasks, return_exceptions=True)

        await self.audio.close()
        if self.nc:
            await self.nc.close()
        logger.info("Voice pipeline stopped")

    async def _handle_tts_request(self, msg: Msg) -> None:
        """Handle TTS request: text.in -> audio.out"""
        try:
            data = json.loads(msg.data.decode())
            text = data.get("text", "")
            voice = data.get("voice")
            speed = data.get("speed")
            request_id = data.get("request_id", "")

            logger.debug("TTS request", text_len=len(text), voice=voice, request_id=request_id)

            result = await self.tts.synthesize(text, voice=voice, speed=speed)

            # Send audio chunks
            audio_int16 = (result.audio * 32767).astype(np.int16)

            # Publish in chunks
            chunk_size = 1024
            for i in range(0, len(audio_int16), chunk_size):
                chunk = audio_int16[i:i + chunk_size]
                if len(chunk) < chunk_size:
                    chunk = np.pad(chunk, (0, chunk_size - len(chunk)))

                response = {
                    "request_id": request_id,
                    "audio": base64.b64encode(chunk.tobytes()).decode("ascii"),
                    "sample_rate": result.sample_rate,
                    "is_final": i + chunk_size >= len(audio_int16),
                }
                await self.nc.publish(
                    f"{self.config.subject_prefix}.audio.out",
                    json.dumps(response).encode()
                )

            logger.debug("TTS completed", request_id=request_id, duration=result.duration)

        except Exception as e:
            logger.error("TTS error", error=str(e))
            if msg.reply:
                await self.nc.publish(msg.reply, json.dumps({"error": str(e)}).encode())

    async def _handle_stt_request(self, msg: Msg) -> None:
        """Handle STT request: audio.in -> text.out"""
        try:
            data = json.loads(msg.data.decode())
            audio_b64 = data.get("audio", b"")

            # Accept base64 strings or raw bytes
            audio_bytes = base64.b64decode(audio_b64) if isinstance(audio_b64, str) else audio_b64
            request_id = data.get("request_id", "")
            language = data.get("language")

            logger.debug("STT request", audio_len=len(audio_bytes), request_id=request_id)

            # Save to temp file
            import tempfile
            with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
                import wave
                with wave.open(f.name, "wb") as wf:
                    wf.setnchannels(1)
                    wf.setsampwidth(2)
                    wf.setframerate(16000)
                    wf.writeframes(audio_bytes)

                result = await self.stt.transcribe(Path(f.name), language=language)

            Path(f.name).unlink()

            response = {
                "request_id": request_id,
                "text": result.text,
                "language": result.language,
                "segments": [
                    {"start": s.start, "end": s.end, "text": s.text}
                    for s in result.segments
                ],
            }

            await self.nc.publish(
                f"{self.config.subject_prefix}.text.out",
                json.dumps(response).encode()
            )

            logger.debug("STT completed", request_id=request_id, text=result.text[:50])

        except Exception as e:
            logger.error("STT error", error=str(e))
            if msg.reply:
                await self.nc.publish(msg.reply, json.dumps({"error": str(e)}).encode())

    async def _handle_config(self, msg: Msg) -> None:
        """Handle config updates."""
        try:
            data = json.loads(msg.data.decode())
            # Update configs dynamically
            if "voice" in data:
                self.config.tts.voice = data["voice"]
                self.status.current_voice = data["voice"]
            if "stt_model" in data:
                self.config.stt.model_size = data["stt_model"]
                self.status.stt_loaded = False
                await self.stt.load()
                self.status.stt_loaded = True
            logger.info("Config updated", data=data)
        except Exception as e:
            logger.error("Config error", error=str(e))

    async def _handle_control(self, msg: Msg) -> None:
        """Handle control commands."""
        try:
            data = json.loads(msg.data.decode())
            command = data.get("command")

            if command == "status":
                await self._publish_status()
            elif command == "reload_stt":
                self.status.stt_loaded = False
                await self.stt.load()
                self.status.stt_loaded = True
            elif command == "reload_tts":
                self.status.tts_loaded = False
                await self.tts.load()
                self.status.tts_loaded = True
            elif command == "list_voices":
                voices = self.tts.list_voices()
                await self.nc.publish(f"{self.config.subject_prefix}.voices", json.dumps({"voices": voices}).encode())

        except Exception as e:
            logger.error("Control error", error=str(e))

    async def _publish_status(self) -> None:
        """Periodically publish status."""
        while self._running:
            try:
                status_data = {
                    "stt_loaded": self.status.stt_loaded,
                    "tts_loaded": self.status.tts_loaded,
                    "audio_active": self.status.audio_active,
                    "current_voice": self.status.current_voice,
                    "errors": self.status.errors or [],
                }
                await self.nc.publish(
                    f"{self.config.subject_prefix}.status",
                    json.dumps(status_data).encode()
                )
                await asyncio.sleep(10)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error("Status publish error", error=str(e))
                await asyncio.sleep(10)


async def main() -> None:
    """Main entry point for james-voice."""
    import asyncio

    config = VoiceConfig()
    pipeline = VoicePipeline(config)

    # Setup signal handlers
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, lambda: asyncio.create_task(pipeline.stop()))

    try:
        await pipeline.initialize()
        await pipeline.start()
        logger.info("Voice pipeline running. Press Ctrl+C to stop.")

        # Keep running
        while pipeline._running:
            await asyncio.sleep(1)

    except KeyboardInterrupt:
        pass
    finally:
        await pipeline.stop()


if __name__ == "__main__":
    asyncio.run(main())