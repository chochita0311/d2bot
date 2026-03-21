from __future__ import annotations

import random

import cv2 as cv
import numpy as np

from diablo2.actions.loot_pickup import LootPickupSession
from diablo2.common.realtime import RealtimeVisionRuntime, RuntimeSnapshot
from diablo2.runs.base import RunRouteSegment
from diablo2.runs.summoner.routes.common.arcane_common import (
    ARCANE_BRANCH_FLOOR_PREFERENCE_THRESHOLD,
    ARCANE_BRANCH_SPAN_TRIM_TICKS,
    ARCANE_FINAL_NORTH_BEND_CURSOR_RATIO,
    ARCANE_FINAL_QUARTER_FLOOR_THRESHOLD,
    ARCANE_FINAL_STAGE_MIN_FOUR_OCLOCK_TICKS,
    ARCANE_FINAL_STAGE_TRAVEL_TICKS,
    ARCANE_FIRST_FORK_PROGRESS_THRESHOLD,
    ARCANE_FIRST_FORK_STAGNANT_LIMIT,
    ARCANE_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_FLOOR_SCORE_RADIUS,
    ARCANE_FOUR_OCLOCK_CURSOR_RATIO,
    ARCANE_FOUR_OCLOCK_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_MOVE_STEP_SETTLE,
    ARCANE_NEXT_PATH_CURSOR_RATIO,
    ARCANE_NORTH_CURSOR_RATIO,
    ARCANE_NORTH_TERMINAL_THRESHOLD,
    ARCANE_NORTH_TEST_TICK_SLEEP,
    ARCANE_NORTH_WAY_THRESHOLD,
    ARCANE_PROGRESS_CHANGE_THRESHOLD,
    ARCANE_PROGRESS_STAGNANT_LIMIT,
    ARCANE_RETURN_NORTH_CURSOR_RATIO,
    ARCANE_SECOND_FORK_PROGRESS_THRESHOLD,
    ARCANE_SECOND_FORK_STAGNANT_LIMIT,
    ARCANE_STAR_STAGNANT_LIMIT,
    ARCANE_STAR_THRESHOLD,
    ARCANE_SUMMONER_CLUE_THRESHOLD,
    ARCANE_TELEPORTER_HOVER_THRESHOLD,
    ARCANE_TELEPORTER_NUDGE_OFFSETS,
    ARCANE_WINGS,
    ARCANE_ZERO_POINT_CURSOR_RATIO,
)

ROUTE_SEGMENT = RunRouteSegment(
    key="north_go",
    label="north way go",
    direction="go",
    status="ready",
    notes=(
        "This is the only route piece currently exercised by the Arcane North Test button.",
        "Owns the threaded north-go controller and decision loop.",
    ),
)


