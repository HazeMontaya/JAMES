"""Integration tests for Voice pipelines (mocked NATS + engines, no hardware)."""

import base64
import json
from typing import Any, Callable

import numpy as np
import pytest
from james_voice.config import VoiceConfig
from james_voice.pipeline import VoicePipeline
from james_voice.stt import TranscriptionResult, TranscriptionSegment
from james_voice.tts import TTSResult


class FakeNats:
    """Minimal fake NATS client capturing subscriptions and publications."""

    def __init__(self) -> None:
        self.published: list[tuple[str, bytes]] = []
        self.subscriptions: list[tuple[str, Callable]] = []
        self.closed = False

    async def publish(self, subject: str, data: bytes) -> None:
        self.published.append((subject, data))

    async def subscribe(self, subject: str, cb: Callable, **_: Any) -> None:
        self.subscriptions.append((subject, cb))

    async def close(self) -> None:
        self.closed = True


@pytest.fixture
def voice_pipeline() -> VoicePipeline:
    pipeline = VoicePipeline(VoiceConfig(nats_url="nats://localhost:4222"))
    pipeline.nc = FakeNats()
    return pipeline


@pytest.mark.asyncio
async def test_voice_config_env_loading(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("JAMES_VOICE_STT__MODEL_SIZE", "base")
    monkeypatch.setenv("JAMES_VOICE_TTS__VOICE", "af_bella")
    config = VoiceConfig()
    assert config.stt.model_size == "base"
    assert config.tts.voice == "af_bella"


@pytest.mark.asyncio
async def test_voice_pipeline_constructs() -> None:
    pipeline = VoicePipeline(VoiceConfig())
    assert pipeline.status.stt_loaded is False
    assert pipeline.status.errors == []


@pytest.mark.asyncio
async def test_handle_tts_request_publishes_base64_audio(voice_pipeline: VoicePipeline) -> None:
    import james_voice.pipeline as pipeline_mod
    from james_voice.pipeline import Msg

    async def fake_synthesize(
        self, text: str, voice: str | None = None, speed: float | None = None
    ) -> TTSResult:
        audio = np.zeros(4096, dtype=np.float32)
        return TTSResult(audio=audio, sample_rate=24000, duration=4096 / 24000, text=text)

    pipeline_mod.TTSEngine.synthesize = fake_synthesize  # type: ignore[method-assign]

    raw = json.dumps({"text": "hello world", "request_id": "abc", "voice": "af_heart", "speed": 1.0}).encode()
    msg = Msg(None, subject="james.voice.text.in", data=raw)

    await voice_pipeline._handle_tts_request(msg)

    assert voice_pipeline.nc is not None
    published = voice_pipeline.nc.published
    assert len(published) == 4  # 4096/1024 chunks
    for subject, data in published:
        assert subject == "james.voice.audio.out"
        payload = json.loads(data.decode())
        assert payload["request_id"] == "abc"
        assert payload["sample_rate"] == 24000
        # audio is base64 encoded bytes
        audio = base64.b64decode(payload["audio"])
        assert len(audio) == 1024 * 2  # int16 chunks


@pytest.mark.asyncio
async def test_handle_stt_request_publishes_text(voice_pipeline: VoicePipeline) -> None:
    import james_voice.pipeline as pipeline_mod

    async def fake_transcribe(self, audio_path, language: str | None = None) -> TranscriptionResult:
        return TranscriptionResult(
            text="hello world",
            segments=[TranscriptionSegment(start=0.0, end=0.5, text="hello world", probability=0.9, no_speech_prob=0.1)],
            language="en",
            language_probability=1.0,
            duration=0.5,
        )

    pipeline_mod.STTEngine.transcribe = fake_transcribe  # type: ignore[method-assign]

    import io
    import wave
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(16000)
        wf.writeframes(b"\x00\x00" * 8000)
    audio_b64 = base64.b64encode(buf.getvalue()).decode("ascii")

    raw = json.dumps({"audio": audio_b64, "request_id": "xyz", "language": "en"}).encode()
    from james_voice.pipeline import Msg
    msg = Msg(None, subject="james.voice.audio.in", data=raw)

    await voice_pipeline._handle_stt_request(msg)

    assert voice_pipeline.nc is not None
    published = voice_pipeline.nc.published
    assert len(published) == 1
    payload = json.loads(published[0][1].decode())
    assert payload["request_id"] == "xyz"
    assert payload["text"] == "hello world"
    assert payload["segments"][0]["start"] == 0.0


@pytest.mark.asyncio
async def test_handle_tts_error_has_reply(voice_pipeline: VoicePipeline) -> None:
    import james_voice.pipeline as pipeline_mod

    async def failing_synthesize(
        self, text: str, voice: str | None = None, speed: float | None = None
    ) -> TTSResult:
        raise RuntimeError("voice unavailable")

    pipeline_mod.TTSEngine.synthesize = failing_synthesize  # type: ignore[method-assign]

    raw = json.dumps({"text": "hi"}).encode()
    from james_voice.pipeline import Msg
    msg = Msg(None, subject="james.voice.text.in", reply="james.voice.error", data=raw)

    await voice_pipeline._handle_tts_request(msg)

    assert voice_pipeline.nc is not None
    assert voice_pipeline.nc.published
    payload = json.loads(voice_pipeline.nc.published[-1][1].decode())
    assert "error" in payload