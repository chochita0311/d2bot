from __future__ import annotations

import random
import json
import ctypes
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
        self._town_loop = False
        self._town_guard = None
        self._town_destination = "town"
        self._evidence_directory: Path | None = None
        self._stop_reason: str | None = None
        self.last_result: str | None = None
        self._town_mouse_held = False

        self._character_select_template = self._load_image(self.CHARACTER_SELECT_TEMPLATE_PATH)
        self._difficulty_template = self._load_image(self.DIFFICULTY_TEMPLATE_PATH)
        self._difficulty_button_templates = {name: self._load_image(path) for name, path in self.DIFFICULTY_BUTTON_TEMPLATE_PATHS.items()}
        self._loading_template = self._load_image(self.LOADING_TEMPLATE_PATH)
        self._exit_template = self._load_image(self.EXIT_TEMPLATE_PATH)

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._is_running

    def start(self, repeat_count: int | None, difficulty: str = "hell", town_loop: bool = False, destination: str = "town") -> None:
        if town_loop:
            from diablo2.actions.town_loop_vision import TownLoopPlanner, TownLoopVision

            TownLoopPlanner(TownLoopVision(), repeat_count, difficulty, destination)
        with self._lock:
            if self._is_running or (self._thread is not None and self._thread.is_alive()):
                raise RuntimeError("Run lifecycle is already running.")
            self._repeat_count = repeat_count
            self._difficulty = difficulty if difficulty in self.DIFFICULTY_BUTTON_TEMPLATE_PATHS else "hell"
            self._town_loop = town_loop
            self._town_destination = destination
            self._stop_reason = None
            self.last_result = None
            self._stop_event.clear()
            self._thread = threading.Thread(target=self._run, daemon=True)
            self._is_running = True
            self._thread.start()
        loop_text = "until stopped" if repeat_count is None else f"for {repeat_count} run(s)"
        difficulty_label = {"normal": "Normal", "nightmare": "Nightmare", "hell": "Hell"}[self._difficulty]
        self.events.put(
            LifecycleEvent("info", f"Run lifecycle started {loop_text} on {difficulty_label}. Press {self.STOP_HOTKEY.upper()} to stop.")
        )

    def stop(self) -> None:
        self.request_stop()
        thread = self._thread
        if thread is not None:
            thread.join(timeout=3.0)
        with self._lock:
            alive = thread is not None and thread.is_alive()
            if not alive:
                self._thread = None
                self._is_running = False
        self.events.put(LifecycleEvent("info", "Run lifecycle stopping." if alive else "Run lifecycle stopped."))

    def request_stop(self) -> None:
        self._stop_reason = "stop requested (F10 or session request)"
        self._stop_event.set()

    def _run(self) -> None:
        capture = None
        try:
            if pydirectinput is None:
                raise RuntimeError("pydirectinput is required for run lifecycle actions.")

            capture = ScreenCapture(self.capture_config)
            self._bind_hotkey()
            self._focus_game_window(capture)

            if self._town_loop:
                self._run_town_loop(capture)
                return

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
            self._town_guard = None
            # 정리 실패가 다음 세션을 막지 않도록 나머지 해제와 상태 정리를 계속한다.
            try:
                cleanup = [self._release_town_mouse, self._unbind_hotkey]
                if capture is not None:
                    cleanup.append(capture.close)
                for release in cleanup:
                    try:
                        release()
                    except Exception as exc:
                        self.last_result = "failed"
                        self.events.put(LifecycleEvent("error", f"Run lifecycle cleanup failed: {exc}"))
            finally:
                with self._lock:
                    self._is_running = False

    def _run_town_loop(self, capture: ScreenCapture) -> None:
        from diablo2.actions.town_loop_vision import REFERENCE_SIZE, TownLoopPlanner, TownLoopVision

        planner = TownLoopPlanner(TownLoopVision(), self._repeat_count, self._difficulty, self._town_destination)
        window = resolve_window_from_config(self.capture_config)
        if window is None:
            raise RuntimeError("Town loop requires a named Diablo window.")
        initial_geometry = (window.handle, window.left, window.top, window.width, window.height)

        def guard():
            current = resolve_window_from_config(self.capture_config)
            geometry = None if current is None else (current.handle, current.left, current.top, current.width, current.height)
            if geometry != initial_geometry:
                raise RuntimeError("Game window moved or resized; town loop stopped before further input.")
            if ctypes.windll.user32.GetForegroundWindow() != window.handle:
                raise RuntimeError("Game focus was lost; town loop stopped before further input.")

        self._town_guard = guard
        directory = Path("recordings/summoner/evidence") / time.strftime("%Y%m%d-%H%M%S-town-loop")
        directory.mkdir(parents=True, exist_ok=False)
        self._evidence_directory = directory
        events_path = directory / "events.jsonl"
        previous_phase = None
        result = "stopped"
        timings = []
        cursor_pending_since = None
        previous_sample_start = None
        try:
            while not self._stop_event.is_set():
                guard()
                sample_start = time.perf_counter()
                interval_ms = None if previous_sample_start is None else (sample_start - previous_sample_start) * 1000
                previous_sample_start = sample_start
                packet = capture.grab()
                observed_at = time.perf_counter()
                cursor_state = self._accept_waypoint_cursor_shift(packet.frame, planner, window.width)
                if cursor_state == "pending":
                    cursor_pending_since = time.perf_counter() if cursor_pending_since is None else cursor_pending_since
                    if time.perf_counter() - cursor_pending_since < 0.45:
                        self._stop_event.wait(0.04)
                        continue
                else:
                    cursor_pending_since = None
                if self._check_for_user_interrupt():
                    break
                action = planner.step(packet.frame)
                planned_at = time.perf_counter()
                phase = planner.data["phase"]
                dispatch_at = None
                if action is not None:
                    guard()
                    dispatch_at = time.perf_counter()
                    self._dispatch_town_action(capture, packet.frame, action, REFERENCE_SIZE)
                sample = {
                    "interval_ms": interval_ms,
                    "capture_ms": (observed_at - sample_start) * 1000,
                    "vision_ms": (planned_at - observed_at) * 1000,
                    "input_ms": (time.perf_counter() - dispatch_at) * 1000 if dispatch_at else 0,
                    "observation_to_input_ms": (dispatch_at - observed_at) * 1000 if dispatch_at else None,
                }
                timings.append(sample)
                if action is not None or phase != previous_phase:
                    filename = f"{planner.data['action_number']:03d}-{phase}.png"
                    cv.imencode(".png", packet.frame)[1].tofile(str(directory / filename))
                    with events_path.open("a", encoding="utf-8") as stream:
                        stream.write(
                            json.dumps(
                                {
                                    "time": time.time(),
                                    "phase": phase,
                                    "completed": planner.data["completed"],
                                    "town_layout": planner.data["town_layout"],
                                    "action": action,
                                    "frame": filename,
                                    "timing": sample,
                                    "pointer": self._last_pointer,
                                }
                            )
                            + "\n"
                        )
                    self.events.put(
                        LifecycleEvent("info", f"Town loop: {phase}, completed {planner.data['completed']}/{self._repeat_count}.")
                    )
                    previous_phase = phase
                if phase == "done":
                    result = "passed"
                    self.events.put(
                        LifecycleEvent("info", f"Town loop completed {planner.data['completed']} run(s). Evidence: {directory}")
                    )
                    return
                self._stop_event.wait(0.04)
        except Exception as exc:
            result = "failed"
            if "packet" in locals():
                cv.imencode(".png", packet.frame)[1].tofile(str(directory / "failure-frame.png"))
            with events_path.open("a", encoding="utf-8") as stream:
                stream.write(json.dumps({"time": time.time(), "error": str(exc), "phase": planner.data["phase"]}) + "\n")
            raise
        finally:
            self._release_town_mouse()
            if result == "stopped" and "packet" in locals():
                cv.imencode(".png", packet.frame)[1].tofile(str(directory / "stopped-frame.png"))
            self.last_result = result
            (directory / "result.json").write_text(
                json.dumps({"result": result, "planner": planner.data, "geometry": initial_geometry}, indent=2), encoding="utf-8"
            )
            (directory / "timings.json").write_text(json.dumps(timings), encoding="utf-8")
            if self._stop_reason is not None:
                with events_path.open("a", encoding="utf-8") as stream:
                    stream.write(json.dumps({"time": time.time(), "stop_reason": self._stop_reason}) + "\n")
            self._town_guard = None

    def _accept_waypoint_cursor_shift(self, frame, planner, window_width):
        # WP 패널을 열면 게임이 커서를 반 패널 너비만큼 오른쪽으로 옮긴다.
        # 메뉴의 긍정 증거와 예상 변위가 모두 맞을 때만 이 이동을 인정한다.
        phase = planner.data["phase"]
        opening = phase in {"wait_act1_panel", "return_panel"}
        closing = phase in {"destination_arrival", "act1_return"}
        if not (opening or closing) or self._last_pointer is None:
            return
        current = pydirectinput.position()
        dx, dy = current[0] - self._last_pointer[0], current[1] - self._last_pointer[1]
        if abs(dx - window_width * 0.1875 * (1 if opening else -1)) > 8 or abs(dy) > 3:
            return
        gray = planner.vision.normalize(frame)
        menu = planner.vision.find(gray, "waypoint_header")
        arrival_name = "area_act1" if phase == "act1_return" else "area_act2" if planner.data["destination"] == "town" else "area_arcane"
        confirmed = (
            (menu and (planner.vision.find(gray, "list_act1") or planner.vision.find(gray, "list_act2")))
            if opening
            else (not menu and planner.vision.find(gray, "hud") and planner.vision.find(gray, arrival_name))
        )
        if confirmed:
            self.events.put(LifecycleEvent("info", f"Confirmed waypoint-panel cursor shift: {self._last_pointer} -> {tuple(current)}"))
            self._last_pointer = tuple(current)
            return "confirmed"
        return "pending"

    def _release_town_mouse(self):
        if getattr(self, "_town_mouse_held", False):
            pydirectinput.mouseUp(button="left", _pause=False)
            self._town_mouse_held = False

    def _dispatch_town_action(self, capture, frame, action, reference_size):
        # 중단·포커스 상실 뒤에도 홀드를 반드시 해제한다.
        if action["kind"] == "release":
            self._release_town_mouse()
            return
        self._ensure_input_allowed()
        if action["kind"] == "key":
            self._release_town_mouse()
            pydirectinput.press(action["key"], _pause=False)
            return
        height, width = frame.shape[:2]
        point = (round(action["point"][0] * width / reference_size[0]), round(action["point"][1] * height / reference_size[1]))
        self._move_absolute(capture, *self._relative_point(capture, *point))
        self._ensure_input_allowed()
        if action["kind"] == "hold":
            self._town_mouse_held = True
            pydirectinput.mouseDown(button="left", _pause=False)
        else:
            self._release_town_mouse()
            pydirectinput.click(_pause=False)

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
        self._wait_for_template(
            capture, self._character_select_template, self.CHARACTER_SELECT_THRESHOLD, 20.0, "character select after exit"
        )

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
        if frame.shape[0] < template.shape[0] or frame.shape[1] < template.shape[1]:
            return None
        result = cv.matchTemplate(frame, template, cv.TM_CCOEFF_NORMED)
        _, max_value, _, max_loc = cv.minMaxLoc(result)
        if max_value < threshold:
            return None
        return MatchResult(top_left=max_loc, width=template.shape[1], height=template.shape[0], score=float(max_value))

    def _relative_point(self, capture: ScreenCapture, point_x: int, point_y: int) -> tuple[int, int]:
        return capture.target["left"] + point_x, capture.target["top"] + point_y

    def _click_relative(self, capture: ScreenCapture, point: tuple[int, int]) -> None:
        self._ensure_input_allowed()
        abs_x, abs_y = self._relative_point(capture, *point)
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        self._ensure_input_allowed()
        pydirectinput.click()
        self._last_pointer = (abs_x, abs_y)
        self._sleep_range(*self.ACTION_SLEEP)

    def _click_match_center(self, capture: ScreenCapture, match: MatchResult) -> None:
        self._ensure_input_allowed()
        abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
        abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        self._ensure_input_allowed()
        pydirectinput.click()
        self._last_pointer = (abs_x, abs_y)
        self._sleep_range(*self.ACTION_SLEEP)

    def _press_key(self, key: str) -> None:
        self._ensure_input_allowed()
        pydirectinput.press(key)
        self._sleep_range(*self.ACTION_SLEEP)

    def _ensure_input_allowed(self) -> None:
        if self._check_for_user_interrupt():
            raise RuntimeError("stopped by user interference")
        if self._town_guard is not None:
            self._town_guard()

    def _move_absolute(self, capture: ScreenCapture | None, target_x: int, target_y: int) -> None:
        if capture is not None:
            target = capture.target
            target_x = max(target["left"] + 2, min(target_x, target["left"] + target["width"] - 2))
            target_y = max(target["top"] + 2, min(target_y, target["top"] + target["height"] - 2))
        if self._town_loop:
            # 가상 데스크톱 좌표로 SendInput을 보낸다. SetCursorPos만 쓰면
            # 게임의 상대 입력 처리와 커서 위치가 어긋날 수 있다.
            if self._stop_event.is_set():
                return
            user32 = ctypes.windll.user32
            origin_x, origin_y = user32.GetSystemMetrics(76), user32.GetSystemMetrics(77)
            desktop_width, desktop_height = user32.GetSystemMetrics(78), user32.GetSystemMetrics(79)
            dx = round((target_x - origin_x) * 65535 / (desktop_width - 1))
            dy = round((target_y - origin_y) * 65535 / (desktop_height - 1))
            extra = ctypes.c_ulong(0)
            payload = pydirectinput.Input_I()
            payload.mi = pydirectinput.MouseInput(dx, dy, 0, 0x0001 | 0x8000 | 0x4000, 0, ctypes.pointer(extra))
            event = pydirectinput.Input(ctypes.c_ulong(0), payload)
            if pydirectinput.SendInput(1, ctypes.pointer(event), ctypes.sizeof(event)) != 1:
                raise RuntimeError("Desktop pointer movement failed.")
            self._stop_event.wait(0.012)
            actual = pydirectinput.position()
            if abs(actual[0] - target_x) > 3 or abs(actual[1] - target_y) > 3:
                raise RuntimeError(f"Pointer mapping mismatch: requested {(target_x, target_y)}, observed {actual}")
            self._last_pointer = tuple(actual)
            return
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
            self._stop_reason = f"pointer changed outside planned movement: expected {self._last_pointer}, observed {current}"
            self.events.put(LifecycleEvent("info", self._stop_reason))
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