def run_arcane_north_go(session, capture) -> None:
    actions = session._resolve_arcane_character_actions()
    if actions is None or not actions.movement_skill_key:
        raise RuntimeError("Arcane North Test requires a character with movement_skill_key configured.")

    session.events.put(
        session.event_class(
            "info",
            "Arcane North Test: assuming Arcane Sanctuary hub, item labels, and buffs are already ready; beginning north wing test.",
        )
    )
    state = session._build_initial_arcane_belief_state()
    north = next((wing for wing in ARCANE_WINGS if wing.key == "north"), None)
    if north is None:
        raise RuntimeError("Arcane North Test could not resolve the north wing definition.")
    session._commit_arcane_wing(state, north)
    session._log_arcane_state(state)

    loot_session = LootPickupSession(session.config)
    previous_fast_frame: np.ndarray | None = None

    class _TargetRef:
        def __init__(self, target: dict[str, int]):
            self.target = target

    control = {
        "north_steps": 0,
        "stagnant_ticks": 0,
        "star_ticks": 0,
        "first_fork_stagnant_ticks": 0,
        "second_fork_stagnant_ticks": 0,
        "path_stage": 0,
        "path_stage_move_ticks": 0,
        "effective_stage_move_ticks": 0,
        "observed_branch_span_ticks": None,
        "pending_stage_repress": False,
        "movement_key_held": False,
        "pause_until": 0.0,
        "last_frame_id": -1,
        "last_fast_payload": None,
        "last_slow_payload": None,
        "last_monster_log_at": 0.0,
        "last_loot_log_at": 0.0,
    }

    def _runtime_error(stage: str, exc: Exception) -> None:
        session.events.put(session.event_class("error", f"Arcane North Test {stage} thread failed: {exc}"))

    def _fast_vision(frame_packet) -> dict[str, object]:
        nonlocal previous_fast_frame
        current_frame = frame_packet.frame
        recent_frames = runtime.state.snapshot().recent_frames
        trend_change = _measure_arcane_progress_trend(recent_frames, control["path_stage"])
        payload = {
            "progress_change": _measure_arcane_progress_change(session, previous_fast_frame, current_frame, 0),
            "final_progress_change": _measure_arcane_progress_change(session, previous_fast_frame, current_frame, 3),
            "progress_trend": trend_change,
            "first_fork_change": _measure_arcane_first_fork_change(session, previous_fast_frame, current_frame),
            "second_fork_change": _measure_arcane_second_fork_change(session, previous_fast_frame, current_frame),
            "star_detected": _detect_arcane_dead_end_star(session, current_frame),
            "upper_right_floor_score": _arcane_upper_right_floor_score(session, current_frame),
        }
        previous_fast_frame = current_frame
        return payload

    def _slow_vision(frame_packet) -> dict[str, object]:
        loot_hit = loot_session.scan_frame(frame_packet.frame)
        return {
            "north_terminal": _detect_arcane_north_terminal(session, frame_packet.frame),
            "loot_label": loot_hit.label if loot_hit is not None else None,
            "monster_hit": session._scan_arcane_monsters(frame_packet.frame),
            "north_way_detected": _detect_arcane_final_north_way(session, frame_packet.frame),
            "teleporter_hover_detected": session._locate_template(
                frame_packet.frame,
                session._arcane_teleporter_hover_template,
                ARCANE_TELEPORTER_HOVER_THRESHOLD,
            )
            is not None,
        }

    def _decision(snapshot: RuntimeSnapshot) -> dict[str, object] | None:
        if session._check_for_user_interrupt():
            raise RuntimeError("stopped by user interference")
        latest_frame = snapshot.latest_frame
        if latest_frame is None or latest_frame.sequence_id == control["last_frame_id"]:
            return None
        control["last_frame_id"] = latest_frame.sequence_id

        now = snapshot.sampled_at
        fast_payload = control["last_fast_payload"]
        if snapshot.fast_vision is not None:
            fast_payload = snapshot.fast_vision.payload
            control["last_fast_payload"] = fast_payload
        slow_payload = control["last_slow_payload"]
        if snapshot.slow_vision is not None:
            slow_payload = snapshot.slow_vision.payload
            control["last_slow_payload"] = slow_payload

        frame_age_ms = int((now - latest_frame.captured_at) * 1000)
        fast_age_ms = int((now - snapshot.fast_vision.source_captured_at) * 1000) if snapshot.fast_vision is not None else None
        slow_age_ms = int((now - snapshot.slow_vision.source_captured_at) * 1000) if snapshot.slow_vision is not None else None

        if slow_payload is not None and slow_payload["north_terminal"] is not None:
            terminal = slow_payload["north_terminal"]
            if terminal == "summoner":
                session.events.put(
                    session.event_class("info", "Arcane North Test: detected north dead end with Summoner layout; stopping north run here.")
                )
            else:
                session.events.put(
                    session.event_class("info", "Arcane North Test: detected north dead end without Summoner; stopping north run here.")
                )
            session.request_stop()
            return {"status": "terminal", "frame_age_ms": frame_age_ms, "fast_age_ms": fast_age_ms, "slow_age_ms": slow_age_ms}

        if slow_payload is not None and slow_payload["monster_hit"] is not None:
            if now - control["last_monster_log_at"] >= 0.35:
                session.events.put(
                    session.event_class("info", f"Arcane North Test: monster detected during run -> {slow_payload['monster_hit']}.")
                )
                control["last_monster_log_at"] = now
            control["pause_until"] = max(control["pause_until"], now + random.uniform(*ARCANE_NORTH_TEST_TICK_SLEEP))
            return {"status": "pause_monster", "frame_age_ms": frame_age_ms, "fast_age_ms": fast_age_ms, "slow_age_ms": slow_age_ms}

        if slow_payload is not None and slow_payload["loot_label"] is not None:
            if now - control["last_loot_log_at"] >= 0.35:
                session.events.put(
                    session.event_class(
                        "info", f"Arcane North Test: loot remains visible, delaying north movement for {slow_payload['loot_label']}."
                    )
                )
                control["last_loot_log_at"] = now
            control["pause_until"] = max(control["pause_until"], now + random.uniform(*ARCANE_NORTH_TEST_TICK_SLEEP))
            return {"status": "pause_loot", "frame_age_ms": frame_age_ms, "fast_age_ms": fast_age_ms, "slow_age_ms": slow_age_ms}

        if now < control["pause_until"]:
            return {"status": "cooldown", "frame_age_ms": frame_age_ms, "fast_age_ms": fast_age_ms, "slow_age_ms": slow_age_ms}

        path_stage = control["path_stage"]
        frame_change = None
        first_fork_change = None
        second_fork_change = None
        upper_right_floor_visible = False
        if fast_payload is not None:
            frame_change = fast_payload["final_progress_change"] if path_stage >= 3 else fast_payload["progress_change"]
            trend_change = fast_payload.get("progress_trend")
            if trend_change is not None and frame_change is not None:
                frame_change = max(frame_change, trend_change)
            first_fork_change = fast_payload["first_fork_change"]
            second_fork_change = fast_payload["second_fork_change"]
            upper_right_floor_visible = fast_payload["upper_right_floor_score"] >= ARCANE_FINAL_QUARTER_FLOOR_THRESHOLD
            control["star_ticks"] = control["star_ticks"] + 1 if fast_payload["star_detected"] else 0

        if path_stage == 0 and first_fork_change is not None:
            control["first_fork_stagnant_ticks"] = (
                control["first_fork_stagnant_ticks"] + 1 if first_fork_change < ARCANE_FIRST_FORK_PROGRESS_THRESHOLD else 0
            )
            if control["first_fork_stagnant_ticks"] >= ARCANE_FIRST_FORK_STAGNANT_LIMIT:
                control["first_fork_stagnant_ticks"] = 0
                control["second_fork_stagnant_ticks"] = 0
                control["star_ticks"] = 0
                control["stagnant_ticks"] = 0
                (
                    control["path_stage"],
                    control["path_stage_move_ticks"],
                    control["effective_stage_move_ticks"],
                    control["observed_branch_span_ticks"],
                ) = _advance_arcane_path_stage(
                    session,
                    control["path_stage"],
                    control["path_stage_move_ticks"],
                    control["effective_stage_move_ticks"],
                    control["observed_branch_span_ticks"],
                    "upper-right corner stabilized",
                )
                control["pending_stage_repress"] = True
        elif path_stage == 1 and second_fork_change is not None:
            control["second_fork_stagnant_ticks"] = (
                control["second_fork_stagnant_ticks"] + 1 if second_fork_change < ARCANE_SECOND_FORK_PROGRESS_THRESHOLD else 0
            )
            if control["second_fork_stagnant_ticks"] >= ARCANE_SECOND_FORK_STAGNANT_LIMIT:
                control["second_fork_stagnant_ticks"] = 0
                control["star_ticks"] = 0
                control["stagnant_ticks"] = 0
                (
                    control["path_stage"],
                    control["path_stage_move_ticks"],
                    control["effective_stage_move_ticks"],
                    control["observed_branch_span_ticks"],
                ) = _advance_arcane_path_stage(
                    session,
                    control["path_stage"],
                    control["path_stage_move_ticks"],
                    control["effective_stage_move_ticks"],
                    control["observed_branch_span_ticks"],
                    "upper-left corner stabilized",
                )
                control["pending_stage_repress"] = True
        elif control["star_ticks"] >= ARCANE_STAR_STAGNANT_LIMIT:
            control["star_ticks"] = 0
            control["stagnant_ticks"] = 0
            (
                control["path_stage"],
                control["path_stage_move_ticks"],
                control["effective_stage_move_ticks"],
                control["observed_branch_span_ticks"],
            ) = _advance_arcane_path_stage(
                session,
                control["path_stage"],
                control["path_stage_move_ticks"],
                control["effective_stage_move_ticks"],
                control["observed_branch_span_ticks"],
                "star dead-end detected",
            )
            control["pending_stage_repress"] = True
        elif frame_change is not None:
            control["stagnant_ticks"] = control["stagnant_ticks"] + 1 if frame_change < ARCANE_PROGRESS_CHANGE_THRESHOLD else 0
            if control["stagnant_ticks"] >= ARCANE_PROGRESS_STAGNANT_LIMIT:
                control["stagnant_ticks"] = 0
                (
                    control["path_stage"],
                    control["path_stage_move_ticks"],
                    control["effective_stage_move_ticks"],
                    control["observed_branch_span_ticks"],
                ) = _advance_arcane_path_stage(
                    session,
                    control["path_stage"],
                    control["path_stage_move_ticks"],
                    control["effective_stage_move_ticks"],
                    control["observed_branch_span_ticks"],
                    "low frame change detected",
                )
                control["pending_stage_repress"] = True

        control["north_steps"] += 1
        control["path_stage_move_ticks"] += 1
        if frame_change is None or frame_change >= ARCANE_PROGRESS_CHANGE_THRESHOLD:
            control["effective_stage_move_ticks"] += 1

        cursor_ratio = _resolve_arcane_north_cursor_ratio(
            session,
            latest_frame.frame,
            control["path_stage"],
            control["effective_stage_move_ticks"],
            control["observed_branch_span_ticks"],
            north_way_detected=slow_payload["north_way_detected"] if slow_payload is not None else None,
            upper_right_floor_visible=upper_right_floor_visible,
        )
        target_ref = _TargetRef(latest_frame.target)
        if control["pending_stage_repress"]:
            if control["movement_key_held"]:
                session._hold_key_up(actions.movement_skill_key)
                control["movement_key_held"] = False
            session._aim_relative_ratio(target_ref, *ARCANE_ZERO_POINT_CURSOR_RATIO, apply_jitter=False)
            session._sleep_range(*session.CLICK_SETTLE)
            session._aim_relative_ratio(target_ref, *cursor_ratio, apply_jitter=False)
            session._sleep_range(*session.CLICK_SETTLE)
            if control["path_stage"] < 3:
                session._hold_key_down(actions.movement_skill_key)
                control["movement_key_held"] = True
            control["pending_stage_repress"] = False
        elif control["path_stage"] >= 3 and control["movement_key_held"]:
            session._hold_key_up(actions.movement_skill_key)
            control["movement_key_held"] = False

        final_ratio = _steer_arcane_movement(
            session,
            target_ref,
            latest_frame.frame,
            cursor_ratio,
            control["path_stage"],
            teleporter_hover_detected=slow_payload["teleporter_hover_detected"] if slow_payload is not None else False,
        )
        if control["path_stage"] >= 3:
            session._press_key(actions.movement_skill_key)
            action_phrase = f"and cast movement_skill_key '{actions.movement_skill_key}'."
        else:
            if not control["movement_key_held"]:
                session._hold_key_down(actions.movement_skill_key)
                control["movement_key_held"] = True
            action_phrase = f"with movement_skill_key '{actions.movement_skill_key}' still held."
        session.events.put(
            session.event_class(
                "info",
                f"Arcane North Test: step {control['north_steps']}, stage={_arcane_path_stage_label(control['path_stage'])}, frame_age={frame_age_ms}ms, fast_age={fast_age_ms}ms, slow_age={slow_age_ms}ms, base_ratio={cursor_ratio}, final_ratio={final_ratio} {action_phrase}",
            )
        )
        session._sleep_range(*ARCANE_MOVE_STEP_SETTLE)
        return {
            "status": "move",
            "frame_age_ms": frame_age_ms,
            "fast_age_ms": fast_age_ms,
            "slow_age_ms": slow_age_ms,
            "path_stage": control["path_stage"],
            "final_ratio": final_ratio,
        }

    runtime = RealtimeVisionRuntime(
        session.config.capture,
        session._stop_event,
        _fast_vision,
        _slow_vision,
        _decision,
        fast_interval=0.005,
        slow_interval=0.03,
        decision_interval=0.005,
        error_handler=_runtime_error,
    )
    try:
        session._aim_relative_ratio(capture, *ARCANE_ZERO_POINT_CURSOR_RATIO, apply_jitter=False)
        runtime.start()
        while not session._stop_event.wait(0.05):
            pass
    finally:
        runtime.stop()
        if control["movement_key_held"]:
            session._hold_key_up(actions.movement_skill_key)

    session.events.put(session.event_class("info", "Arcane North Test: stopped."))


