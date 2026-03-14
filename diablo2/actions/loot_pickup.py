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
from diablo2.common.config import BotConfig
from diablo2.common.controller import keyboard, pydirectinput


@dataclass
class LootEvent:
    level: str
    message: str


@dataclass
class MatchResult:
    top_left: tuple[int, int]
    width: int
    height: int
    score: float


@dataclass
class LootCandidate:
    label: str
    template: np.ndarray
    threshold: float


@dataclass
class LootScanHit:
    label: str
    match: MatchResult


@dataclass
class LootActionResult:
    found: bool
    picked_up: bool
    label: str | None = None
    message: str = ""


class LootPickupSession:
    STOP_HOTKEY = "f10"
    USER_INTERRUPT_DISTANCE = 80
    TARGET_JITTER = 3
    MOVE_STEPS = (2, 4)
    MOVE_SLEEP = (0.015, 0.035)
    CLICK_SETTLE = (0.05, 0.09)
    ACTION_SLEEP = (0.14, 0.25)
    FOCUS_SLEEP = (0.12, 0.2)
    QUIET_TIMEOUT = 1.2
    RESCAN_SLEEP = 0.12
    POST_PICKUP_SETTLE = (0.9, 1.1)
    DUPLICATE_POSITION_TOLERANCE = 18
    DUPLICATE_GUARD_SECONDS = 1.4

    def __init__(self, config: BotConfig):
        self.config = config
        self.events: Queue[LootEvent] = Queue()
        self._thread: threading.Thread | None = None
        self._stop_event = threading.Event()
        self._lock = threading.Lock()
        self._is_running = False
        self._hotkey_handle = None
        self._last_pointer: tuple[int, int] | None = None
        self._candidates_cache: list[LootCandidate] | None = None

    def update_config(self, config: BotConfig) -> None:
        self.config = config
        self._candidates_cache = None

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._is_running

    def start(self) -> None:
        with self._lock:
            if self._is_running:
                raise RuntimeError("Loot pickup is already running.")
            self._stop_event.clear()
            self._thread = threading.Thread(target=self._run, daemon=True)
            self._is_running = True
            self._thread.start()
        self.events.put(LootEvent("info", f"Loot pickup started. Press {self.STOP_HOTKEY.upper()} to stop."))

    def stop(self) -> None:
        self._stop_event.set()
        thread = self._thread
        if thread is not None:
            thread.join(timeout=3.0)
        with self._lock:
            self._thread = None
            self._is_running = False
        self.events.put(LootEvent("info", "Loot pickup stopped."))

    def request_stop(self) -> None:
        self._stop_event.set()

    def scan_visible_loot(self, capture: ScreenCapture) -> LootScanHit | None:
        candidates = self._build_candidates()
        packet = capture.grab()
        return self.scan_frame(packet.frame, candidates)

    def scan_frame(self, frame: np.ndarray, candidates: list[LootCandidate] | None = None) -> LootScanHit | None:
        active_candidates = candidates or self._build_candidates()
        for candidate in active_candidates:
            match = self._locate_template(frame, candidate.template, candidate.threshold)
            if match is not None:
                return LootScanHit(label=candidate.label, match=match)
        return None

    def try_pickup_visible_loot(self, capture: ScreenCapture, focus_window_first: bool = False) -> LootActionResult:
        if pydirectinput is None:
            raise RuntimeError("pydirectinput is required for loot pickup actions.")

        candidates = self._build_candidates()
        if not candidates:
            raise RuntimeError("No ground-loot items are configured in shared_loot.fixed_items or the active run profile.")

        if focus_window_first:
            self._focus_game_window(capture)

        hit = self.scan_visible_loot(capture)
        if hit is None:
            return LootActionResult(found=False, picked_up=False, message="No configured loot is visible right now.")

        self._click_match_center(capture, hit.match)
        return LootActionResult(found=True, picked_up=True, label=hit.label, message=f"Clicked {hit.label}.")

    def _run(self) -> None:
        try:
            if pydirectinput is None:
                raise RuntimeError("pydirectinput is required for loot pickup actions.")

            candidates = self._build_candidates()
            if not candidates:
                raise RuntimeError("No ground-loot items are configured in shared_loot.fixed_items or the active run profile.")

            capture = ScreenCapture(self.config.capture)
            self._bind_hotkey()
            self._focus_game_window(capture)

            labels = ", ".join(candidate.label for candidate in candidates)
            self.events.put(LootEvent("info", f"Loot pickup: scanning for configured ground items: {labels}"))

            hits = self._pickup_until_quiet(capture, candidates)
            if hits == 0:
                self.events.put(LootEvent("info", "Loot pickup: no configured loot appeared before the quiet timeout."))
            else:
                self.events.put(LootEvent("info", f"Loot pickup: finished after {hits} pickup(s)."))
        except Exception as exc:  # pragma: no cover
            self.events.put(LootEvent("error", f"Loot pickup failed: {exc}"))
        finally:
            self._unbind_hotkey()
            with self._lock:
                self._is_running = False

    def _build_candidates(self) -> list[LootCandidate]:
        if self._candidates_cache is not None:
            return self._candidates_cache

        candidates: list[LootCandidate] = []
        seen: set[tuple[str, str]] = set()

        for item in self.config.shared_loot.fixed_items:
            if not item.ground_template:
                continue
            marker = (item.label, item.ground_template)
            if marker in seen:
                continue
            template = self._load_image(Path(item.ground_template))
            candidates.append(LootCandidate(label=item.label, template=template, threshold=item.threshold))
            seen.add(marker)

        for rule in self.config.farm.templates:
            context = rule.context.casefold()
            marker = (rule.name, rule.path)
            if marker in seen:
                continue
            if context not in {"any", "ground"} and "ground" not in rule.name.casefold() and "ground" not in rule.path.casefold():
                continue
            template = self._load_image(Path(rule.path))
            label = rule.label or rule.name.replace("_", " ")
            candidates.append(LootCandidate(label=label, template=template, threshold=rule.threshold))
            seen.add(marker)

        self._candidates_cache = candidates
        return candidates

    def _focus_game_window(self, capture: ScreenCapture) -> None:
        window = resolve_window_from_config(self.config.capture)
        if window is None:
            return
        if not focus_window(window):
            raise RuntimeError("Could not bring the Diablo window to the foreground from the control panel.")
        center_x = window.left + window.width // 2
        center_y = window.top + window.height // 2
        self._move_absolute(capture, center_x, center_y)
        self._last_pointer = (center_x, center_y)
        self._sleep_range(*self.FOCUS_SLEEP)

    def _wait_for_any_candidate(self, capture: ScreenCapture, candidates: list[LootCandidate], timeout_seconds: float) -> LootScanHit:
        end_time = time.time() + timeout_seconds
        best_label = ""
        best_match: MatchResult | None = None
        while time.time() < end_time:
            if self._check_for_user_interrupt():
                raise RuntimeError("stopped by user interference")
            packet = capture.grab()
            hit = self.scan_frame(packet.frame, candidates)
            if hit is not None:
                return hit
            for candidate in candidates:
                fallback = self._locate_template(packet.frame, candidate.template, 0.0)
                if fallback is not None and (best_match is None or fallback.score > best_match.score):
                    best_match = fallback
                    best_label = candidate.label
            time.sleep(0.1)
        best_score = -1.0 if best_match is None else best_match.score
        label = best_label or "configured loot"
        raise RuntimeError(f"Timed out waiting for {label}. Best match score was {best_score:.3f}.")

    def _pickup_until_quiet(self, capture: ScreenCapture, candidates: list[LootCandidate]) -> int:
        quiet_deadline = time.time() + self.QUIET_TIMEOUT
        pickups = 0
        last_hit: LootScanHit | None = None
        last_hit_time = 0.0
        while time.time() < quiet_deadline:
            if self._check_for_user_interrupt():
                raise RuntimeError("stopped by user interference")
            packet = capture.grab()
            hit = self.scan_frame(packet.frame, candidates)
            if hit is None:
                time.sleep(self.RESCAN_SLEEP)
                continue
            now = time.time()
            if self._is_duplicate_hit(hit, last_hit, now - last_hit_time):
                time.sleep(self.RESCAN_SLEEP)
                continue
            self._click_match_center(capture, hit.match)
            pickups += 1
            last_hit = hit
            last_hit_time = now
            quiet_deadline = time.time() + self.QUIET_TIMEOUT
            self.events.put(LootEvent("info", f"Loot pickup: clicked {hit.label}."))
            self._sleep_range(*self.POST_PICKUP_SETTLE)
        return pickups

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

    def _is_duplicate_hit(self, current: LootScanHit, previous: LootScanHit | None, age_seconds: float) -> bool:
        if previous is None:
            return False
        if age_seconds > self.DUPLICATE_GUARD_SECONDS:
            return False
        if current.label != previous.label:
            return False
        return (
            abs(current.match.top_left[0] - previous.match.top_left[0]) <= self.DUPLICATE_POSITION_TOLERANCE
            and abs(current.match.top_left[1] - previous.match.top_left[1]) <= self.DUPLICATE_POSITION_TOLERANCE
        )

    def _click_match_center(self, capture: ScreenCapture, match: MatchResult) -> None:
        abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
        abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click(button="left")
        self._last_pointer = (abs_x, abs_y)
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
