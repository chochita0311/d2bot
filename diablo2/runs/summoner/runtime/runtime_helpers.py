from __future__ import annotations

import random
import time
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.capture import focus_window, resolve_window_from_config
from diablo2.common.controller import keyboard, pydirectinput


def locate_waypoint_panel(session, capture, template: np.ndarray):
    packet = capture.grab()
    return locate_template(session, packet.frame, template, session.LIST_PANEL_THRESHOLD)


def wait_for_waypoint_panel(session, capture, template: np.ndarray, threshold: float, timeout_seconds: float, label: str):
    end_time = time.time() + timeout_seconds
    best_match = None
    while time.time() < end_time:
        if check_for_user_interrupt(session):
            raise RuntimeError("stopped by user interference")
        packet = capture.grab()
        match = locate_template(session, packet.frame, template, threshold)
        if match is not None:
            return match
        fallback = locate_template(session, packet.frame, template, 0.0)
        if fallback is not None and (best_match is None or fallback.score > best_match.score):
            best_match = fallback
        time.sleep(0.02)
    best_score = -1.0 if best_match is None else best_match.score
    raise RuntimeError(f"Timed out waiting for {label}. Best match score was {best_score:.3f}.")


def click_panel_ratio(session, capture, panel_match, ratio: tuple[float, float]) -> None:
    point_x = int(panel_match.width * ratio[0])
    point_y = int(panel_match.height * ratio[1])
    click_panel_point(session, capture, panel_match, (point_x, point_y))


def click_panel_point(session, capture, panel_match, point: tuple[int, int]) -> None:
    abs_x = capture.target["left"] + panel_match.top_left[0] + point[0]
    abs_y = capture.target["top"] + panel_match.top_left[1] + point[1]
    move_absolute(session, capture, abs_x, abs_y)
    sleep_range(session, *session.CLICK_SETTLE)
    pydirectinput.click(button="left")
    session._last_pointer = (abs_x, abs_y)
    sleep_range(session, *session.ACTION_SLEEP)


def center_right_anchor(session) -> tuple[float, float]:
    return apply_offset((0.5, 0.5), session.WORLD_SCOUT_ANCHOR_OFFSET)


def apply_offset(point: tuple[float, float], offset: tuple[float, float]) -> tuple[float, float]:
    x = min(0.95, max(0.05, point[0] + offset[0]))
    y = min(0.95, max(0.05, point[1] + offset[1]))
    return x, y


def approach_waypoint(session, capture, match) -> None:
    move_to_match_center(session, capture, match)


def focus_game_window(session, capture) -> None:
    window = resolve_window_from_config(session.config.capture)
    if window is None:
        return
    if not focus_window(window):
        raise RuntimeError("Could not bring the Diablo window to the foreground from the control panel.")
    center_x = window.left + window.width // 2
    center_y = window.top + window.height // 2
    move_absolute(session, capture, center_x, center_y)
    session._last_pointer = (center_x, center_y)
    sleep_range(session, *session.FOCUS_SLEEP)


def load_optional_image(session, path: Path, label: str):
    try:
        return load_image(path)
    except RuntimeError as exc:
        session.events.put(session.event_class("warning", f"{label} template skipped: {exc}"))
        return None


def load_image(path: Path) -> np.ndarray:
    if not path.exists():
        raise RuntimeError(f"Required asset is missing: {path.as_posix()}")
    raw = path.read_bytes()
    if raw.startswith(b"RIFF") and raw[8:12] == b"WEBP":
        raise RuntimeError(f"Unsupported WebP asset: {path.as_posix()}")
    data = np.frombuffer(raw, dtype=np.uint8)
    image = cv.imdecode(data, cv.IMREAD_COLOR)
    if image is None:
        raise RuntimeError(f"Failed to load asset: {path.as_posix()}")
    return image


def wait_for_any_template(session, capture, templates: list[np.ndarray], threshold: float, timeout_seconds: float, label: str):
    end_time = time.time() + timeout_seconds
    best_match = None
    while time.time() < end_time:
        if check_for_user_interrupt(session):
            raise RuntimeError("stopped by user interference")
        packet = capture.grab()
        current_best = locate_best_template(session, packet.frame, templates, threshold)
        if current_best is not None:
            return current_best
        fallback = locate_best_template(session, packet.frame, templates, 0.0)
        if fallback is not None and (best_match is None or fallback.score > best_match.score):
            best_match = fallback
        time.sleep(0.03)
    best_score = -1.0 if best_match is None else best_match.score
    raise RuntimeError(f"Timed out waiting for {label}. Best match score was {best_score:.3f}.")