def _steer_arcane_movement(
    session, capture_target, frame: np.ndarray, base_ratio: tuple[float, float], path_stage: int, teleporter_hover_detected: bool = False
) -> tuple[float, float]:
    floor_ratio = base_ratio if path_stage >= 3 else _resolve_arcane_floor_guided_ratio(session, frame, base_ratio, path_stage)
    session._aim_relative_ratio(capture_target, *floor_ratio, apply_jitter=False)
    session._sleep_range(*session.CLICK_SETTLE)
    if not teleporter_hover_detected:
        return floor_ratio
    session.events.put(
        session.event_class("info", "Arcane North Test: teleporter hover detected while steering; nudging cursor beside it.")
    )
    for offset in ARCANE_TELEPORTER_NUDGE_OFFSETS:
        nudged_ratio = session._apply_offset(floor_ratio, offset)
        session._aim_relative_ratio(capture_target, *nudged_ratio, apply_jitter=False)
        session._sleep_range(*session.CLICK_SETTLE)
        return nudged_ratio
    return floor_ratio


def _resolve_arcane_floor_guided_ratio(
    session, frame: np.ndarray, base_ratio: tuple[float, float], path_stage: int = 0
) -> tuple[float, float]:
    best_ratio = base_ratio
    best_score = -1.0
    offsets = ARCANE_FOUR_OCLOCK_FLOOR_CANDIDATE_OFFSETS if path_stage >= 3 else ARCANE_FLOOR_CANDIDATE_OFFSETS
    for offset in offsets:
        candidate = session._apply_offset(base_ratio, offset)
        score = _score_arcane_floor_at_ratio(session, frame, candidate)
        if score > best_score:
            best_score = score
            best_ratio = candidate
    return best_ratio


