from __future__ import annotations

import time
from dataclasses import dataclass

import cv2 as cv
import mss
import numpy as np

from d2bot.config import CaptureConfig, RecordingConfig


@dataclass(slots=True)
class FramePacket:
    frame: np.ndarray
    timestamp: float


class ScreenCapture:
    def __init__(self, config: CaptureConfig):
        self.config = config
        self._sct = mss.mss()
        self._target = self._resolve_target()

    def _resolve_target(self) -> dict[str, int]:
        if self.config.region:
            return self.config.region
        return dict(self._sct.monitors[self.config.monitor_index])

    @property
    def target(self) -> dict[str, int]:
        return self._target

    def grab(self) -> FramePacket:
        shot = np.array(self._sct.grab(self._target))
        frame = cv.cvtColor(shot, cv.COLOR_BGRA2BGR)
        return FramePacket(frame=frame, timestamp=time.time())


class SessionRecorder:
    def __init__(self, config: RecordingConfig, frame_size: tuple[int, int]):
        self.config = config
        self.frame_size = frame_size
        self._writer: cv.VideoWriter | None = None

    def start(self) -> None:
        if not self.config.enabled:
            return
        width, height = self.frame_size
        fourcc = cv.VideoWriter_fourcc(*self.config.codec)
        self._writer = cv.VideoWriter(
            self.config.output_path,
            fourcc,
            8.0,
            (width, height),
        )

    def write(self, frame: np.ndarray) -> None:
        if self._writer is not None:
            self._writer.write(frame)

    def close(self) -> None:
        if self._writer is not None:
            self._writer.release()
            self._writer = None
