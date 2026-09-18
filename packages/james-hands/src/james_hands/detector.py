"""MediaPipe detectors for hands, pose, and face"""

from dataclasses import dataclass, field

import cv2
import mediapipe as mp
import numpy as np
import structlog

from .config import DetectionConfig

logger = structlog.get_logger()


@dataclass
class HandLandmarks:
    """21 hand landmarks in 3D."""
    landmarks: np.ndarray  # (21, 3) x, y, z normalized [0,1]
    handedness: str  # "Left" or "Right"
    confidence: float


@dataclass
class PoseLandmarks:
    """33 pose landmarks in 3D."""
    landmarks: np.ndarray  # (33, 4) x, y, z, visibility
    confidence: float


@dataclass
class FaceMeshLandmarks:
    """468 face mesh landmarks in 3D."""
    landmarks: np.ndarray  # (468, 3) x, y, z normalized
    confidence: float


@dataclass
class DetectionResult:
    """Combined detection results."""
    hands: list[HandLandmarks] = field(default_factory=list)
    pose: PoseLandmarks | None = None
    face: FaceMeshLandmarks | None = None
    timestamp: float = 0.0
    frame_shape: tuple = (0, 0)


class BaseDetector:
    """Base detector with MediaPipe initialization."""

    def __init__(self, config: DetectionConfig):
        self.config = config
        self._initialized = False

    def _init_mp(self):
        """Lazy MediaPipe import."""
        if not self._initialized:
            self.mp = mp
            self._initialized = True


class HandDetector(BaseDetector):
    """MediaPipe Hands detector."""

    def __init__(self, config: DetectionConfig):
        super().__init__(config)
        self._hands = None

    def initialize(self) -> None:
        self._init_mp()
        self._hands = self.mp.solutions.hands.Hands(
            static_image_mode=False,
            max_num_hands=self.config.max_hands,
            model_complexity=self.config.model_complexity,
            min_detection_confidence=self.config.min_detection_confidence,
            min_tracking_confidence=self.config.min_tracking_confidence,
        )

    def detect(self, frame: np.ndarray) -> list[HandLandmarks]:
        """Detect hands in frame."""
        if self._hands is None:
            self.initialize()

        # Convert BGR to RGB
        rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        results = self._hands.process(rgb)

        hands = []
        if results.multi_hand_landmarks:
            for i, hand_landmarks in enumerate(results.multi_hand_landmarks):
                # Get handedness
                handedness = "Unknown"
                if results.multi_handedness and i < len(results.multi_handedness):
                    handedness = results.multi_handedness[i].classification[0].label

                # Extract landmarks
                landmarks = np.array([
                    [lm.x, lm.y, lm.z] for lm in hand_landmarks.landmark
                ], dtype=np.float32)

                # Calculate average confidence
                confidence = np.mean([lm.visibility if hasattr(lm, 'visibility') else 1.0
                                    for lm in hand_landmarks.landmark])

                hands.append(HandLandmarks(
                    landmarks=landmarks,
                    handedness=handedness,
                    confidence=confidence,
                ))

        return hands

    def draw(self, frame: np.ndarray, hands: list[HandLandmarks]) -> np.ndarray:
        """Draw hand landmarks on frame."""
        if self._hands is None:
            return frame

        annotated = frame.copy()
        for hand in hands:
            # Draw landmarks
            for lm in hand.landmarks:
                h, w = frame.shape[:2]
                x, y = int(lm[0] * w), int(lm[1] * h)
                cv2.circle(annotated, (x, y), 3, (0, 255, 0), -1)

            # Draw connections
            connections = self.mp.solutions.hands.HAND_CONNECTIONS
            for conn in connections:
                p1 = hand.landmarks[conn[0]]
                p2 = hand.landmarks[conn[1]]
                h, w = frame.shape[:2]
                x1, y1 = int(p1[0] * w), int(p1[1] * h)
                x2, y2 = int(p2[0] * w), int(p2[1] * h)
                cv2.line(annotated, (x1, y1), (x2, y2), (255, 0, 0), 2)

        return annotated

    def close(self) -> None:
        if self._hands:
            self._hands.close()
            self._hands = None