def _score_arcane_floor_patch(patch: np.ndarray) -> float:
    hsv = cv.cvtColor(patch, cv.COLOR_BGR2HSV)
    mask = cv.inRange(hsv, (0, 0, 70), (180, 60, 235))
    return float(mask.mean())


def _score_arcane_floor_at_ratio(session, frame: np.ndarray, ratio: tuple[float, float]) -> float:
    height, width = frame.shape[:2]
    center_x = int(width * ratio[0])
    center_y = int(height * ratio[1])
    radius = ARCANE_FLOOR_SCORE_RADIUS
    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    patch = frame[top:bottom, left:right]
    if patch.size == 0:
        return -1.0
    return _score_arcane_floor_patch(patch)


def _resolve_arcane_north_cursor_ratio(
    session,
    frame: np.ndarray,
    path_stage: int,
    effective_stage_move_ticks: int,
    observed_branch_span_ticks: int | None,
    north_way_detected: bool | None = None,
    upper_right_floor_visible: bool | None = None,
) -> tuple[float, float]:
    if path_stage <= 0:
        return ARCANE_NORTH_CURSOR_RATIO
    if path_stage == 1:
        return ARCANE_NEXT_PATH_CURSOR_RATIO
    if path_stage == 2:
        return ARCANE_RETURN_NORTH_CURSOR_RATIO
    return _resolve_arcane_final_stage_ratio(
        session,
        frame,
        effective_stage_move_ticks,
        observed_branch_span_ticks,
        north_way_detected=north_way_detected,
        upper_right_floor_visible=upper_right_floor_visible,
    )


