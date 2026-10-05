from __future__ import annotations

import json
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from queue import Empty, Full, Queue
from typing import Any

from diablo2.common.config import LoggingConfig


@dataclass
class AsyncLogRecord:
    level: str
    message: str
    priority: int = 1
    source: str = "app"
    timestamp: float = field(default_factory=time.time)
    payload: dict[str, Any] | None = None


class AsyncSegmentedLogger:
    def __init__(self, config: LoggingConfig, base_name: str = "north-go-tuning"):
        self.config = config
        self.base_name = base_name
        self.log_dir = Path(config.directory)
        self.queue: Queue[AsyncLogRecord] = Queue(maxsize=max(100, int(config.queue_max_size)))
        self._stop_event = threading.Event()
        self._thread: threading.Thread | None = None
        self._writer = None
        self._current_path: Path | None = None
        self._dropped_records = 0
        self._drop_notice_pending = False
        self._lock = threading.Lock()

    def start(self) -> None:
        with self._lock:
            if self._thread is not None:
                return
            self.log_dir.mkdir(parents=True, exist_ok=True)
            self._stop_event.clear()
            self._open_writer()
            self._thread = threading.Thread(target=self._run, name=f"{self.base_name}-logger", daemon=True)
            self._thread.start()

    def stop(self) -> None:
        self._stop_event.set()
        thread = self._thread
        if thread is not None:
            thread.join(timeout=3.0)
        with self._lock:
            self._thread = None
            if self._writer is not None:
                self._writer.flush()
                self._writer.close()
                self._writer = None

    def log(self, level: str, message: str, *, payload: dict[str, Any] | None = None, priority: int = 1, source: str = "app") -> None:
        record = AsyncLogRecord(level=level, message=message, payload=payload, priority=priority, source=source)
        try:
            self.queue.put_nowait(record)
        except Full:
            if priority <= 0:
                self._force_log(record)
                return
            self._dropped_records += 1
            self._drop_notice_pending = True

    def _force_log(self, record: AsyncLogRecord) -> None:
        with self._lock:
            if self._writer is None:
                self.log_dir.mkdir(parents=True, exist_ok=True)
                self._open_writer()
            self._write_record(record)
            self._writer.flush()

    def _run(self) -> None:
        flush_interval = max(0.1, float(self.config.flush_interval_seconds))
        next_flush_at = time.time() + flush_interval
        while not self._stop_event.is_set() or not self.queue.empty():
            try:
                record = self.queue.get(timeout=0.1)
            except Empty:
                record = None
            if record is not None:
                with self._lock:
                    self._write_record(record)
            if self._drop_notice_pending and not self.queue.full():
                self._drop_notice_pending = False
                dropped = self._dropped_records
                self._dropped_records = 0
                with self._lock:
                    self._write_record(
                        AsyncLogRecord(
                            level="warning",
                            message="Low-priority log records were dropped because the logging queue was full.",
                            priority=0,
                            source="logger",
                            payload={"dropped_records": dropped},
                        )
                    )
            if time.time() >= next_flush_at:
                with self._lock:
                    if self._writer is not None:
                        self._writer.flush()
                next_flush_at = time.time() + flush_interval
        with self._lock:
            if self._writer is not None:
                self._writer.flush()

    def _write_record(self, record: AsyncLogRecord) -> None:
        if self._writer is None:
            self._open_writer()
        self._rotate_if_needed()
        serialized = json.dumps(
            {
                "ts": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(record.timestamp)),
                "level": record.level.upper(),
                "source": record.source,
                "message": record.message,
                "payload": record.payload or {},
            },
            ensure_ascii=True,
        )
        self._writer.write(serialized + "\n")

    def _open_writer(self) -> None:
        self._current_path = self.log_dir / f"{self.base_name}.log"
        self._writer = self._current_path.open("a", encoding="utf-8")

    def _rotate_if_needed(self) -> None:
        if self._writer is None or self._current_path is None:
            return
        max_bytes = max(1, int(self.config.max_segment_size_mb)) * 1024 * 1024
        try:
            current_size = self._current_path.stat().st_size
        except FileNotFoundError:
            current_size = 0
        if current_size < max_bytes:
            return
        self._writer.flush()
        self._writer.close()
        retained = max(1, int(self.config.retained_segments))
        oldest_path = self.log_dir / f"{self.base_name}.{retained}.log"
        if oldest_path.exists():
            oldest_path.unlink()
        for index in range(retained - 1, 0, -1):
            source = self.log_dir / f"{self.base_name}.{index}.log"
            target = self.log_dir / f"{self.base_name}.{index + 1}.log"
            if source.exists():
                source.replace(target)
        self._current_path.replace(self.log_dir / f"{self.base_name}.1.log")
        self._open_writer()
