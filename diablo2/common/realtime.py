from __future__ import annotations

import threading
import time
from collections import deque
from dataclasses import dataclass, replace
from typing import Any, Callable

from diablo2.common.capture import ScreenCapture
from diablo2.common.config import CaptureConfig
from diablo2.common.survival import _integer, _number


@dataclass(frozen=True)
class RuntimeFrame:
    sequence_id: int
    frame: Any
    captured_at: float
    target: dict[str, int]
    generation: int = 0
    wall_captured_at: float | None = None

    def __post_init__(self):
        _integer(self.sequence_id, "sequence_id")
        _integer(self.generation, "generation")
        _number(self.captured_at, "captured_at")


@dataclass(frozen=True)
class VisionSnapshot:
    channel: str
    payload: Any
    source_sequence_id: int
    source_captured_at: float
    produced_at: float
    source_generation: int = 0

    def __post_init__(self):
        _integer(self.source_sequence_id, "source_sequence_id")
        _integer(self.source_generation, "source_generation")
        _number(self.source_captured_at, "source_captured_at")
        _number(self.produced_at, "produced_at")


@dataclass(frozen=True)
class DecisionSnapshot:
    payload: Any
    produced_at: float
    source_sequence_id: int | None = None
    source_captured_at: float | None = None
    source_generation: int = 0

    def __post_init__(self):
        _number(self.produced_at, "produced_at")
        _integer(self.source_generation, "source_generation")
        if self.source_sequence_id is not None:
            _integer(self.source_sequence_id, "source_sequence_id")
        if self.source_captured_at is not None:
            _number(self.source_captured_at, "source_captured_at")


@dataclass(frozen=True)
class RuntimeSnapshot:
    sampled_at: float
    latest_frame: RuntimeFrame | None
    recent_frames: tuple[RuntimeFrame, ...]
    fast_vision: VisionSnapshot | None
    slow_vision: VisionSnapshot | None
    latest_decision: DecisionSnapshot | None
    generation: int = 0