def _resolve_arcane_final_stage_ratio(
    session,
    frame: np.ndarray,
    effective_stage_move_ticks: int,
    observed_branch_span_ticks: int | None,
    north_way_detected: bool | None = None,
    upper_right_floor_visible: bool | None = None,
) -> tuple[float, float]:
    four_oclock_ratio = _resolve_arcane_floor_guided_ratio(session, frame, ARCANE_FOUR_OCLOCK_CURSOR_RATIO, 3)
    if effective_stage_move_ticks >= ARCANE_FINAL_STAGE_MIN_FOUR_OCLOCK_TICKS:
        north_way_ready = _detect_arcane_final_north_way(session, frame) if north_way_detected is None else north_way_detected
        upper_right_ready = (
            _detect_arcane_final_upper_right_floor(session, frame) if upper_right_floor_visible is None else upper_right_floor_visible
        )
        if north_way_ready or upper_right_ready:
            return _resolve_arcane_floor_guided_ratio(session, frame, ARCANE_FINAL_NORTH_BEND_CURSOR_RATIO, 2)
    travel_budget = (
        min(observed_branch_span_ticks, ARCANE_FINAL_STAGE_TRAVEL_TICKS)
        if observed_branch_span_ticks is not None
        else ARCANE_FINAL_STAGE_TRAVEL_TICKS
    )
    if effective_stage_move_ticks < travel_budget:
        return four_oclock_ratio
    final_north_ratio = _resolve_arcane_floor_guided_ratio(session, frame, ARCANE_FINAL_NORTH_BEND_CURSOR_RATIO, 2)
    if _score_arcane_floor_at_ratio(session, frame, final_north_ratio) >= ARCANE_BRANCH_FLOOR_PREFERENCE_THRESHOLD:
        return final_north_ratio
    return ARCANE_FINAL_NORTH_BEND_CURSOR_RATIO


