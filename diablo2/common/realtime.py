from __future__ import annotations

import threading
import time
from collections import deque
from dataclasses import dataclass
from typing import Any, Callable

from diablo2.common.capture import ScreenCapture
from diablo2.common.config import CaptureConfig


@dataclass(frozen=True)
class RuntimeFrame:
    sequence_id: int
    frame: Any
    captured_at: float
    target: dict[str, int]


@dataclass(frozen=True)
class VisionSnapshot:
    channel: str
    payload: Any
    source_sequence_id: int
    source_captured_at: float
    produced_at: float


@dataclass(frozen=True)
class DecisionSnapshot:
    payload: Any
    produced_at: float


@dataclass(frozen=True)
class RuntimeSnapshot:
    sampled_at: float
    latest_frame: RuntimeFrame | None
    recent_frames: tuple[RuntimeFrame, ...]
    fast_vision: VisionSnapshot | None
    slow_vision: VisionSnapshot | None
    latest_decision: DecisionSnapshot | None


class RealtimeRuntimeState:
    FRAME_BUFFER_SIZE = 5

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._latest_frame: RuntimeFrame | None = None
        self._recent_frames: deque[RuntimeFrame] = deque(maxlen=self.FRAME_BUFFER_SIZE)
        self._fast_vision: VisionSnapshot | None = None
        self._slow_vision: VisionSnapshot | None = None
        self._latest_decision: DecisionSnapshot | None = None

    def publish_frame(self, frame: RuntimeFrame) -> None:
        with self._lock:
            self._latest_frame = frame
            self._recent_frames.append(frame)

    def publish_vision(self, channel: str, snapshot: VisionSnapshot) -> None:
        with self._lock:
            if channel == "fast":
                self._fast_vision = snapshot
            else:
                self._slow_vision = snapshot

    def publish_decision(self, snapshot: DecisionSnapshot) -> None:
        with self._lock:
            self._latest_decision = snapshot

    def latest_frame(self) -> RuntimeFrame | None:
        with self._lock:
            return self._latest_frame

    def snapshot(self) -> RuntimeSnapshot:
        with self._lock:
            return RuntimeSnapshot(
                sampled_at=time.time(),
                latest_frame=self._latest_frame,
                recent_frames=tuple(self._recent_frames),
                fast_vision=self._fast_vision,
                slow_vision=self._slow_vision,
                latest_decision=self._latest_decision,
            )


class RealtimeVisionRuntime:
    def __init__(
        self,
        capture_config: CaptureConfig,
        stop_event: threading.Event,
        fast_vision_fn: Callable[[RuntimeFrame], Any],
        slow_vision_fn: Callable[[RuntimeFrame], Any],
        decision_fn: Callable[[RuntimeSnapshot], Any],
        *,
        fast_interval: float = 0.01,
        slow_interval: float = 0.04,
        decision_interval: float = 0.01,
        idle_sleep: float = 0.005,
        capture_fps: float | None = None,
        error_handler: Callable[[str, Exception], None] | None = None,
    ) -> None:
        self.capture_config = capture_config
        self.stop_event = stop_event
        self.fast_vision_fn = fast_vision_fn
        self.slow_vision_fn = slow_vision_fn
        self.decision_fn = decision_fn
        self.fast_interval = fast_interval
        self.slow_interval = slow_interval
        self.decision_interval = decision_interval
        self.idle_sleep = idle_sleep
        self.capture_fps = capture_fps
        self.error_handler = error_handler
        self.state = RealtimeRuntimeState()
        self._threads: list[threading.Thread] = []

    def start(self) -> None:
        self._threads = [
            threading.Thread(target=self._capture_loop, name="d2-capture", daemon=True),
            threading.Thread(
                target=self._vision_loop, args=("fast", self.fast_vision_fn, self.fast_interval), name="d2-fast-vision", daemon=True
            ),
            threading.Thread(
                target=self._vision_loop, args=("slow", self.slow_vision_fn, self.slow_interval), name="d2-slow-vision", daemon=True
            ),
            threading.Thread(target=self._decision_loop, name="d2-decision", daemon=True),
        ]
        for thread in self._threads:
            thread.start()

    def stop(self, timeout: float = 3.0) -> None:
        self.stop_event.set()
        deadline = time.time() + timeout
        for thread in self._threads:
            remaining = max(0.0, deadline - time.time())
            thread.join(timeout=remaining)

    def _capture_loop(self) -> None:
        try:
            capture = ScreenCapture(self.capture_config)
            sequence_id = 0
            effective_fps = self.capture_fps if self.capture_fps is not None else self.capture_config.fps
            frame_delay = 1.0 / max(1.0, effective_fps)
            while not self.stop_event.is_set():
                loop_started_at = time.time()
                packet = capture.grab()
                self.state.publish_frame(
                    RuntimeFrame(
                        sequence_id=sequence_id,
                        frame=packet.frame,
                        captured_at=packet.timestamp,
                        target=dict(capture.target),
                    )
                )
                sequence_id += 1
                remaining = frame_delay - (time.time() - loop_started_at)
                if remaining > 0:
                    self.stop_event.wait(remaining)
        except Exception as exc:  # pragma: no cover
            self._handle_error("capture", exc)

    def _vision_loop(self, channel: str, processor: Callable[[RuntimeFrame], Any], interval: float) -> None:
        last_sequence_id = -1
        try:
            while not self.stop_event.is_set():
                latest_frame = self.state.latest_frame()
                if latest_frame is None or latest_frame.sequence_id == last_sequence_id:
                    self.stop_event.wait(self.idle_sleep)
                    continue
                payload = processor(latest_frame)
                last_sequence_id = latest_frame.sequence_id
                if payload is not None:
                    self.state.publish_vision(
                        channel,
                        VisionSnapshot(
                            channel=channel,
                            payload=payload,
                            source_sequence_id=latest_frame.sequence_id,
                            source_captured_at=latest_frame.captured_at,
                            produced_at=time.time(),
                        ),
                    )

                newest_frame = self.state.latest_frame()
                if newest_frame is not None and newest_frame.sequence_id != last_sequence_id:
                    continue
                if interval > 0:
                    self.stop_event.wait(interval)
        except Exception as exc:  # pragma: no cover
            self._handle_error(channel, exc)

    def _decision_loop(self) -> None:
        try:
            while not self.stop_event.is_set():
                payload = self.decision_fn(self.state.snapshot())
                if payload is not None:
                    self.state.publish_decision(DecisionSnapshot(payload=payload, produced_at=time.time()))
                if self.decision_interval > 0:
                    self.stop_event.wait(self.decision_interval)
        except Exception as exc:  # pragma: no cover
            self._handle_error("decision", exc)

    def _handle_error(self, stage: str, exc: Exception) -> None:
        self.stop_event.set()
        if self.error_handler is not None:
            self.error_handler(stage, exc)