def wait_for_optional_any_template(session, capture, templates: list[np.ndarray], threshold: float, timeout_seconds: float):
    end_time = time.time() + timeout_seconds
    while time.time() < end_time:
        if check_for_user_interrupt(session):
            raise RuntimeError("stopped by user interference")
        packet = capture.grab()
        match = locate_best_template(session, packet.frame, templates, threshold)
        if match is not None:
            return match
        time.sleep(0.02)
    return None


def wait_for_optional_template(session, capture, template: np.ndarray, threshold: float, timeout_seconds: float):
    end_time = time.time() + timeout_seconds
    while time.time() < end_time:
        if check_for_user_interrupt(session):
            raise RuntimeError("stopped by user interference")
        packet = capture.grab()
        match = locate_template(session, packet.frame, template, threshold)
        if match is not None:
            return match
        time.sleep(0.02)
    return None


def locate_current_waypoint_list(session, capture):
    packet = capture.grab()
    return locate_template(session, packet.frame, session._act1_list_template, session.LIST_THRESHOLD)


def locate_best_template(session, frame: np.ndarray, templates: list[np.ndarray], threshold: float):
    best = None
    for index, template in enumerate(templates):
        result = locate_template(session, frame, template, threshold)
        if result is None:
            continue
        result.source_index = index
        if best is None or result.score > best.score:
            best = result
    return best


def locate_template(session, frame: np.ndarray, template: np.ndarray, threshold: float):
    if frame.size == 0 or template.size == 0:
        return None
    if frame.shape[0] < template.shape[0] or frame.shape[1] < template.shape[1]:
        return None
    result = cv.matchTemplate(frame, template, cv.TM_CCOEFF_NORMED)
    _, max_value, _, max_loc = cv.minMaxLoc(result)
    if max_value < threshold:
        return None
    return session.match_result_class(top_left=max_loc, width=template.shape[1], height=template.shape[0], score=float(max_value))


def aim_relative_ratio(session, capture, ratio_x: float, ratio_y: float, apply_jitter: bool = True) -> None:
    abs_x = capture.target["left"] + int(capture.target["width"] * ratio_x)
    abs_y = capture.target["top"] + int(capture.target["height"] * ratio_y)
    if apply_jitter:
        abs_x, abs_y = jitter_point(session, abs_x, abs_y, session.TARGET_JITTER)
    move_absolute(session, capture, abs_x, abs_y)
    session._last_pointer = (abs_x, abs_y)


def click_relative_ratio(session, capture, ratio_x: float, ratio_y: float, apply_jitter: bool = True) -> None:
    abs_x = capture.target["left"] + int(capture.target["width"] * ratio_x)
    abs_y = capture.target["top"] + int(capture.target["height"] * ratio_y)
    if apply_jitter:
        abs_x, abs_y = jitter_point(session, abs_x, abs_y, session.TARGET_JITTER)
    move_absolute(session, capture, abs_x, abs_y)
    sleep_range(session, *session.CLICK_SETTLE)
    pydirectinput.click(button="left")
    session._last_pointer = (abs_x, abs_y)
    sleep_range(session, *session.ACTION_SLEEP)


def hold_move_relative_ratio(session, capture, ratio_x: float, ratio_y: float, hold_seconds: tuple[float, float], apply_jitter: bool = True) -> None:
    abs_x = capture.target["left"] + int(capture.target["width"] * ratio_x)
    abs_y = capture.target["top"] + int(capture.target["height"] * ratio_y)
    if apply_jitter:
        abs_x, abs_y = jitter_point(session, abs_x, abs_y, session.TARGET_JITTER)
    move_absolute(session, capture, abs_x, abs_y)
    sleep_range(session, *session.CLICK_SETTLE)
    pydirectinput.mouseDown(button="left")
    session._last_pointer = (abs_x, abs_y)
    sleep_range(session, *hold_seconds)
    pydirectinput.mouseUp(button="left")
    sleep_range(session, *session.ACTION_SLEEP)