def _advance_arcane_path_stage(
    session,
    path_stage: int,
    path_stage_move_ticks: int,
    effective_stage_move_ticks: int,
    observed_branch_span_ticks: int | None,
    reason: str,
) -> tuple[int, int, int, int | None]:
    if path_stage >= 3:
        return path_stage, path_stage_move_ticks, effective_stage_move_ticks, observed_branch_span_ticks
    next_stage = path_stage + 1
    if path_stage == 1 and next_stage == 2 and effective_stage_move_ticks > 0:
        observed_branch_span_ticks = max(1, effective_stage_move_ticks - ARCANE_BRANCH_SPAN_TRIM_TICKS)
    detail = ""
    if observed_branch_span_ticks is not None and next_stage >= 3:
        detail = f" (using final travel budget {min(observed_branch_span_ticks, ARCANE_FINAL_STAGE_TRAVEL_TICKS)} tick(s) after trim)"
    session.events.put(
        session.event_class("info", f"Arcane North Test: {reason}, advancing path aim to {_arcane_path_stage_label(next_stage)}.{detail}")
    )
    return next_stage, 0, 0, observed_branch_span_ticks


def _arcane_path_stage_label(path_stage: int) -> str:
    if path_stage <= 0:
        return "north-straight"
    if path_stage == 1:
        return "10 o'clock next-path"
    if path_stage == 2:
        return "2 o'clock next-path"
    return "4 o'clock next-path / final north bend"


def _detect_arcane_dead_end_star(session, frame: np.ndarray) -> bool:
    return session._locate_template(frame, session._arcane_star_template, ARCANE_STAR_THRESHOLD) is not None