class PoseDetector(BaseDetector):
    """MediaPipe Pose detector."""

    def __init__(self, config: DetectionConfig):
        super().__init__(config)
        self._pose = None

    def initialize(self) -> None:
        self._init_mp()
        self._pose = self.mp.solutions.pose.Pose(
            static_image_mode=False,
            model_complexity=self.config.model_complexity,
            min_detection_confidence=self.config.min_detection_confidence,
            min_tracking_confidence=self.config.min_tracking_confidence,
            enable_segmentation=self.config.enable_segmentation,
        )

    def detect(self, frame: np.ndarray) -> PoseLandmarks | None:
        if self._pose is None:
            self.initialize()

        rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        results = self._pose.process(rgb)

        if results.pose_landmarks:
            landmarks = np.array([
                [lm.x, lm.y, lm.z, lm.visibility]
                for lm in results.pose_landmarks.landmark
            ], dtype=np.float32)
            confidence = np.mean([lm.visibility for lm in results.pose_landmarks.landmark])
            return PoseLandmarks(landmarks=landmarks, confidence=confidence)

        return None

    def draw(self, frame: np.ndarray, pose: PoseLandmarks | None) -> np.ndarray:
        if pose is None or self._pose is None:
            return frame

        annotated = frame.copy()
        # Draw pose landmarks
        for lm in pose.landmarks:
            h, w = frame.shape[:2]
            x, y = int(lm[0] * w), int(lm[1] * h)
            if lm[3] > 0.5:  # visibility
                cv2.circle(annotated, (x, y), 4, (0, 255, 255), -1)

        # Draw connections
        connections = self.mp.solutions.pose.POSE_CONNECTIONS
        for conn in connections:
            p1 = pose.landmarks[conn[0]]
            p2 = pose.landmarks[conn[1]]
            if p1[3] > 0.5 and p2[3] > 0.5:
                h, w = frame.shape[:2]
                x1, y1 = int(p1[0] * w), int(p1[1] * h)
                x2, y2 = int(p2[0] * w), int(p2[1] * h)
                cv2.line(annotated, (x1, y1), (x2, y2), (255, 0, 255), 2)

        return annotated

    def close(self) -> None:
        if self._pose:
            self._pose.close()
            self._pose = None


class FaceMeshDetector(BaseDetector):
    """MediaPipe Face Mesh detector."""

    def __init__(self, config: DetectionConfig):
        super().__init__(config)
        self._face_mesh = None

    def initialize(self) -> None:
        self._init_mp()
        self._face_mesh = self.mp.solutions.face_mesh.FaceMesh(
            static_image_mode=False,
            max_num_faces=self.config.max_faces,
            refine_landmarks=True,
            min_detection_confidence=self.config.min_detection_confidence,
            min_tracking_confidence=self.config.min_tracking_confidence,
        )

    def detect(self, frame: np.ndarray) -> FaceMeshLandmarks | None:
        if self._face_mesh is None:
            self.initialize()

        rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        results = self._face_mesh.process(rgb)

        if results.multi_face_landmarks:
            face = results.multi_face_landmarks[0]
            landmarks = np.array([
                [lm.x, lm.y, lm.z] for lm in face.landmark
            ], dtype=np.float32)
            return FaceMeshLandmarks(landmarks=landmarks, confidence=1.0)

        return None

    def draw(self, frame: np.ndarray, face: FaceMeshLandmarks | None) -> np.ndarray:
        if face is None or self._face_mesh is None:
            return frame

        annotated = frame.copy()
        # Draw only key points for performance
        for i, lm in enumerate(face.landmarks):
            if i % 3 == 0:  # Subsample
                h, w = frame.shape[:2]
                x, y = int(lm[0] * w), int(lm[1] * h)
                cv2.circle(annotated, (x, y), 1, (255, 255, 0), -1)

        return annotated

    def close(self) -> None:
        if self._face_mesh:
            self._face_mesh.close()
            self._face_mesh = None