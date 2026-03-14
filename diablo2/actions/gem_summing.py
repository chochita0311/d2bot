from __future__ import annotations

import math
import random
import threading
import time
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from queue import Queue

import cv2 as cv
import numpy as np

from diablo2.common.capture import ScreenCapture, focus_window, resolve_window_from_config
from diablo2.common.config import CaptureConfig
from diablo2.common.controller import keyboard, pydirectinput


@dataclass
class GemActionEvent:
    level: str
    message: str


@dataclass
class GemPlanStep:
    gem_name: str
    level_name: str
    col: int
    row: int
    repeats: int


@dataclass
class MatchResult:
    top_left: tuple[int, int]
    width: int
    height: int
    score: float


class GemSummingSession:
    GEM_NAMES = ["diamond", "emerald", "ruby", "topaz", "amethyst", "sapphire", "skull"]
    LEVEL_NAMES = ["chipped", "flawed", "normal", "flawless", "perfect"]
    STOP_HOTKEY = "f10"

    REFERENCE_COUNTS = [
        [23, 18, 8, 12, 15, 21, 8],
        [27, 47, 24, 38, 7, 37, 26],
        [23, 27, 11, 21, 5, 19, 14],
        [7, 18, 10, 16, 8, 12, 12],
        [19, 17, 48, 10, 11, 15, 11],
    ]

    STASH_TEMPLATE_PATH = Path("assets/stash/stash_open_gems_focused.png")
    CUBE_BUTTON_TEMPLATE_PATH = Path("assets/items/horadric_cube/horadric_cube_button.png")
    CUBE_BUTTON_HOVER_TEMPLATE_PATH = Path("assets/items/horadric_cube/horadric_cube_button_when_hover.png")
    GEM_ICON_DIR = Path("assets/items/gems/icons")

    MATCH_THRESHOLD = 0.82
    BUTTON_MATCH_THRESHOLD = 0.78
    USER_INTERRUPT_DISTANCE = 80

    # Geometry inside the stash reference image.
    SLOT_START_X = 82
    SLOT_START_Y = 117
    SLOT_STEP_X = 61
    SLOT_STEP_Y = 50
    SLOT_SIZE = 46

    COUNT_OFFSET_X = 18
    COUNT_OFFSET_Y = 28
    COUNT_WIDTH = 28
    COUNT_HEIGHT = 18
    LEFT_DIGIT_WIDTH = 14
    RIGHT_DIGIT_START = 14

    GEM_TAB_POINT = (260, 91)
    SAFE_PARK_POINT = (455, 610)

    CUBE_GRID_START_X = 216
    CUBE_GRID_START_Y = 396
    CUBE_CELL_STEP_X = 49
    CUBE_CELL_STEP_Y = 49
    CUBE_CELL_WIDTH = 46
    CUBE_CELL_HEIGHT = 45
    ICON_MARGIN = 6

    PREPROCESS_VARIANTS = (
        ("thr_110", 110),
        ("thr_120", 120),
        ("thr_130", 130),
        ("thr_140", 140),
        ("thr_150", 150),
        ("otsu", None),
    )

    TENS_MARGIN_SCORE = 0.12
    SLOT_ICON_MIN_SCORE = 0.42
    TARGET_JITTER = 3
    EXPECTED_RESULT_MIN_SCORE = 0.56
    EXPECTED_RESULT_MARGIN = 0.02
    COUNT_SCAN_SAMPLES = 3
    COUNT_SCAN_SAMPLE_SLEEP = (0.02, 0.05)
    RESYNC_EVERY_SUCCESS_STEPS = 3
    RESYNC_MAX_DELTA_WITH_TRACKED = 6

    MOVE_STEPS = (2, 3)
    MOVE_SLEEP = (0.016, 0.038)
    CLICK_SETTLE = (0.05, 0.09)
    ACTION_SLEEP = (0.12, 0.22)
    TRANSMUTE_SLEEP = (0.16, 0.26)
    FOCUS_SLEEP = (0.12, 0.20)
    UI_SETTLE_SLEEP = (0.10, 0.18)

    def __init__(self, capture_config: CaptureConfig):
        self.capture_config = capture_config
        self.events: Queue[GemActionEvent] = Queue()
        self._thread: threading.Thread | None = None
        self._stop_event = threading.Event()
        self._is_running = False
        self._lock = threading.Lock()
        self._hotkey_handle = None
        self._last_pointer: tuple[int, int] | None = None
        self._blocked_slots: set[tuple[int, int]] = set()

        self._stash_template = self._load_image(self.STASH_TEMPLATE_PATH)
        self._cube_button_template = self._load_image(self.CUBE_BUTTON_TEMPLATE_PATH)
        self._cube_button_hover_template = self._load_image(self.CUBE_BUTTON_HOVER_TEMPLATE_PATH)
        self._gem_icon_templates = self._load_gem_icon_templates()
        self._ones_templates, self._tens_templates, self._blank_left_templates = self._build_digit_templates(self._stash_template)

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._is_running

    def scan_counts(self) -> list[list[int]]:
        capture = ScreenCapture(self.capture_config)
        packet = capture.grab()
        stash_match = self._locate_template(packet.frame, self._stash_template, self.MATCH_THRESHOLD)
        if stash_match is None:
            raise RuntimeError("Could not find the opened gem stash panel. Open stash + cube, then try again.")
        return self._read_consensus_counts(capture, stash_match)

    def _focus_game_window(self, capture: ScreenCapture) -> None:
        window = resolve_window_from_config(self.capture_config)
        if window is None:
            return
        focused = focus_window(window)
        if not focused:
            raise RuntimeError("Could not bring the Diablo window to the foreground from the control panel.")
        center_x = window.left + window.width // 2
        center_y = window.top + window.height // 2
        self._move_absolute(capture, center_x, center_y)
        self._last_pointer = (center_x, center_y)
        self._sleep_range(*self.FOCUS_SLEEP)

    def start(self) -> None:
        with self._lock:
            if self._is_running:
                raise RuntimeError("Gem summing is already running.")
            self._stop_event.clear()
            self._thread = threading.Thread(target=self._run, daemon=True)
            self._is_running = True
            self._thread.start()
        self.events.put(GemActionEvent("info", f"Gem summing started. Press {self.STOP_HOTKEY.upper()} to stop."))

    def stop(self) -> None:
        self._stop_event.set()
        thread = self._thread
        if thread is not None:
            thread.join(timeout=3.0)
        with self._lock:
            self._thread = None
            self._is_running = False
        self.events.put(GemActionEvent("info", "Gem summing stopped."))

    def request_stop(self) -> None:
        self._stop_event.set()

    def _run(self) -> None:
        try:
            if pydirectinput is None:
                raise RuntimeError("pydirectinput is required for gem summing.")

            capture = ScreenCapture(self.capture_config)
            self._bind_hotkey()
            self._blocked_slots.clear()
            self._focus_game_window(capture)

            initial_packet = capture.grab()
            target = capture.target
            stash_match = self._locate_template(initial_packet.frame, self._stash_template, self.MATCH_THRESHOLD)
            if stash_match is None:
                raise RuntimeError("Could not find the opened gem stash panel in the starting screen. Open stash + cube first, then start again.")

            cube_button_match = self._locate_best_template(
                initial_packet.frame,
                [self._cube_button_hover_template, self._cube_button_template],
                self.BUTTON_MATCH_THRESHOLD,
            )
            if cube_button_match is None:
                raise RuntimeError("Could not find the Horadric Cube transmute button in the starting screen. Open stash + cube first, then start again.")

            self._click_relative(capture, stash_match, *self.GEM_TAB_POINT)
            self._sleep_range(*self.UI_SETTLE_SLEEP)

            tracked_counts: list[list[int]] | None = None
            needs_resync = True
            successful_steps_since_resync = 0

            while True:
                self._park_cursor(capture, stash_match)
                self._sleep_range(*self.UI_SETTLE_SLEEP)
                self._restash_all_cube_items(capture, stash_match)
                self._park_cursor(capture, stash_match)
                self._sleep_range(*self.UI_SETTLE_SLEEP)
                current_packet = capture.grab()
                stash_match = self._locate_template(current_packet.frame, self._stash_template, self.MATCH_THRESHOLD)
                if stash_match is None:
                    raise RuntimeError("Lost the opened gem stash panel during gem summing.")
                cube_button_match = self._locate_best_template(
                    current_packet.frame,
                    [self._cube_button_hover_template, self._cube_button_template],
                    self.BUTTON_MATCH_THRESHOLD,
                )
                if cube_button_match is None:
                    raise RuntimeError("Lost the Horadric Cube transmute button during gem summing.")

                if needs_resync or tracked_counts is None or successful_steps_since_resync >= self.RESYNC_EVERY_SUCCESS_STEPS:
                    tracked_counts = self._read_consensus_counts(capture, stash_match, fallback_counts=tracked_counts)
                    needs_resync = False
                    successful_steps_since_resync = 0

                plan = self._build_plan(tracked_counts)
                if not plan:
                    self.events.put(GemActionEvent("info", "All non-perfect gem stacks are now below 10."))
                    return

                step = plan[0]
                self.events.put(
                    GemActionEvent(
                        "info",
                        f"{step.gem_name} {step.level_name}: running {step.repeats} combine(s) from tracked counts {tracked_counts[step.row][step.col]}",
                    )
                )

                step_completed = True
                for repeat_index in range(step.repeats):
                    if self._check_for_user_interrupt():
                        self.events.put(GemActionEvent("warning", "Gem summing interrupted by user."))
                        return

                    slot_point = self._slot_center(target, stash_match, step.col, step.row)
                    self._ctrl_shift_click_absolute(capture, *slot_point, button="right")
                    self._sleep_range(*self.ACTION_SLEEP)
                    self._click_match_center(capture, cube_button_match)
                    self._sleep_range(*self.TRANSMUTE_SLEEP)
                    occupied_after_transmute = self._detect_occupied_cube_cells(capture, stash_match)
                    if len(occupied_after_transmute) != 1:
                        self.events.put(
                            GemActionEvent(
                                "warning",
                                f"{step.gem_name} {step.level_name}: cube had {len(occupied_after_transmute)} item(s) after transmute on combine {repeat_index + 1}/{step.repeats}; skipping this slot.",
                            )
                        )
                        self._blocked_slots.add((step.col, step.row))
                        self._restash_all_cube_items(capture, stash_match)
                        step_completed = False
                        break

                    result_point = occupied_after_transmute[0]
                    result_icon = self._cube_result_icon_crop(capture, stash_match, result_point)
                    if not self._is_expected_result(step.gem_name, step.row + 1, result_icon):
                        self.events.put(
                            GemActionEvent(
                                "warning",
                                f"{step.gem_name} {step.level_name}: result did not match the expected next gem on combine {repeat_index + 1}/{step.repeats}; skipping this slot.",
                            )
                        )
                        self._blocked_slots.add((step.col, step.row))
                        self._restash_all_cube_items(capture, stash_match)
                        step_completed = False
                        break

                    self._ctrl_shift_click_absolute(capture, *result_point, button="left")
                    self._sleep_range(*self.ACTION_SLEEP)
                    self._restash_all_cube_items(capture, stash_match)
                    tracked_counts = self._apply_successful_combine(tracked_counts, step.row, step.col)

                needs_resync = not step_completed
                if step_completed:
                    successful_steps_since_resync += 1
                    self.events.put(
                        GemActionEvent(
                            "info",
                            f"Tracked counts after {step.gem_name} {step.level_name}: {tracked_counts}",
                        )
                    )
        except Exception as exc:  # pragma: no cover
            self.events.put(GemActionEvent("error", f"Gem summing failed: {exc}"))
        finally:
            self._unbind_hotkey()
            with self._lock:
                self._is_running = False

    def _load_image(self, path: Path) -> np.ndarray:
        if not path.exists():
            raise RuntimeError(f"Required asset is missing: {path.as_posix()}")
        data = np.fromfile(path, dtype=np.uint8)
        image = cv.imdecode(data, cv.IMREAD_COLOR)
        if image is None:
            raise RuntimeError(f"Failed to load asset: {path.as_posix()}")
        return image

    def _load_gem_icon_templates(self) -> dict[str, list[np.ndarray]]:
        templates: dict[str, list[np.ndarray]] = {}
        for gem_name in self.GEM_NAMES:
            level_templates: list[np.ndarray] = []
            for level_name in self.LEVEL_NAMES:
                path = self.GEM_ICON_DIR / f"{level_name}_{gem_name}.png"
                level_templates.append(self._load_image(path))
            templates[gem_name] = level_templates
        return templates

    def _build_digit_templates(
        self,
        reference_image: np.ndarray,
    ) -> tuple[dict[str, dict[str, list[np.ndarray]]], dict[str, dict[str, list[np.ndarray]]], dict[str, list[np.ndarray]]]:
        ones_templates: dict[str, dict[str, list[np.ndarray]]] = {}
        tens_templates: dict[str, dict[str, list[np.ndarray]]] = {}
        blank_left_templates: dict[str, list[np.ndarray]] = {}

        for variant_name, _ in self.PREPROCESS_VARIANTS:
            ones_templates[variant_name] = {}
            tens_templates[variant_name] = {}
            blank_left_templates[variant_name] = []

        for row in range(5):
            for col in range(7):
                count = str(self.REFERENCE_COUNTS[row][col])
                crop = self._count_crop(reference_image, row, col)
                processed_variants = self._preprocess_count_crop(crop)
                for variant_name, processed in processed_variants.items():
                    ones_templates[variant_name].setdefault(count[-1], []).append(
                        self._normalize_digit(processed[:, self.RIGHT_DIGIT_START :])
                    )
                    left_crop = self._normalize_digit(processed[:, : self.LEFT_DIGIT_WIDTH])
                    if len(count) == 2:
                        tens_templates[variant_name].setdefault(count[0], []).append(left_crop)
                    else:
                        blank_left_templates[variant_name].append(left_crop)

        return ones_templates, tens_templates, blank_left_templates

    def _read_consensus_counts(
        self,
        capture: ScreenCapture,
        stash_match: MatchResult,
        fallback_counts: list[list[int]] | None = None,
    ) -> list[list[int]]:
        weighted_votes = [[defaultdict(float) for _ in range(len(self.GEM_NAMES))] for _ in range(len(self.LEVEL_NAMES))]
        first_counts: list[list[int]] | None = None
        invalid_slots: list[tuple[int, int]] = []

        for sample_index in range(self.COUNT_SCAN_SAMPLES):
            packet = capture.grab()
            counts, confidences, icon_scores = self._read_frame_counts(packet.frame, stash_match)
            if first_counts is None:
                first_counts = counts
            for row in range(len(self.LEVEL_NAMES)):
                for col in range(len(self.GEM_NAMES)):
                    if icon_scores[row][col] < self.SLOT_ICON_MIN_SCORE:
                        continue
                    weighted_votes[row][col][counts[row][col]] += icon_scores[row][col] + confidences[row][col]
            if sample_index + 1 < self.COUNT_SCAN_SAMPLES:
                self._sleep_range(*self.COUNT_SCAN_SAMPLE_SLEEP)

        assert first_counts is not None
        resolved = [[0 for _ in range(len(self.GEM_NAMES))] for _ in range(len(self.LEVEL_NAMES))]
        for row in range(len(self.LEVEL_NAMES)):
            for col in range(len(self.GEM_NAMES)):
                votes = weighted_votes[row][col]
                if votes:
                    resolved[row][col] = max(votes, key=votes.get)
                    continue
                invalid_slots.append((row, col))
                if fallback_counts is not None:
                    resolved[row][col] = fallback_counts[row][col]
                else:
                    resolved[row][col] = first_counts[row][col]

        if fallback_counts is not None:
            corrected_slots: list[tuple[int, int, int, int]] = []
            for row in range(len(self.LEVEL_NAMES)):
                for col in range(len(self.GEM_NAMES)):
                    tracked_value = fallback_counts[row][col]
                    reread_value = resolved[row][col]
                    if abs(reread_value - tracked_value) > self.RESYNC_MAX_DELTA_WITH_TRACKED:
                        resolved[row][col] = tracked_value
                        corrected_slots.append((row, col, tracked_value, reread_value))
            if corrected_slots:
                labels = ", ".join(
                    f"{self.LEVEL_NAMES[row]} {self.GEM_NAMES[col]} {old}->{new}"
                    for row, col, old, new in corrected_slots
                )
                self.events.put(GemActionEvent("warning", f"Count resync kept tracked values on {len(corrected_slots)} slot(s): {labels}"))

        if invalid_slots:
            labels = ", ".join(f"{self.LEVEL_NAMES[row]} {self.GEM_NAMES[col]}" for row, col in invalid_slots)
            self.events.put(GemActionEvent("warning", f"Count scan fell back on {len(invalid_slots)} slot(s): {labels}"))
        self.events.put(GemActionEvent("info", f"Detected gem counts (consensus): {resolved}"))
        return resolved

    def _read_frame_counts(
        self,
        frame: np.ndarray,
        stash_match: MatchResult,
    ) -> tuple[list[list[int]], list[list[float]], list[list[float]]]:
        stash_frame = self._extract_region(frame, stash_match)
        counts: list[list[int]] = []
        confidences: list[list[float]] = []
        icon_scores: list[list[float]] = []
        for row in range(len(self.LEVEL_NAMES)):
            row_counts: list[int] = []
            row_confidences: list[float] = []
            row_icon_scores: list[float] = []
            for col in range(len(self.GEM_NAMES)):
                slot_icon = self._slot_icon_from_stash_frame(stash_frame, row, col)
                row_icon_scores.append(self._slot_icon_score(row, col, slot_icon))
                crop = self._count_crop(stash_frame, row, col)
                value, confidence = self._read_count_value(crop)
                row_counts.append(value)
                row_confidences.append(confidence)
            counts.append(row_counts)
            confidences.append(row_confidences)
            icon_scores.append(row_icon_scores)
        return counts, confidences, icon_scores

    def _read_count_value(self, crop: np.ndarray) -> tuple[int, float]:
        processed_variants = self._preprocess_count_crop(crop)
        ones_digit, ones_score, ones_scores = self._classify_digit_variants(processed_variants, self._ones_templates, self.RIGHT_DIGIT_START)
        tens_digit, tens_score, tens_scores = self._classify_digit_variants(processed_variants, self._tens_templates, 0, self.LEFT_DIGIT_WIDTH)
        blank_score = self._classify_blank_left(processed_variants)
        tens_present = (
            tens_score > blank_score + self.TENS_MARGIN_SCORE
            and (tens_score - self._second_best_score(tens_scores)) >= 0.02
        )
        value = int(f"{tens_digit}{ones_digit}") if tens_present else int(ones_digit)
        ones_margin = ones_score - self._second_best_score(ones_scores)
        left_margin = (tens_score - max(blank_score, self._second_best_score(tens_scores))) if tens_present else (blank_score - tens_score)
        confidence = max(0.01, ones_margin + left_margin)
        return value, confidence

    def _build_plan(self, counts: list[list[int]]) -> list[GemPlanStep]:
        working = [row[:] for row in counts]
        plan: list[GemPlanStep] = []
        for col, gem_name in enumerate(self.GEM_NAMES):
            for row in range(4):
                if (col, row) in self._blocked_slots:
                    continue
                count = working[row][col]
                repeats = max(0, math.ceil((count - 9) / 3))
                if repeats <= 0:
                    continue
                working[row][col] -= repeats * 3
                working[row + 1][col] += repeats
                plan.append(
                    GemPlanStep(
                        gem_name=gem_name,
                        level_name=self.LEVEL_NAMES[row],
                        col=col,
                        row=row,
                        repeats=repeats,
                    )
                )
        return plan

    def _apply_successful_combine(self, counts: list[list[int]], row: int, col: int) -> list[list[int]]:
        updated = [current_row[:] for current_row in counts]
        updated[row][col] = max(0, updated[row][col] - 3)
        updated[row + 1][col] += 1
        return updated

    def _locate_best_template(
        self,
        frame: np.ndarray,
        templates: list[np.ndarray],
        threshold: float,
    ) -> MatchResult | None:
        best: MatchResult | None = None
        for template in templates:
            result = self._locate_template(frame, template, threshold)
            if result is None:
                continue
            if best is None or result.score > best.score:
                best = result
        return best

    def _locate_template(self, frame: np.ndarray, template: np.ndarray, threshold: float) -> MatchResult | None:
        result = cv.matchTemplate(frame, template, cv.TM_CCOEFF_NORMED)
        _, max_value, _, max_loc = cv.minMaxLoc(result)
        if max_value < threshold:
            return None
        return MatchResult(top_left=max_loc, width=template.shape[1], height=template.shape[0], score=float(max_value))

    def _extract_region(self, frame: np.ndarray, match: MatchResult) -> np.ndarray:
        x, y = match.top_left
        return frame[y : y + match.height, x : x + match.width].copy()

    def _count_crop(self, stash_frame: np.ndarray, row: int, col: int) -> np.ndarray:
        slot_x = self.SLOT_START_X + col * self.SLOT_STEP_X
        slot_y = self.SLOT_START_Y + row * self.SLOT_STEP_Y
        start_x = slot_x + self.COUNT_OFFSET_X
        start_y = slot_y + self.COUNT_OFFSET_Y
        return stash_frame[start_y : start_y + self.COUNT_HEIGHT, start_x : start_x + self.COUNT_WIDTH].copy()

    def _preprocess_count_crop(self, crop: np.ndarray) -> dict[str, np.ndarray]:
        gray = cv.cvtColor(crop, cv.COLOR_BGR2GRAY)
        gray = cv.GaussianBlur(gray, (3, 3), 0)
        variants: dict[str, np.ndarray] = {}
        kernel = np.ones((2, 2), np.uint8)
        for variant_name, threshold_value in self.PREPROCESS_VARIANTS:
            if threshold_value is None:
                _, thresholded = cv.threshold(gray, 0, 255, cv.THRESH_BINARY + cv.THRESH_OTSU)
            else:
                _, thresholded = cv.threshold(gray, threshold_value, 255, cv.THRESH_BINARY)
            variants[variant_name] = cv.morphologyEx(thresholded, cv.MORPH_CLOSE, kernel)
        return variants

    def _normalize_digit(self, crop: np.ndarray) -> np.ndarray:
        return cv.resize(crop, (12, 14), interpolation=cv.INTER_NEAREST)

    def _classify_digit_variants(
        self,
        processed_variants: dict[str, np.ndarray],
        bank: dict[str, dict[str, list[np.ndarray]]],
        start_x: int,
        end_x: int | None = None,
    ) -> tuple[str, float, dict[str, float]]:
        aggregate: dict[str, float] = {}
        end_x = end_x if end_x is not None else self.COUNT_WIDTH
        for variant_name, processed in processed_variants.items():
            crop = processed[:, start_x:end_x]
            normalized = self._normalize_digit(crop)
            for digit, templates in bank[variant_name].items():
                digit_score = max(
                    1.0 - float(np.mean(np.abs(normalized.astype(np.float32) - template.astype(np.float32))) / 255.0)
                    for template in templates
                )
                aggregate[digit] = aggregate.get(digit, 0.0) + digit_score
        best_digit = max(aggregate, key=aggregate.get)
        return best_digit, aggregate[best_digit], aggregate

    def _slot_icon_crop(self, frame: np.ndarray, stash_match: MatchResult, row: int, col: int) -> np.ndarray:
        stash_frame = self._extract_region(frame, stash_match)
        return self._slot_icon_from_stash_frame(stash_frame, row, col)

    def _slot_icon_from_stash_frame(self, stash_frame: np.ndarray, row: int, col: int) -> np.ndarray:
        slot_x = self.SLOT_START_X + col * self.SLOT_STEP_X
        slot_y = self.SLOT_START_Y + row * self.SLOT_STEP_Y
        return self._icon_crop(stash_frame[slot_y : slot_y + self.SLOT_SIZE, slot_x : slot_x + self.SLOT_SIZE])

    def _cube_result_icon_crop(self, capture: ScreenCapture, stash_match: MatchResult, result_point: tuple[int, int]) -> np.ndarray:
        packet = capture.grab()
        stash_frame = self._extract_region(packet.frame, stash_match)
        local_x = result_point[0] - capture.target["left"] - stash_match.top_left[0]
        local_y = result_point[1] - capture.target["top"] - stash_match.top_left[1]
        start_x = int(local_x - self.CUBE_CELL_WIDTH // 2)
        start_y = int(local_y - self.CUBE_CELL_HEIGHT // 2)
        cell = stash_frame[start_y : start_y + self.CUBE_CELL_HEIGHT, start_x : start_x + self.CUBE_CELL_WIDTH]
        return self._icon_crop(cell)

    def _icon_crop(self, cell: np.ndarray) -> np.ndarray:
        margin = self.ICON_MARGIN
        crop = cell[margin : cell.shape[0] - margin, margin : cell.shape[1] - margin]
        return cv.resize(crop, (28, 28), interpolation=cv.INTER_AREA)

    def _icon_similarity(self, a: np.ndarray, b: np.ndarray) -> float:
        a_gray = cv.cvtColor(a, cv.COLOR_BGR2GRAY)
        b_gray = cv.cvtColor(b, cv.COLOR_BGR2GRAY)
        return float(cv.matchTemplate(a_gray, b_gray, cv.TM_CCOEFF_NORMED)[0][0])

    def _slot_icon_score(self, row: int, col: int, slot_icon: np.ndarray) -> float:
        expected_icon = self._gem_icon_templates[self.GEM_NAMES[col]][row]
        expected_icon = cv.resize(expected_icon, (slot_icon.shape[1], slot_icon.shape[0]), interpolation=cv.INTER_AREA)
        return self._icon_similarity(slot_icon, expected_icon)

    def _is_expected_result(self, gem_name: str, level_index: int, result_icon: np.ndarray) -> bool:
        expected_icon = self._gem_icon_templates[gem_name][level_index]
        expected_icon = cv.resize(expected_icon, (result_icon.shape[1], result_icon.shape[0]), interpolation=cv.INTER_AREA)
        expected_score = self._icon_similarity(result_icon, expected_icon)
        if level_index == 0:
            return expected_score >= self.EXPECTED_RESULT_MIN_SCORE
        previous_icon = self._gem_icon_templates[gem_name][level_index - 1]
        previous_icon = cv.resize(previous_icon, (result_icon.shape[1], result_icon.shape[0]), interpolation=cv.INTER_AREA)
        previous_score = self._icon_similarity(result_icon, previous_icon)
        return expected_score >= max(self.EXPECTED_RESULT_MIN_SCORE, previous_score + self.EXPECTED_RESULT_MARGIN)

    def _restash_all_cube_items(self, capture: ScreenCapture, stash_match: MatchResult) -> None:
        for _ in range(6):
            occupied = self._detect_occupied_cube_cells(capture, stash_match)
            if not occupied:
                return
            for point in occupied:
                if self._check_for_user_interrupt():
                    return
                self._ctrl_shift_click_absolute(capture, *point, button="left")
                self._sleep_range(*self.ACTION_SLEEP)
        self.events.put(GemActionEvent("warning", "Cube cleanup left items behind after several attempts."))

    def _detect_occupied_cube_cells(self, capture: ScreenCapture, stash_match: MatchResult) -> list[tuple[int, int]]:
        packet = capture.grab()
        stash_frame = self._extract_region(packet.frame, stash_match)
        occupied: list[tuple[int, int, float]] = []
        for row in range(4):
            for col in range(3):
                cell = self._cube_cell_crop(stash_frame, row, col)
                score = self._cube_cell_occupancy_score(cell)
                if score >= 18.0:
                    occupied.append((*self._cube_cell_center(capture.target, stash_match, row, col), score))
        occupied.sort(key=lambda item: item[2], reverse=True)
        return [(x, y) for x, y, _ in occupied]

    def _detect_cube_result_point(self, capture: ScreenCapture, stash_match: MatchResult) -> tuple[int, int] | None:
        occupied = self._detect_occupied_cube_cells(capture, stash_match)
        if not occupied:
            return None
        return occupied[0]

    def _cube_cell_crop(self, stash_frame: np.ndarray, row: int, col: int) -> np.ndarray:
        start_x = self.CUBE_GRID_START_X + col * self.CUBE_CELL_STEP_X
        start_y = self.CUBE_GRID_START_Y + row * self.CUBE_CELL_STEP_Y
        return stash_frame[start_y : start_y + self.CUBE_CELL_HEIGHT, start_x : start_x + self.CUBE_CELL_WIDTH].copy()

    def _cube_cell_occupancy_score(self, cell: np.ndarray) -> float:
        gray = cv.cvtColor(cell, cv.COLOR_BGR2GRAY)
        return float(np.mean(gray)) + float(np.std(gray))

    def _cube_cell_center(self, target: dict[str, int], stash_match: MatchResult, row: int, col: int) -> tuple[int, int]:
        x = target["left"] + stash_match.top_left[0] + self.CUBE_GRID_START_X + col * self.CUBE_CELL_STEP_X + self.CUBE_CELL_WIDTH // 2
        y = target["top"] + stash_match.top_left[1] + self.CUBE_GRID_START_Y + row * self.CUBE_CELL_STEP_Y + self.CUBE_CELL_HEIGHT // 2
        return x, y

    def _classify_blank_left(self, processed_variants: dict[str, np.ndarray]) -> float:
        total_score = 0.0
        for variant_name, processed in processed_variants.items():
            crop = processed[:, : self.LEFT_DIGIT_WIDTH]
            normalized = self._normalize_digit(crop)
            variant_score = max(
                1.0 - float(np.mean(np.abs(normalized.astype(np.float32) - template.astype(np.float32))) / 255.0)
                for template in self._blank_left_templates[variant_name]
            )
            total_score += variant_score
        return total_score

    def _second_best_score(self, scores: dict[str, float]) -> float:
        ordered = sorted(scores.values(), reverse=True)
        if len(ordered) < 2:
            return -1.0
        return ordered[1]

    def _slot_center(self, target: dict[str, int], stash_match: MatchResult, col: int, row: int) -> tuple[int, int]:
        x = target["left"] + stash_match.top_left[0] + self.SLOT_START_X + col * self.SLOT_STEP_X + self.SLOT_SIZE // 2
        y = target["top"] + stash_match.top_left[1] + self.SLOT_START_Y + row * self.SLOT_STEP_Y + self.SLOT_SIZE // 2
        return x, y

    def _relative_point(self, target: dict[str, int], stash_match: MatchResult, point_x: int, point_y: int) -> tuple[int, int]:
        return target["left"] + stash_match.top_left[0] + point_x, target["top"] + stash_match.top_left[1] + point_y

    def _click_relative(self, capture: ScreenCapture, stash_match: MatchResult, point_x: int, point_y: int) -> None:
        abs_x, abs_y = self._relative_point(capture.target, stash_match, point_x, point_y)
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click()
        self._last_pointer = (abs_x, abs_y)

    def _ctrl_shift_click_relative(
        self,
        capture: ScreenCapture,
        stash_match: MatchResult,
        point_x: int,
        point_y: int,
        button: str,
    ) -> None:
        abs_x, abs_y = self._relative_point(capture.target, stash_match, point_x, point_y)
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.keyDown("ctrl")
        pydirectinput.keyDown("shift")
        try:
            pydirectinput.click(button=button)
        finally:
            pydirectinput.keyUp("shift")
            pydirectinput.keyUp("ctrl")
        self._last_pointer = (abs_x, abs_y)

    def _ctrl_shift_click_absolute(self, capture: ScreenCapture, abs_x: int, abs_y: int, button: str) -> None:
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.keyDown("ctrl")
        pydirectinput.keyDown("shift")
        try:
            pydirectinput.click(button=button)
        finally:
            pydirectinput.keyUp("shift")
            pydirectinput.keyUp("ctrl")
        self._last_pointer = (abs_x, abs_y)

    def _click_match_center(self, capture: ScreenCapture, match: MatchResult) -> None:
        abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
        abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
        abs_x, abs_y = self._jitter_point(abs_x, abs_y, self.TARGET_JITTER)
        self._move_absolute(capture, abs_x, abs_y)
        self._sleep_range(*self.CLICK_SETTLE)
        pydirectinput.click()
        self._last_pointer = (abs_x, abs_y)

    def _jitter_point(self, point_x: int, point_y: int, radius: int) -> tuple[int, int]:
        if radius <= 0:
            return point_x, point_y
        return point_x + random.randint(-radius, radius), point_y + random.randint(-radius, radius)

    def _park_cursor(self, capture: ScreenCapture, stash_match: MatchResult) -> None:
        abs_x, abs_y = self._relative_point(capture.target, stash_match, *self.SAFE_PARK_POINT)
        self._move_absolute(capture, abs_x, abs_y)

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