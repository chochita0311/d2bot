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
class SummonerEvent:
    level: str
    message: str


@dataclass
class MatchResult:
    top_left: tuple[int, int]
    width: int
    height: int
    score: float
    source_index: int | None = None


class SummonerRunSession:
    STOP_HOTKEY = "f10"

    ACT1_MAP_TEMPLATE_PATHS = (
        Path("assets/waypoint/act1/act1_자매단_야영지_on_map_1.png"),
        Path("assets/waypoint/act1/act1_자매단_야영지_on_map_2.png"),
    )
    ACT1_WAYPOINT_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_자매단_야영지.png")
    ACT1_WAYPOINT_HOVER_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_자매단_야영지_when_hover.png")
    ACT1_LIST_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_list_when_left_click.png")
    ACT1_LIST_PANEL_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_list.png")
    ACT2_LIST_PANEL_TEMPLATE_PATH = Path("assets/waypoint/act2/act2_list.png")

    MAP_THRESHOLD = 0.5
    WAYPOINT_THRESHOLD = 0.74
    WAYPOINT_HOVER_THRESHOLD = 0.74
    LIST_THRESHOLD = 0.93
    LIST_PANEL_THRESHOLD = 0.88

    USER_INTERRUPT_DISTANCE = 80
    TARGET_JITTER = 3
    MOVE_STEPS = (1, 2)
    MOVE_SLEEP = (0.005, 0.015)
    CLICK_SETTLE = (0.01, 0.03)
    ACTION_SLEEP = (0.01, 0.03)
    FOCUS_SLEEP = (0.12, 0.2)

    MINIMAP_SLEEP = (0.05, 0.12)
    SCOUT_MOVE_SETTLE = (0.01, 0.02)
    SCOUT_HOLD_SETTLE = (0.01, 0.02)
    SCOUT_UP_SETTLE = (0.28, 0.32)
    SCOUT_RIGHT_SETTLE = (0.28, 0.32)
    HOVER_WAIT_SECONDS = 2.4
    WAYPOINT_LIST_WAIT_SECONDS = 1.2
    MAP_SCAN_TIMEOUT = 2.2
    WAYPOINT_SCAN_TIMEOUT = 1.0
    OPEN_ATTEMPTS = 4
    ACT2_TAB_RATIO = (136 / 444, 18 / 599)
    ARCANE_SANCTUARY_RATIO = (220 / 447, 498 / 597)
    MINIMAP_SCOUT_POINTS = ((0.66, 0.44), (0.74, 0.42), (0.80, 0.40))
    WORLD_SCOUT_ANCHOR_OFFSET = (0.14, 0.0)
    WORLD_SCOUT_UP_HOLD_POINT = (0.0, -0.18)
    WORLD_SCOUT_DOWN_HOLD_POINT = (0.0, 0.24)
    WORLD_SCOUT_UP_HOLD_SECONDS = (0.75, 0.80)
    WORLD_SCOUT_DOWN_HOLD_SECONDS = (1.60, 1.70)
    WORLD_SCOUT_RIGHT_HOLD_POINT = (0.20, 0.0)
    WORLD_SCOUT_RIGHT_HOLD_SECONDS = (0.25, 0.35)

    def __init__(self, config: BotConfig):
        self.config = config
        self.events: Queue[SummonerEvent] = Queue()
        self._thread: threading.Thread | None = None
        self._stop_event = threading.Event()
        self._lock = threading.Lock()
        self._is_running = False
        self._hotkey_handle = None
        self._last_pointer: tuple[int, int] | None = None
        self._current_scout_anchor = (0.5, 0.5)
        self._last_detection_route = "direct"

        self._act1_map_templates = [self._load_image(path) for path in self.ACT1_MAP_TEMPLATE_PATHS]
        self._act1_waypoint_template = self._load_image(self.ACT1_WAYPOINT_TEMPLATE_PATH)
        self._act1_waypoint_hover_template = self._load_image(self.ACT1_WAYPOINT_HOVER_TEMPLATE_PATH)
        self._act1_list_template = self._load_image(self.ACT1_LIST_TEMPLATE_PATH)
        self._act1_list_panel_template = self._load_image(self.ACT1_LIST_PANEL_TEMPLATE_PATH)
        self._act2_list_panel_template = self._load_image(self.ACT2_LIST_PANEL_TEMPLATE_PATH)

    def update_config(self, config: BotConfig) -> None:
        self.config = config

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._is_running

    def start(self) -> None:
        with self._lock:
            if self._is_running:
                raise RuntimeError("Summoner run is already running.")
            self._stop_event.clear()
            self._thread = threading.Thread(target=self._run, daemon=True)
            self._is_running = True
            self._thread.start()
        self.events.put(SummonerEvent("info", f"Summoner run started. Press {self.STOP_HOTKEY.upper()} to stop."))

    def stop(self) -> None:
        self._stop_event.set()
        thread = self._thread
        if thread is not None:
            thread.join(timeout=3.0)
        with self._lock:
            self._thread = None
            self._is_running = False
        self.events.put(SummonerEvent("info", "Summoner run stopped."))

    def request_stop(self) -> None:
        self._stop_event.set()

    def _run(self) -> None:
        try:
            if pydirectinput is None:
                raise RuntimeError("pydirectinput is required for Summoner run actions.")

            capture = ScreenCapture(self.config.capture)
            self._bind_hotkey()
            self._focus_game_window(capture)

            self.events.put(SummonerEvent("info", "Summoner: assuming the minimap is already open in Act 1 town."))
            self._sleep_range(*self.MINIMAP_SLEEP)

            map_match = self._find_minimap_waypoint_with_scouting(capture)
            self.events.put(
                SummonerEvent(
                    "info",
                    f"Summoner: found the Act 1 waypoint marker on the minimap (score={map_match.score:.3f}); using it only as direction guidance.",
                )
            )
            self.events.put(SummonerEvent("info", "Summoner: moving to the town center-right anchor before waypoint search."))
            self._current_scout_anchor = self._center_right_anchor()
            self._click_relative_ratio(capture, *self._current_scout_anchor, apply_jitter=False)
            self._sleep_range(*self.SCOUT_MOVE_SETTLE)
            self.events.put(
                SummonerEvent(
                    "info",
                    "Summoner: scanning the world view for the real waypoint object with the center-right vertical sweep.",
                )
            )

            act1_panel = self._open_act1_waypoint_list(capture)
            self.events.put(SummonerEvent("info", "Summoner: Act 1 waypoint list is open."))
            self._travel_to_arcane_sanctuary(capture, act1_panel)
            self.events.put(SummonerEvent("info", "Summoner: switched to Act 2 and clicked Arcane Sanctuary."))
        except Exception as exc:  # pragma: no cover
            self.events.put(SummonerEvent("error", f"Summoner run failed: {exc}"))
        finally:
            self._unbind_hotkey()
            with self._lock:
                self._is_running = False

    def _find_minimap_waypoint_with_scouting(self, capture: ScreenCapture) -> MatchResult:
        best_match: MatchResult | None = None

        initial = self._wait_for_optional_any_template(
            capture,
            self._act1_map_templates,
            self.MAP_THRESHOLD,
            self.MAP_SCAN_TIMEOUT,
        )
        if initial is not None:
            self._last_detection_route = "direct"
            return initial
        best_match = self._locate_best_template(capture.grab().frame, self._act1_map_templates, 0.0)

        for index, point in enumerate(self.MINIMAP_SCOUT_POINTS, start=1):
            self.events.put(
                SummonerEvent(
                    "info",
                    f"Summoner: waypoint marker is faint from spawn view, scouting right ({index}/{len(self.MINIMAP_SCOUT_POINTS)}).",
                )
            )
            self._click_relative_ratio(capture, point[0], point[1], apply_jitter=False)
            self._sleep_range(*self.SCOUT_MOVE_SETTLE)

            match = self._wait_for_optional_any_template(
                capture,
                self._act1_map_templates,
                self.MAP_THRESHOLD,
                self.MAP_SCAN_TIMEOUT,
            )
            if match is not None:
                self._last_detection_route = "vertical_sweep"
                return match

            fallback = self._locate_best_template(capture.grab().frame, self._act1_map_templates, 0.0)
            if fallback is not None and (best_match is None or fallback.score > best_match.score):
                best_match = fallback

        best_score = -1.0 if best_match is None else best_match.score
        raise RuntimeError(f"Timed out waiting for Act 1 waypoint on minimap. Best match score was {best_score:.3f}.")

    def _refresh_visible_waypoint(self, capture: ScreenCapture, fallback: MatchResult) -> MatchResult:
        packet = capture.grab()
        refreshed = self._locate_best_template(
            packet.frame,
            [self._act1_waypoint_hover_template, self._act1_waypoint_template],
            self.WAYPOINT_THRESHOLD,
        )
        return refreshed or fallback

    def _find_world_waypoint_with_scouting(self, capture: ScreenCapture) -> MatchResult:
        best_match: MatchResult | None = None

        initial = self._wait_for_optional_any_template(
            capture,
            [self._act1_waypoint_hover_template, self._act1_waypoint_template],
            self.WAYPOINT_THRESHOLD,
            self.WAYPOINT_SCAN_TIMEOUT,
        )
        if initial is not None:
            return initial
        best_match = self._locate_best_template(capture.grab().frame, [self._act1_waypoint_hover_template, self._act1_waypoint_template], 0.0)

        vertical_anchor = (0.5, 0.5)
        upward_point = self._apply_offset(vertical_anchor, self.WORLD_SCOUT_UP_HOLD_POINT)
        downward_point = self._apply_offset(vertical_anchor, self.WORLD_SCOUT_DOWN_HOLD_POINT)

        self.events.put(
            SummonerEvent(
                "info",
                "Summoner: waypoint not visible yet, holding upward movement from the Diablo window center.",
            )
        )
        match = self._hold_move_until_waypoint(
            capture,
            upward_point[0],
            upward_point[1],
            self.WORLD_SCOUT_UP_HOLD_SECONDS,
        )
        self._sleep_range(*self.SCOUT_UP_SETTLE)

        if match is None:
            match = self._wait_for_optional_any_template(
                capture,
                [self._act1_waypoint_hover_template, self._act1_waypoint_template],
                self.WAYPOINT_THRESHOLD,
                self.WAYPOINT_SCAN_TIMEOUT,
            )
        if match is not None:
            self.events.put(
                SummonerEvent(
                    "info",
                    "Summoner: waypoint became visible during the upward hold; moving to it now.",
                )
            )
            self._last_detection_route = "vertical_sweep"
            return match

        fallback = self._locate_best_template(capture.grab().frame, [self._act1_waypoint_hover_template, self._act1_waypoint_template], 0.0)
        if fallback is not None and (best_match is None or fallback.score > best_match.score):
            best_match = fallback

        self.events.put(
            SummonerEvent(
                "info",
                "Summoner: waypoint not visible yet, holding downward movement from the Diablo window center.",
            )
        )
        match = self._hold_move_until_waypoint(
            capture,
            downward_point[0],
            downward_point[1],
            self.WORLD_SCOUT_DOWN_HOLD_SECONDS,
        )
        self._sleep_range(*self.SCOUT_RIGHT_SETTLE)

        if match is None:
            match = self._wait_for_optional_any_template(
                capture,
                [self._act1_waypoint_hover_template, self._act1_waypoint_template],
                self.WAYPOINT_THRESHOLD,
                self.WAYPOINT_SCAN_TIMEOUT,
            )
        if match is not None:
            self.events.put(
                SummonerEvent(
                    "info",
                    "Summoner: waypoint became visible during the downward hold; moving to it now.",
                )
            )
            self._last_detection_route = "vertical_sweep"
            return match

        fallback = self._locate_best_template(capture.grab().frame, [self._act1_waypoint_hover_template, self._act1_waypoint_template], 0.0)
        if fallback is not None and (best_match is None or fallback.score > best_match.score):
            best_match = fallback

        right_point = self._apply_offset((0.5, 0.5), self.WORLD_SCOUT_RIGHT_HOLD_POINT)
        self.events.put(
            SummonerEvent(
                "info",
                "Summoner: vertical sweep missed; holding rightward movement from the Diablo window center.",
            )
        )
        match = self._hold_move_until_waypoint(
            capture,
            right_point[0],
            right_point[1],
            self.WORLD_SCOUT_RIGHT_HOLD_SECONDS,
        )
        self._sleep_range(*self.SCOUT_HOLD_SETTLE)

        if match is None:
            match = self._wait_for_optional_any_template(
                capture,
                [self._act1_waypoint_hover_template, self._act1_waypoint_template],
                self.WAYPOINT_THRESHOLD,
                self.WAYPOINT_SCAN_TIMEOUT,
            )
        if match is not None:
            self.events.put(
                SummonerEvent(
                    "info",
                    "Summoner: waypoint became visible after the rightward hold; moving to it now.",
                )
            )
            self._last_detection_route = "right_probe"
            return match

        fallback = self._locate_best_template(capture.grab().frame, [self._act1_waypoint_hover_template, self._act1_waypoint_template], 0.0)
        if fallback is not None and (best_match is None or fallback.score > best_match.score):
            best_match = fallback

        best_score = -1.0 if best_match is None else best_match.score
        raise RuntimeError(f"Timed out waiting for Act 1 waypoint object. Best match score was {best_score:.3f}.")

    def _open_act1_waypoint_list(self, capture: ScreenCapture) -> MatchResult:
        for attempt in range(1, self.OPEN_ATTEMPTS + 1):
            open_match = self._locate_current_waypoint_list(capture)
            if open_match is not None:
                panel_match = self._locate_waypoint_panel(capture, self._act1_list_panel_template)
                if panel_match is not None:
                    return panel_match
                return open_match

            target_match = self._find_world_waypoint_with_scouting(capture)
            target_match = self._refresh_visible_waypoint(capture, target_match)
            self.events.put(
                SummonerEvent(
                    "info",
                    f"Summoner: found the real waypoint object on screen (attempt {attempt}/{self.OPEN_ATTEMPTS}, score={target_match.score:.3f}).",
                )
            )
            self._approach_waypoint(capture, target_match)
            self._sleep_range(*self.CLICK_SETTLE)

            hover_match = self._wait_for_optional_template(
                capture,
                self._act1_waypoint_hover_template,
                self.WAYPOINT_HOVER_THRESHOLD,
                self.HOVER_WAIT_SECONDS,
            )
            click_target = hover_match or target_match
            self.events.put(
                SummonerEvent(
                    "info",
                    f"Summoner: opening the waypoint list (attempt {attempt}/{self.OPEN_ATTEMPTS}).",
                )
            )
            self._click_match_center(capture, click_target)
            self._sleep_range(*self.ACTION_SLEEP)

            panel_match = self._wait_for_waypoint_list_open(capture, self.WAYPOINT_LIST_WAIT_SECONDS)
            if panel_match is not None:
                return panel_match

        raise RuntimeError("Could not open the Act 1 waypoint list from town.")

    def _wait_for_waypoint_list_open(self, capture: ScreenCapture, timeout_seconds: float) -> MatchResult | None:
        end_time = time.time() + timeout_seconds
        best_match: MatchResult | None = None
        templates = [self._act1_list_panel_template, self._act1_list_template]
        thresholds = [self.LIST_PANEL_THRESHOLD, self.LIST_THRESHOLD]
        while time.time() < end_time:
            packet = capture.grab()
            for template, threshold in zip(templates, thresholds):
                match = self._locate_template(packet.frame, template, threshold)
                if match is not None:
                    return match
                fallback = self._locate_template(packet.frame, template, 0.0)
                if fallback is not None and (best_match is None or fallback.score > best_match.score):
                    best_match = fallback
            time.sleep(0.03)
        return None

    def _travel_to_arcane_sanctuary(self, capture: ScreenCapture, act1_panel: MatchResult) -> None:
        self.events.put(SummonerEvent("info", "Summoner: clicking the Act 2 tab in the waypoint list."))
        self._click_panel_ratio(capture, act1_panel, self.ACT2_TAB_RATIO)
        self._sleep_range(*self.ACTION_SLEEP)

        act2_panel = self._wait_for_waypoint_panel(capture, self._act2_list_panel_template, self.LIST_PANEL_THRESHOLD, 2.5, "Act 2 waypoint list")
        self.events.put(SummonerEvent("info", "Summoner: Act 2 waypoint list is open."))
        self.events.put(SummonerEvent("info", "Summoner: clicking Arcane Sanctuary in the Act 2 list."))
        self._click_panel_ratio(capture, act2_panel, self.ARCANE_SANCTUARY_RATIO)
        self._sleep_range(*self.ACTION_SLEEP)

    def _locate_waypoint_panel(self, capture: ScreenCapture, template: np.ndarray) -> MatchResult | None:
        packet = capture.grab()
        return self._locate_template(packet.frame, template, self.LIST_PANEL_THRESHOLD)

    def _wait_for_waypoint_panel(
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
            fallback = self._locate_template(packet.frame, template, 0.0)
            if fallback is not None and (best_match is None or fallback.score > best_match.score):
                best_match = fallback
            time.sleep(0.02)
        best_score = -1.0 if best_match is None else best_match.score
        raise RuntimeError(f"Timed out waiting for {label}. Best match score was {best_score:.3f}.")

    def _click_panel_ratio(self, capture: ScreenCapture, panel_match: MatchResult, ratio: tuple[float, float]) -> None:
        point_x = int(panel_match.width * ratio[0])
        point_y = int(panel_match.height * ratio[1])
        self._click_panel_point(capture, panel_match, (point_x, point_y))

    def _click_panel_point(self, capture: ScreenCapture, panel_match: MatchResult, point: tuple[int, int]) -> None:
        abs_x = capture.target["left"] + panel_match.top_left[0] + point[0]
        abs_y = capture.target["top"] + panel_match.top_left[1] + point[1]
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click(button="left")
        self._last_pointer = (abs_x, abs_y)
        self._sleep_range(*self.ACTION_SLEEP)

    def _center_right_anchor(self) -> tuple[float, float]:
        return self._apply_offset((0.5, 0.5), self.WORLD_SCOUT_ANCHOR_OFFSET)

    def _apply_offset(self, point: tuple[float, float], offset: tuple[float, float]) -> tuple[float, float]:
        x = min(0.95, max(0.05, point[0] + offset[0]))
        y = min(0.95, max(0.05, point[1] + offset[1]))
        return x, y

    def _approach_waypoint(self, capture: ScreenCapture, match: MatchResult) -> None:
        self._move_to_match_center(capture, match)

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

    def _load_image(self, path: Path) -> np.ndarray:
        if not path.exists():
            raise RuntimeError(f"Required asset is missing: {path.as_posix()}")
        data = np.fromfile(path, dtype=np.uint8)
        image = cv.imdecode(data, cv.IMREAD_COLOR)
        if image is None:
            raise RuntimeError(f"Failed to load asset: {path.as_posix()}")
        return image

    def _wait_for_any_template(
        self,
        capture: ScreenCapture,
        templates: list[np.ndarray],
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
            current_best = self._locate_best_template(packet.frame, templates, threshold)
            if current_best is not None:
                return current_best
            fallback = self._locate_best_template(packet.frame, templates, 0.0)
            if fallback is not None and (best_match is None or fallback.score > best_match.score):
                best_match = fallback
            time.sleep(0.03)
        best_score = -1.0 if best_match is None else best_match.score
        raise RuntimeError(f"Timed out waiting for {label}. Best match score was {best_score:.3f}.")

    def _wait_for_optional_any_template(
        self,
        capture: ScreenCapture,
        templates: list[np.ndarray],
        threshold: float,
        timeout_seconds: float,
    ) -> MatchResult | None:
        end_time = time.time() + timeout_seconds
        while time.time() < end_time:
            if self._check_for_user_interrupt():
                raise RuntimeError("stopped by user interference")
            packet = capture.grab()
            match = self._locate_best_template(packet.frame, templates, threshold)
            if match is not None:
                return match
            time.sleep(0.02)
        return None

    def _wait_for_optional_template(
        self,
        capture: ScreenCapture,
        template: np.ndarray,
        threshold: float,
        timeout_seconds: float,
    ) -> MatchResult | None:
        end_time = time.time() + timeout_seconds
        while time.time() < end_time:
            if self._check_for_user_interrupt():
                raise RuntimeError("stopped by user interference")
            packet = capture.grab()
            match = self._locate_template(packet.frame, template, threshold)
            if match is not None:
                return match
            time.sleep(0.02)
        return None

    def _locate_current_waypoint_list(self, capture: ScreenCapture) -> MatchResult | None:
        packet = capture.grab()
        return self._locate_template(packet.frame, self._act1_list_template, self.LIST_THRESHOLD)

    def _locate_best_template(
        self,
        frame: np.ndarray,
        templates: list[np.ndarray],
        threshold: float,
    ) -> MatchResult | None:
        best: MatchResult | None = None
        for index, template in enumerate(templates):
            result = self._locate_template(frame, template, threshold)
            if result is None:
                continue
            result.source_index = index
            if best is None or result.score > best.score:
                best = result
        return best

    def _locate_template(self, frame: np.ndarray, template: np.ndarray, threshold: float) -> MatchResult | None:
        result = cv.matchTemplate(frame, template, cv.TM_CCOEFF_NORMED)
        _, max_value, _, max_loc = cv.minMaxLoc(result)
        if max_value < threshold:
            return None
        return MatchResult(top_left=max_loc, width=template.shape[1], height=template.shape[0], score=float(max_value))

    def _click_relative_ratio(self, capture: ScreenCapture, ratio_x: float, ratio_y: float, apply_jitter: bool = True) -> None:
        abs_x = capture.target["left"] + int(capture.target["width"] * ratio_x)
        abs_y = capture.target["top"] + int(capture.target["height"] * ratio_y)
        if apply_jitter:
            abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click(button="left")
        self._last_pointer = (abs_x, abs_y)
        self._sleep_range(*self.ACTION_SLEEP)

    def _hold_move_relative_ratio(
        self,
        capture: ScreenCapture,
        ratio_x: float,
        ratio_y: float,
        hold_seconds: tuple[float, float],
        apply_jitter: bool = True,
    ) -> None:
        abs_x = capture.target["left"] + int(capture.target["width"] * ratio_x)
        abs_y = capture.target["top"] + int(capture.target["height"] * ratio_y)
        if apply_jitter:
            abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.mouseDown(button="left")
        self._last_pointer = (abs_x, abs_y)
        self._sleep_range(*hold_seconds)
        pydirectinput.mouseUp(button="left")
        self._sleep_range(*self.ACTION_SLEEP)

    def _hold_move_until_waypoint(
        self,
        capture: ScreenCapture,
        ratio_x: float,
        ratio_y: float,
        hold_seconds: tuple[float, float],
    ) -> MatchResult | None:
        abs_x = capture.target["left"] + int(capture.target["width"] * ratio_x)
        abs_y = capture.target["top"] + int(capture.target["height"] * ratio_y)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.mouseDown(button="left")
        self._last_pointer = (abs_x, abs_y)
        try:
            end_time = time.time() + random.uniform(*hold_seconds)
            while time.time() < end_time:
                if self._check_for_user_interrupt():
                    raise RuntimeError("stopped by user interference")
                packet = capture.grab()
                match = self._locate_best_template(
                    packet.frame,
                    [self._act1_waypoint_hover_template, self._act1_waypoint_template],
                    self.WAYPOINT_THRESHOLD,
                )
                if match is not None:
                    return match
                time.sleep(0.02)
            return None
        finally:
            pydirectinput.mouseUp(button="left")
            self._sleep_range(*self.ACTION_SLEEP)

    def _move_to_match_center(self, capture: ScreenCapture, match: MatchResult) -> None:
        abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
        abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._last_pointer = (abs_x, abs_y)

    def _click_match_center(self, capture: ScreenCapture, match: MatchResult) -> None:
        abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
        abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click(button="left")
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
