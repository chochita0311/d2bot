from __future__ import annotations

import threading
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from queue import Queue

from diablo2.common.capture import ScreenCapture, SessionRecorder
from diablo2.common.config import CaptureConfig, RecordingConfig


@dataclass
class RecorderEvent:
    level: str
    message: str


class RecordingSession:
    def __init__(
        self,
        capture_config: CaptureConfig,
        recordings_dir: str = "recordings",
    ):
        self.capture_config = capture_config
        self.recordings_dir = Path(recordings_dir)
        self.events: Queue[RecorderEvent] = Queue()
        self._thread: threading.Thread | None = None
        self._stop_event = threading.Event()
        self._is_running = False
        self._lock = threading.Lock()
        self._output_path: Path | None = None

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._is_running

    @property
    def output_path(self) -> Path | None:
        return self._output_path

    def start(self) -> Path:
        with self._lock:
            if self._is_running:
                raise RuntimeError("Recording is already running.")
            self._stop_event.clear()
            self._output_path = self._build_output_path()
            self._thread = threading.Thread(target=self._run, daemon=True)
            self._is_running = True
            self._thread.start()
            self.events.put(
                RecorderEvent("info", f"Recording started: {self._output_path.name}")
            )
            return self._output_path

    def stop(self) -> None:
        with self._lock:
            if not self._is_running:
                return
            self._stop_event.set()
            thread = self._thread

        if thread is not None:
            thread.join(timeout=3.0)

        with self._lock:
            self._thread = None
            self._is_running = False

        self.events.put(RecorderEvent("info", "Recording stopped."))

    def _build_output_path(self) -> Path:
        self.recordings_dir.mkdir(parents=True, exist_ok=True)
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        return self.recordings_dir / f"diablo2_{timestamp}.avi"

    def _run(self) -> None:
        recorder: SessionRecorder | None = None
        try:
            capture = ScreenCapture(self.capture_config)
            first_packet = capture.grab()
            height, width = first_packet.frame.shape[:2]
            recording_config = RecordingConfig(
                enabled=True,
                output_path=str(self._output_path),
                codec="XVID",
            )
            recorder = SessionRecorder(recording_config, (width, height))
            recorder.start()
            recorder.write(first_packet.frame)

            frame_delay = 1.0 / max(1, self.capture_config.fps)
            while not self._stop_event.is_set():
                started = time.time()
                packet = capture.grab()
                recorder.write(packet.frame)
                elapsed = time.time() - started
                time.sleep(max(0.0, frame_delay - elapsed))
        except Exception as exc:  # pragma: no cover
            self.events.put(RecorderEvent("error", f"Recording failed: {exc}"))
        finally:
            if recorder is not None:
                recorder.close()
            with self._lock:
                self._is_running = False