def _arcane_upper_right_floor_score(session, frame: np.ndarray) -> float:
    height, width = frame.shape[:2]
    patch = frame[0 : max(1, height // 4), (width * 3) // 4 : width]
    if patch.size == 0:
        return -1.0
    return _score_arcane_floor_patch(patch)


def _detect_arcane_final_upper_right_floor(session, frame: np.ndarray) -> bool:
    return _arcane_upper_right_floor_score(session, frame) >= ARCANE_FINAL_QUARTER_FLOOR_THRESHOLD


def _detect_arcane_final_north_way(session, frame: np.ndarray) -> bool:
    if session._locate_template(frame, session._arcane_north_way_template, ARCANE_NORTH_WAY_THRESHOLD) is not None:
        return True
    roi = _arcane_progress_roi(frame, 3)
    return session._locate_template(roi, session._arcane_north_way_template, ARCANE_NORTH_WAY_THRESHOLD) is not None


def _detect_arcane_north_terminal(session, frame: np.ndarray) -> str | None:
    if (
        session._arcane_summoner_north_template is not None
        and session._locate_template(frame, session._arcane_summoner_north_template, ARCANE_NORTH_TERMINAL_THRESHOLD) is not None
    ):
        return "summoner"
    if _detect_arcane_summoner_clues(session, frame):
        return "summoner"
    if (
        session._arcane_without_summoner_north_template is not None
        and session._locate_template(frame, session._arcane_without_summoner_north_template, ARCANE_NORTH_TERMINAL_THRESHOLD) is not None
    ):
        return "without_summoner"
    return None


def _detect_arcane_summoner_clues(session, frame: np.ndarray) -> bool:
    for template in (
        session._arcane_horazon_journal_template,
        session._arcane_summoner_location_template,
        session._arcane_summoner_location_background_template,
    ):
        if session._locate_template(frame, template, ARCANE_SUMMONER_CLUE_THRESHOLD) is not None:
            return True
    return False


def _measure_arcane_second_fork_change(session, previous_frame: np.ndarray | None, current_frame: np.ndarray) -> float | None:
    if previous_frame is None:
        return None
    prev_small = cv.resize(cv.cvtColor(_arcane_second_fork_roi(previous_frame), cv.COLOR_BGR2GRAY), (120, 80))
    curr_small = cv.resize(cv.cvtColor(_arcane_second_fork_roi(current_frame), cv.COLOR_BGR2GRAY), (120, 80))
    return float(cv.absdiff(prev_small, curr_small).mean())


def _measure_arcane_first_fork_change(session, previous_frame: np.ndarray | None, current_frame: np.ndarray) -> float | None:
    if previous_frame is None:
        return None
    prev_small = cv.resize(cv.cvtColor(_arcane_first_fork_roi(previous_frame), cv.COLOR_BGR2GRAY), (120, 80))
    curr_small = cv.resize(cv.cvtColor(_arcane_first_fork_roi(current_frame), cv.COLOR_BGR2GRAY), (120, 80))
    return float(cv.absdiff(prev_small, curr_small).mean())


def _measure_arcane_progress_change(
    session, previous_frame: np.ndarray | None, current_frame: np.ndarray, path_stage: int = 0
) -> float | None:
    if previous_frame is None:
        return None
    prev_small = cv.resize(cv.cvtColor(_arcane_progress_roi(previous_frame, path_stage), cv.COLOR_BGR2GRAY), (160, 90))
    curr_small = cv.resize(cv.cvtColor(_arcane_progress_roi(current_frame, path_stage), cv.COLOR_BGR2GRAY), (160, 90))
    return float(cv.absdiff(prev_small, curr_small).mean())


def _measure_arcane_progress_trend(recent_frames: tuple, path_stage: int) -> float | None:
    if len(recent_frames) < 3:
        return None
    values: list[float] = []
    for previous, current in zip(recent_frames[:-1], recent_frames[1:]):
        change = _measure_arcane_progress_change(None, previous.frame, current.frame, path_stage)
        if change is not None:
            values.append(change)
    if not values:
        return None
    return float(sum(values) / len(values))


def _arcane_second_fork_roi(frame: np.ndarray) -> np.ndarray:
    height, width = frame.shape[:2]
    return frame[0 : max(1, height // 4), 0 : max(1, width // 4)]


def _arcane_first_fork_roi(frame: np.ndarray) -> np.ndarray:
    height, width = frame.shape[:2]
    return frame[0 : max(1, height // 4), (width * 3) // 4 : width]


def _arcane_progress_roi(frame: np.ndarray, path_stage: int = 0) -> np.ndarray:
    height, width = frame.shape[:2]
    if path_stage >= 3:
        return frame[0 : max(1, height // 3), (width * 2) // 3 : width]
    return frame[int(height * 0.08) : int(height * 0.82), int(width * 0.08) : int(width * 0.92)]
