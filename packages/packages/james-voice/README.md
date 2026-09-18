# JAMES Voice Pipeline

**Local-First STT/TTS Pipeline** — Separate AGPL-3.0 process communicating via NATS.

## Architecture

```
┌─────────────┐     NATS JetStream     ┌──────────────────┐
│  Rust Core  │ ◄────────────────────► │  Python Voice    │
│  (james-core)│   voice.audio.in       │  (james-voice)   │
│             │   voice.audio.out      │  - faster-whisper│
└─────────────┘   voice.text.in        │  - kokoro-onnx   │
                voice.text.out         └──────────────────┘
```

## Features

- **STT**: faster-whisper (local, CPU/GPU, multiple model sizes)
- **TTS**: kokoro-onnx (local, multiple voices, streaming)
- **Audio I/O**: sounddevice (cross-platform, low latency)
- **VAD**: Built-in voice activity detection
- **Streaming**: Real-time audio chunks via NATS
- **Fallback**: Cloud APIs (ElevenLabs, OpenAI) optional

## NATS Subjects

| Subject | Direction | Payload |
|---------|-----------|---------|
| `james.voice.audio.in` | Core → Voice | Raw audio chunks (PCM16, 16kHz) |
| `james.voice.audio.out` | Voice → Core | TTS audio chunks (PCM16, 24kHz) |
| `james.voice.text.in` | Core → Voice | Text to synthesize |
| `james.voice.text.out` | Voice → Core | Transcribed text |
| `james.voice.config` | Core → Voice | Model selection, voice config |
| `james.voice.status` | Voice → Core | Health, model loaded, errors |

## Quick Start

```bash
# Install
cd packages/james-voice
uv pip install -e .

# Run (requires NATS on localhost:4222)
james-voice
```

## Configuration

```yaml
# ~/.james/config.yaml
voice:
  stt:
    model: "large-v3"  # tiny, base, small, medium, large-v3
    device: "auto"     # cpu, cuda, auto
    compute_type: "float16"
  tts:
    voice: "af_heart"  # kokoro voices
    speed: 1.0
  audio:
    sample_rate: 16000
    chunk_size: 1024
  vad:
    threshold: 0.5
    min_silence_ms: 500
```

## License

AGPL-3.0-only — This package must run as a separate process. Source code must be shared with users.