from __future__ import annotations

import random
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from queue import Queue

import cv2 as cv
import numpy as np

from diablo2.common.capture import ScreenCapture, focus_window, resolve_window_from_config
from diablo2.common.config import CaptureConfig
from diablo2.common.controller import keyboard, pydirectinput


@dataclass
class LifecycleEvent:
    level: str
    message: str


@dataclass
class MatchResult:
    top_left: tuple[int, int]
    width: int
    height: int
    score: float


class RunLifecycleSession:
    STOP_HOTKEY = "f10"

    CHARACTER_SELECT_TEMPLATE_PATH = Path("assets/room/character_select.png")
    DIFFICULTY_TEMPLATE_PATH = Path("assets/room/difficulty/difficulty_select.png")
    DIFFICULTY_BUTTON_TEMPLATE_PATHS = {
        "normal": Path("assets/room/difficulty/normal.png"),
        "nightmare": Path("assets/room/difficulty/nightmare.png"),
        "hell": Path("assets/room/difficulty/hell.png"),
    }
    LOADING_TEMPLATE_PATH = Path("assets/room/loading.png")
    EXIT_TEMPLATE_PATH = Path("assets/room/exit.png")

    CHARACTER_SELECT_THRESHOLD = 0.86
    DIFFICULTY_THRESHOLD = 0.88
    LOADING_THRESHOLD = 0.9
    EXIT_THRESHOLD = 0.88

    PLAY_POINT = (808, 1010)
    SAVE_AND_EXIT_POINT = (959, 520)

    USER_INTERRUPT_DISTANCE = 80
    TARGET_JITTER = 3
    MOVE_STEPS = (2, 4)
    MOVE_SLEEP = (0.015, 0.035)
    CLICK_SETTLE = (0.05, 0.09)
    ACTION_SLEEP = (0.16, 0.3)
    FOCUS_SLEEP = (0.12, 0.2)

    def __init__(self, capture_config: CaptureConfig):
        self.capture_config = capture_config
        self.events: Queue[LifecycleEvent] = Queue()
        self._thread: threading.Thread | None = None
        self._stop_event = threading.Event()
        self._lock = threading.Lock()
        self._is_running = False
        self._repeat_count: int | None = None
        self._difficulty = "hell"
        self._hotkey_handle = None
        self._last_pointer: tuple[int, int] | None = None

        self._character_select_template = self._load_image(self.CHARACTER_SELECT_TEMPLATE_PATH)
        self._difficulty_template = self._load_image(self.DIFFICULTY_TEMPLATE_PATH)
        self._difficulty_button_templates = {
            name: self._load_image(path) for name, path in self.DIFFICULTY_BUTTON_TEMPLATE_PATHS.items()
        }
        self._loading_template = self._load_image(self.LOADING_TEMPLATE_PATH)
        self._exit_template = self._load_image(self.EXIT_TEMPLATE_PATH)

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._is_running

    def start(self, repeat_count: int | None, difficulty: str = "hell") -> None:
        with self._lock:
            if self._is_running:
                raise RuntimeError("Run lifecycle is already running.")
            self._repeat_count = repeat_count
            self._difficulty = difficulty if difficulty in self.DIFFICULTY_BUTTON_TEMPLATE_PATHS else "hell"
            self._stop_event.clear()
            self._thread = threading.Thread(target=self._run, daemon=True)
            self._is_running = True
            self._thread.start()
        loop_text = "until stopped" if repeat_count is None else f"for {repeat_count} run(s)"
        difficulty_label = {"normal": "Normal", "nightmare": "Nightmare", "hell": "Hell"}[self._difficulty]
        self.events.put(LifecycleEvent("info", f"Run lifecycle started {loop_text} on {difficulty_label}. Press {self.STOP_HOTKEY.upper()} to stop."))

    def stop(self) -> None:
        self._stop_event.set()
        thread = self._thread
        if thread is not None:
            thread.join(timeout=3.0)
        with self._lock:
            self._thread = None
            self._is_running = False
        self.events.put(LifecycleEvent("info", "Run lifecycle stopped."))

    def request_stop(self) -> None:
        self._stop_event.set()

    def _run(self) -> None:
        try:
            if pydirectinput is None:
                raise RuntimeError("pydirectinput is required for run lifecycle actions.")

            capture = ScreenCapture(self.capture_config)
            self._bind_hotkey()
            self._focus_game_window(capture)

            completed = 0
            while not self._stop_event.is_set():
                if self._repeat_count is not None and completed >= self._repeat_count:
                    self.events.put(LifecycleEvent("info", f"Completed {completed} lifecycle run(s)."))
                    return

                run_number = completed + 1
                self.create_room(capture, run_number)
                self.exit_room(capture, run_number)
                completed += 1
                self.events.put(LifecycleEvent("info", f"Lifecycle run {run_number}: exited room and returned to character select."))
        except Exception as exc:  # pragma: no cover
            self.events.put(LifecycleEvent("error", f"Run lifecycle failed: {exc}"))
        finally:
            self._unbind_hotkey()
            with self._lock:
                self._is_running = False

    def create_room(self, capture: ScreenCapture, run_number: int) -> None:
        self.events.put(LifecycleEvent("info", f"Lifecycle run {run_number}: creating room."))
        self._wait_for_template(capture, self._character_select_template, self.CHARACTER_SELECT_THRESHOLD, 20.0, "character select")
        self._click_relative(capture, self.PLAY_POINT)

        self._wait_for_template(capture, self._difficulty_template, self.DIFFICULTY_THRESHOLD, 10.0, "difficulty select")
        difficulty_match = self._wait_for_template(
            capture,
            self._difficulty_button_templates[self._difficulty],
            self.DIFFICULTY_THRESHOLD,
            10.0,
            f"{self._difficulty} difficulty button",
        )
        self._click_match_center(capture, difficulty_match)

        self._wait_for_template(capture, self._loading_template, self.LOADING_THRESHOLD, 10.0, "loading screen")
        self._wait_until_template_missing(capture, self._loading_template, self.LOADING_THRESHOLD, 20.0, "loading screen")
        self._sleep_range(0.8, 1.2)
        self.events.put(LifecycleEvent("info", f"Lifecycle run {run_number}: room created."))

    def exit_room(self, capture: ScreenCapture, run_number: int) -> None:
        self.events.put(LifecycleEvent("info", f"Lifecycle run {run_number}: exiting room."))
        self._press_key("esc")
        self._wait_for_template(capture, self._exit_template, self.EXIT_THRESHOLD, 8.0, "exit menu")
        self._click_relative(capture, self.SAVE_AND_EXIT_POINT)
        self._wait_for_template(capture, self._character_select_template, self.CHARACTER_SELECT_THRESHOLD, 20.0, "character select after exit")

    def _focus_game_window(self, capture: ScreenCapture) -> None:
        window = resolve_window_from_config(self.capture_config)
        if window is None:
            return
        if not focus_window(window):
            raise RuntimeError("Could not bring the Diablo window to the foreground from the control panel.")
        center_x = window.left + window.width // 2
        center_y = window.top + window.height // 2
        self._move_absolute(capture, center_x, center_y)
        self._last_pointer = (center_x, center_y)
        self._sleep_range(*self.FOCUS_SLEEP)

    def _wait_for_template(
        self,
        capture: ScreenCapture,
        template: np.ndarray,
        threshold: float,
        timeout_seconds: float,
        label: str,
    ) -> MatchResult:
        end_time = time.time() + timeout_seconds
        best_match: MatchResult | None = None
        while time.time() < end_time:
            if self._check_for_user_interrupt():
                raise RuntimeError("stopped by user interference")
            packet = capture.grab()
            match = self._locate_template(packet.frame, template, threshold)
            if match is not None:
                return match
            candidate = self._locate_template(packet.frame, template, 0.0)
            if candidate is not None and (best_match is None or candidate.score > best_match.score):
                best_match = candidate
            time.sleep(0.1)
        best_score = -1.0 if best_match is None else best_match.score
        raise RuntimeError(f"Timed out waiting for {label}. Best match score was {best_score:.3f}.")

    def _wait_until_template_missing(
        self,
        capture: ScreenCapture,
        template: np.ndarray,
        threshold: float,
        timeout_seconds: float,
        label: str,
    ) -> None:
        end_time = time.time() + timeout_seconds
        while time.time() < end_time:
            if self._check_for_user_interrupt():
                raise RuntimeError("stopped by user interference")
            packet = capture.grab()
            match = self._locate_template(packet.frame, template, threshold)
            if match is None:
                return
            time.sleep(0.1)
        raise RuntimeError(f"Timed out waiting for {label} to disappear.")

    def _load_image(self, path: Path) -> np.ndarray:
        if not path.exists():
            raise RuntimeError(f"Required asset is missing: {path.as_posix()}")
        data = np.fromfile(path, dtype=np.uint8)
        image = cv.imdecode(data, cv.IMREAD_COLOR)
        if image is None:
            raise RuntimeError(f"Failed to load asset: {path.as_posix()}")
        return image

    def _locate_template(self, frame: np.ndarray, template: np.ndarray, threshold: float) -> MatchResult | None:
        result = cv.matchTemplate(frame, template, cv.TM_CCOEFF_NORMED)
        _, max_value, _, max_loc = cv.minMaxLoc(result)
        if max_value < threshold:
            return None
        return MatchResult(top_left=max_loc, width=template.shape[1], height=template.shape[0], score=float(max_value))

    def _relative_point(self, capture: ScreenCapture, point_x: int, point_y: int) -> tuple[int, int]:
        return capture.target["left"] + point_x, capture.target["top"] + point_y

    def _click_relative(self, capture: ScreenCapture, point: tuple[int, int]) -> None:
        abs_x, abs_y = self._relative_point(capture, *point)
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click()
        self._last_pointer = (abs_x, abs_y)
        self._sleep_range(*self.ACTION_SLEEP)

    def _click_match_center(self, capture: ScreenCapture, match: MatchResult) -> None:
        abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
        abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click()
        self._last_pointer = (abs_x, abs_y)
        self._sleep_range(*self.ACTION_SLEEP)

    def _press_key(self, key: str) -> None:
        if self._check_for_user_interrupt():
            raise RuntimeError("stopped by user interference")
        pydirectinput.press(key)
        self._sleep_range(*self.ACTION_SLEEP)

    def _move_absolute(self, capture: ScreenCapture | None, target_x: int, target_y: int) -> None:
        if capture is not None:
            target = capture.target
            target_x = max(target["left"] + 2, min(target_x, target["left"] + target["width"] - 2))
            target_y = max(target["top"] + 2, min(target_y, target["top"] + target["height"] - 2))
        current_x, current_y = pydirectinput.position()
        steps = random.randint(*self.MOVE_STEPS)
        for step in range(1, steps + 1):
            if self._stop_event.is_set():
                return
            ratio = step / steps
            next_x = int(current_x + (target_x - current_x) * ratio)
            bend = random.randint(-8, 8)
            next_y = int(current_y + (target_y - current_y) * ratio + bend * (1 - abs(0.5 - ratio) * 2))
            pydirectinput.moveTo(next_x, next_y)
            self._last_pointer = (next_x, next_y)
            time.sleep(random.uniform(*self.MOVE_SLEEP))

    def _jitter_point(self, point_x: int, point_y: int, radius: int) -> tuple[int, int]:
        if radius <= 0:
            return point_x, point_y
        return point_x + random.randint(-radius, radius), point_y + random.randint(-radius, radius)

    def _check_for_user_interrupt(self) -> bool:
        if self._stop_event.is_set():
            return True
        if self._last_pointer is None:
            return False
        current = pydirectinput.position()
        if (
            abs(current[0] - self._last_pointer[0]) > self.USER_INTERRUPT_DISTANCE
            or abs(current[1] - self._last_pointer[1]) > self.USER_INTERRUPT_DISTANCE
        ):
            self._stop_event.set()
            return True
        return False

    def _sleep_range(self, low: float, high: float) -> None:
        end_time = time.time() + random.uniform(low, high)
        while time.time() < end_time:
            if self._stop_event.is_set():
                return
            time.sleep(0.01)

    def _bind_hotkey(self) -> None:
        if keyboard is None:
            return
        self._hotkey_handle = keyboard.add_hotkey(self.STOP_HOTKEY, self.request_stop)

    def _unbind_hotkey(self) -> None:
        if keyboard is None or self._hotkey_handle is None:
            return
        keyboard.remove_hotkey(self._hotkey_handle)
        self._hotkey_handle = None