class RealtimeRuntimeState:
    FRAME_BUFFER_SIZE = 5

    def __init__(self, *, clock=time.monotonic) -> None:
        self._lock = threading.Lock()
        self.clock = clock
        self._last_clock: float | None = None
        self._generation = 0
        self._last_frame_sequence = -1
        self._last_frame_time: float | None = None
        self._channel_sources = {"fast": (-1, 0.0), "slow": (-1, 0.0)}
        self._latest_frame: RuntimeFrame | None = None
        self._recent_frames: deque[RuntimeFrame] = deque(maxlen=self.FRAME_BUFFER_SIZE)
        self._fast_vision: VisionSnapshot | None = None
        self._slow_vision: VisionSnapshot | None = None
        self._latest_decision: DecisionSnapshot | None = None

    def _now(self) -> float:
        now = _number(self.clock(), "runtime clock")
        if self._last_clock is not None and now < self._last_clock:
            raise RuntimeError("runtime monotonic clock moved backwards")
        self._last_clock = now
        return now

    @property
    def generation(self) -> int:
        with self._lock:
            return self._generation

    def _invalidate(self) -> int:
        self._generation += 1
        self._latest_frame = None
        self._recent_frames.clear()
        self._fast_vision = self._slow_vision = self._latest_decision = None
        self._channel_sources = {"fast": (-1, 0.0), "slow": (-1, 0.0)}
        return self._generation

    def invalidate(self) -> int:
        with self._lock:
            return self._invalidate()

    def is_current_generation(self, snapshot: RuntimeSnapshot) -> bool:
        with self._lock:
            return snapshot.generation == self._generation and snapshot.latest_frame is not None and self._latest_frame is not None

    def publish_frame(self, frame: RuntimeFrame) -> bool:
        with self._lock:
            now = self._now()
            if (
                frame.generation != self._generation
                or frame.sequence_id <= self._last_frame_sequence
                or frame.captured_at > now
                or (self._last_frame_time is not None and frame.captured_at < self._last_frame_time)
            ):
                return False
            if self._latest_frame is not None and frame.target != self._latest_frame.target:
                frame = replace(frame, generation=self._invalidate())
            self._latest_frame = frame
            self._recent_frames.append(frame)
            self._last_frame_sequence, self._last_frame_time = frame.sequence_id, frame.captured_at
            return True

    def publish_vision(self, channel: str, snapshot: VisionSnapshot) -> bool:
        if channel not in self._channel_sources or snapshot.channel != channel:
            raise ValueError("matching fast/slow vision channel required")
        with self._lock:
            now = self._now()
            latest = self._latest_frame
            previous_sequence, previous_time = self._channel_sources[channel]
            if (
                latest is None
                or snapshot.source_generation != self._generation
                or not previous_sequence < snapshot.source_sequence_id <= latest.sequence_id
                or not previous_time <= snapshot.source_captured_at <= latest.captured_at
                or not snapshot.source_captured_at <= snapshot.produced_at <= now
            ):
                return False
            self._channel_sources[channel] = (snapshot.source_sequence_id, snapshot.source_captured_at)
            # 새 관찰이 없으면 과거 결과로 정상 동작을 이어가지 않는다.
            accepted = snapshot if snapshot.payload is not None else None
            if channel == "fast":
                self._fast_vision = accepted
            else:
                self._slow_vision = accepted
            return True

    def publish_decision(self, snapshot: DecisionSnapshot) -> bool:
        with self._lock:
            now = self._now()
            latest = self._latest_frame
            if (
                latest is None
                or snapshot.source_generation != self._generation
                or snapshot.source_sequence_id is None
                or snapshot.source_captured_at is None
                or not 0 <= snapshot.source_sequence_id <= latest.sequence_id
                or not 0 <= snapshot.source_captured_at <= latest.captured_at
                or not snapshot.source_captured_at <= snapshot.produced_at <= now
            ):
                return False
            previous = self._latest_decision
            if previous is not None and (
                snapshot.source_sequence_id < previous.source_sequence_id or snapshot.produced_at < previous.produced_at
            ):
                return False
            self._latest_decision = snapshot
            return True

    def latest_frame(self) -> RuntimeFrame | None:
        with self._lock:
            return self._latest_frame

    def snapshot(self) -> RuntimeSnapshot:
        with self._lock:
            return RuntimeSnapshot(
                sampled_at=self._now(),
                latest_frame=self._latest_frame,
                recent_frames=tuple(self._recent_frames),
                fast_vision=self._fast_vision,
                slow_vision=self._slow_vision,
                latest_decision=self._latest_decision,
                generation=self._generation,
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
        clock: Callable[[], float] = time.monotonic,
        capture_factory=ScreenCapture,
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
        self.clock, self.capture_factory = clock, capture_factory
        self.state = RealtimeRuntimeState(clock=clock)
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
        deadline = self.clock() + timeout
        for thread in self._threads:
            remaining = max(0.0, deadline - self.clock())
            thread.join(timeout=remaining)

    def invalidate_observations(self) -> int:
        return self.state.invalidate()

    def _capture_loop(self) -> None:
        capture = None
        try:
            capture = self.capture_factory(self.capture_config)
            sequence_id = 0
            effective_fps = self.capture_fps if self.capture_fps is not None else self.capture_config.fps
            frame_delay = 1.0 / max(1.0, effective_fps)
            while not self.stop_event.is_set():
                generation = self.state.generation
                loop_started_at = self.clock()
                packet = capture.grab()
                self.state.publish_frame(
                    RuntimeFrame(
                        sequence_id=sequence_id,
                        frame=packet.frame,
                        captured_at=loop_started_at,
                        target=dict(capture.target),
                        generation=generation,
                        wall_captured_at=packet.timestamp,
                    )
                )
                sequence_id += 1
                remaining = frame_delay - (self.clock() - loop_started_at)
                if remaining > 0:
                    self.stop_event.wait(remaining)
        except Exception as exc:  # pragma: no cover
            self._handle_error("capture", exc)
        finally:
            if capture is not None:
                capture.close()

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
                self.state.publish_vision(
                    channel,
                    VisionSnapshot(
                        channel=channel,
                        payload=payload,
                        source_sequence_id=latest_frame.sequence_id,
                        source_captured_at=latest_frame.captured_at,
                        produced_at=self.clock(),
                        source_generation=latest_frame.generation,
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
                source = self.state.snapshot()
                payload = self.decision_fn(source)
                if payload is not None:
                    frame = source.latest_frame
                    self.state.publish_decision(
                        DecisionSnapshot(
                            payload=payload,
                            produced_at=self.clock(),
                            source_sequence_id=frame.sequence_id if frame is not None else None,
                            source_captured_at=frame.captured_at if frame is not None else None,
                            source_generation=source.generation,
                        )
                    )
                if self.decision_interval > 0:
                    self.stop_event.wait(self.decision_interval)
        except Exception as exc:  # pragma: no cover
            self._handle_error("decision", exc)

    def _handle_error(self, stage: str, exc: Exception) -> None:
        self.stop_event.set()
        if self.error_handler is not None:
            self.error_handler(stage, exc)
