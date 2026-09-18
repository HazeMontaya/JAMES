# JAMES Hands

**Gesture Control & Computer Interaction** — Separate AGPL-3.0 process for physical-world interaction.

## Architecture

```
┌─────────────┐     NATS + gRPC     ┌──────────────────┐
│  Rust Core  │ ◄─────────────────► │  Python Hands    │
│  (james-core)│   hands.gesture     │  (james-hands)   │
│             │   hands.pose        │  - OpenCV        │
└─────────────┘   hands.action      │  - MediaPipe     │
                hands.feedback      │  - gRPC          │
                                        └──────────────────┘
```

## Features

- **Hand Tracking**: MediaPipe Hands (21 landmarks, 3D)
- **Pose Estimation**: MediaPipe Pose (33 landmarks)
- **Face Mesh**: MediaPipe Face Mesh (468 landmarks)
- **Gesture Recognition**: Custom classifier (point, swipe, pinch, grab, etc.)
- **Action Execution**: Map gestures → NATS commands / system actions
- **Feedback Loop**: Visual/audio/haptic feedback via NATS
- **Multi-camera**: Support for multiple USB/webcam inputs

## NATS Subjects

| Subject | Direction | Payload |
|---------|-----------|---------|
| `james.hands.gesture` | Hands → Core | Recognized gesture + confidence |
| `james.hands.pose` | Hands → Core | Body pose landmarks |
| `james.hands.face` | Hands → Core | Face mesh landmarks |
| `james.hands.action` | Core → Hands | Execute action (click, type, scroll) |
| `james.hands.config` | Core ↔ Hands | Camera, sensitivity, mapping |
| `james.hands.feedback` | Core → Hands | Visual/audio/haptic feedback |

## Quick Start

```bash
cd packages/james-hands
uv pip install -e .
james-hands
```

## Configuration

```yaml
hands:
  camera:
    device_id: 0
    width: 1280
    height: 720
    fps: 30
  detection:
    model_complexity: 1
    min_detection_confidence: 0.7
    min_tracking_confidence: 0.5
    max_hands: 2
  gestures:
    enabled: true
    sensitivity: 0.8
    custom_gestures: {}
  actions:
    click_threshold: 0.05
    swipe_threshold: 0.15
    pinch_threshold: 0.03
```

## License

AGPL-3.0-only