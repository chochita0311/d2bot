from __future__ import annotations

import threading
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from queue import Queue
from uuid import uuid4

from diablo2.common.config import BotConfig


@dataclass
class SummonerEvent:
    level: str
    message: str
    payload: dict[str, object] | None = None
    priority: int = 1


class SummonerGuiSession:
    """Tk 입력 없이 최신 Summoner 실행과 이벤트 전달을 담당한다."""

    def __init__(self, config: BotConfig):
        self.config = config
        self.events: Queue[SummonerEvent] = Queue()
        self._lock = threading.Lock()
        self._stop_event = threading.Event()
        self._stop_requested = False
        self._thread: threading.Thread | None = None
        self.output_path: Path | None = None
        self.result: str | None = None

    def update_config(self, config: BotConfig) -> None:
        with self._lock:
            if self._thread is not None and self._thread.is_alive():
                raise RuntimeError("Stop the Summoner run before changing its configuration.")
            self.config = config

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._thread is not None and self._thread.is_alive()

    def start(self, repeat_count: int | None = None, difficulty: str = "hell", character_key: str | None = None) -> Path:
        with self._lock:
            if self._thread is not None and self._thread.is_alive():
                raise RuntimeError("Summoner run is already running.")
            count = 1 if repeat_count is None else repeat_count
            if type(count) is not int or not 1 <= count <= 5:
                raise ValueError("Summoner Repeat Count must be from 1 to 5; leave it blank for 1 fresh north attempt.")
            if difficulty not in ("normal", "nightmare", "hell"):
                raise ValueError("Select a valid Summoner difficulty: normal, nightmare, or hell.")
            if character_key not in self.config.characters:
                raise ValueError("Select a configured character before starting the Summoner run.")

            self._stop_event.clear()
            self._stop_requested = False
            self.result = None
            run_id = datetime.now().strftime("%Y%m%d-%H%M%S") + "-" + uuid4().hex[:10]
            self.output_path = Path("recordings") / "summoner" / "evidence" / ("gui-" + run_id)
            self._thread = threading.Thread(
                target=self._run,
                args=(self.config, count, difficulty, character_key, self.output_path),
                name="summoner-gui",
                daemon=True,
            )
            self.events.put(
                SummonerEvent(
                    "info",
                    f"Summoner north run started: {count} fresh attempt(s), {difficulty}, character {character_key}. "
                    "The matching character must already be selected in the game. Press F10 or Stop Action to stop.",
                    {"status": "running", "output": str(self.output_path)},
                )
            )
            self._thread.start()
            return self.output_path

    def request_stop(self) -> None:
        with self._lock:
            self._stop_requested = True
            self._stop_event.set()

    def stop(self) -> None:
        self.request_stop()

    def close(self) -> None:
        self.request_stop()

    def _queue_log(self, message: str) -> None:
        self.events.put(SummonerEvent("info", str(message)))

    def _run(self, config: BotConfig, repeat_count: int, difficulty: str, character_key: str, output: Path) -> None:
        try:
            from diablo2.runs.summoner import main_run

            if self._stop_event.is_set():
                result = 2
            else:
                result = main_run.run(
                    config,
                    north_cycles=repeat_count,
                    live=True,
                    resume_arcane=False,
                    casts=1,
                    difficulty=difficulty,
                    output=output,
                    continuous=True,
                    field_seconds=60,
                    segments=4,
                    automatic=True,
                    stop_event=self._stop_event,
                    character_key=character_key,
                    engage_monsters=True,
                    automap="on",
                    emit=self._queue_log,
                )
            with self._lock:
                cancelled = self._stop_requested or self._stop_event.is_set()
            if cancelled:
                status, level = "stopped", "info"
                message = "Summoner run stopped; unfinished attempts do not count as success."
            elif result == 0:
                status, level = "complete", "info"
                message = "Summoner north attempts completed. Full Summoner acceptance remains separate."
            else:
                status, level = "incomplete", "warning"
                message = "Summoner run incomplete; review its evidence before retrying."
            self.result = status
            self.events.put(SummonerEvent(level, message, {"status": status, "exit_code": result, "output": str(output)}))
        except Exception as exc:
            self.result = "failed"
            self.events.put(SummonerEvent("error", f"Summoner run failed: {exc}", {"status": "failed", "output": str(output)}))