def hold_move_until_waypoint(session, capture, ratio_x: float, ratio_y: float, hold_seconds: tuple[float, float]):
    abs_x = capture.target["left"] + int(capture.target["width"] * ratio_x)
    abs_y = capture.target["top"] + int(capture.target["height"] * ratio_y)
    move_absolute(session, capture, abs_x, abs_y)
    sleep_range(session, *session.CLICK_SETTLE)
    pydirectinput.mouseDown(button="left")
    session._last_pointer = (abs_x, abs_y)
    try:
        end_time = time.time() + random.uniform(*hold_seconds)
        while time.time() < end_time:
            if check_for_user_interrupt(session):
                raise RuntimeError("stopped by user interference")
            packet = capture.grab()
            match = locate_best_template(session, packet.frame, [session._act1_waypoint_hover_template, session._act1_waypoint_template], session.WAYPOINT_THRESHOLD)
            if match is not None:
                return match
            time.sleep(0.02)
        return None
    finally:
        pydirectinput.mouseUp(button="left")
        sleep_range(session, *session.ACTION_SLEEP)


def move_to_match_center(session, capture, match) -> None:
    abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
    abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
    abs_x, abs_y = jitter_point(session, abs_x, abs_y, session.TARGET_JITTER)
    move_absolute(session, capture, abs_x, abs_y)
    session._last_pointer = (abs_x, abs_y)


def click_match_center(session, capture, match) -> None:
    abs_x = capture.target["left"] + match.top_left[0] + match.width // 2
    abs_y = capture.target["top"] + match.top_left[1] + match.height // 2
    abs_x, abs_y = jitter_point(session, abs_x, abs_y, session.TARGET_JITTER)
    move_absolute(session, capture, abs_x, abs_y)
    sleep_range(session, *session.CLICK_SETTLE)
    pydirectinput.click(button="left")
    session._last_pointer = (abs_x, abs_y)
    sleep_range(session, *session.ACTION_SLEEP)


def press_key(session, key: str) -> None:
    if check_for_user_interrupt(session):
        raise RuntimeError("stopped by user interference")
    pydirectinput.press(key)
    sleep_range(session, *session.ACTION_SLEEP)


def hold_key_down(session, key: str) -> None:
    if check_for_user_interrupt(session):
        raise RuntimeError("stopped by user interference")
    pydirectinput.keyDown(key)
    sleep_range(session, *session.ACTION_SLEEP)


def hold_key_up(session, key: str) -> None:
    pydirectinput.keyUp(key)
    sleep_range(session, *session.ACTION_SLEEP)


def move_absolute(session, capture, target_x: int, target_y: int) -> None:
    if capture is not None:
        target = capture.target
        target_x = max(target["left"] + 2, min(target_x, target["left"] + target["width"] - 2))
        target_y = max(target["top"] + 2, min(target_y, target["top"] + target["height"] - 2))
    current_x, current_y = pydirectinput.position()
    steps = random.randint(*session.MOVE_STEPS)
    for step in range(1, steps + 1):
        if session._stop_event.is_set():
            return
        ratio = step / steps
        next_x = int(current_x + (target_x - current_x) * ratio)
        bend = random.randint(-8, 8)
        next_y = int(current_y + (target_y - current_y) * ratio + bend * (1 - abs(0.5 - ratio) * 2))
        pydirectinput.moveTo(next_x, next_y)
        session._last_pointer = (next_x, next_y)
        time.sleep(random.uniform(*session.MOVE_SLEEP))


def jitter_point(session, point_x: int, point_y: int, radius: int) -> tuple[int, int]:
    if radius <= 0:
        return point_x, point_y
    return point_x + random.randint(-radius, radius), point_y + random.randint(-radius, radius)


def check_for_user_interrupt(session) -> bool:
    if session._stop_event.is_set():
        return True
    if session._last_pointer is None:
        return False
    current = pydirectinput.position()
    if abs(current[0] - session._last_pointer[0]) > session.USER_INTERRUPT_DISTANCE or abs(current[1] - session._last_pointer[1]) > session.USER_INTERRUPT_DISTANCE:
        session._stop_event.set()
        return True
    return False


def sleep_range(session, low: float, high: float) -> None:
    end_time = time.time() + random.uniform(low, high)
    while time.time() < end_time:
        if session._stop_event.is_set():
            return
        time.sleep(0.01)


def bind_hotkey(session) -> None:
    if keyboard is None:
        return
    session._hotkey_handle = keyboard.add_hotkey(session.STOP_HOTKEY, session.request_stop)


def unbind_hotkey(session) -> None:
    if keyboard is None or session._hotkey_handle is None:
        return
    keyboard.remove_hotkey(session._hotkey_handle)
    session._hotkey_handle = None
