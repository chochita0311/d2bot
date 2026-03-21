from __future__ import annotations

import random
from dataclasses import dataclass

import cv2 as cv
import numpy as np

from diablo2.actions.loot_pickup import LootPickupSession
from diablo2.common.color_palette import palette_distance_map_bgr, palette_match_ratio
from diablo2.common.movement import (
    MOVEMENT_INTENT_TRAVEL,
    MOVEMENT_INTENT_REPOSITION,
    MovementExecutionState,
    apply_movement_intent,
    release_movement_intent,
)
from diablo2.common.realtime import RealtimeVisionRuntime, RuntimeSnapshot
from diablo2.runs.base import RunRouteSegment
from diablo2.runs.summoner.routes.common.arcane_common import (
    ARCANE_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_FLOOR_SCORE_RADIUS,
    ARCANE_FOUR_OCLOCK_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_MOVE_STEP_SETTLE,
    ARCANE_NORTH_TERMINAL_THRESHOLD,
    ARCANE_NORTH_TEST_TICK_SLEEP,
    ARCANE_PROGRESS_CHANGE_THRESHOLD,
    ARCANE_SUMMONER_CLUE_THRESHOLD,
    ARCANE_TELEPORTER_HOVER_THRESHOLD,
    ARCANE_TELEPORTER_NUDGE_OFFSETS,
    ARCANE_WINGS,
    ARCANE_ZERO_POINT_CURSOR_RATIO,
    prepare_arcane_hub_start,
)
from diablo2.runs.summoner.routes.common.arcane_palette import (
    ARCANE_FLOOR_GRAY_PALETTE_BGR,
    ARCANE_FLOOR_MAX_DISTANCE,
    ARCANE_STAR_VOID_MAX_DISTANCE,
    ARCANE_STAR_VOID_PALETTE_BGR,
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

ARCANE_FAST_SWITCH_LIMIT_MS = 120
ARCANE_FAST_STEER_LIMIT_MS = 350
ARCANE_SLOW_STALE_LIMIT_MS = 450
ARCANE_DIRECTION_PATCH_RADIUS = 42
ARCANE_DIRECTION_PATH_SAMPLES = (0.45, 0.65, 0.85)
ARCANE_FAST_SCALE = 0.35
ARCANE_FAST_MIN_PATCH_RADIUS = 8
ARCANE_NORTH_OPEN_FLOOR_RATIO = 0.1
ARCANE_VOTE_KEEP_MARGIN = 0.045
ARCANE_FAMILY_KEEP_MARGIN = 0.06
ARCANE_NORTH_RECLAIM_MARGIN = 0.08
ARCANE_NORTH_OPEN_CIRCLE_PROBES = (
    (2.0 / 3.0, 1.0 / 3.0),
    (5.0 / 6.0, 1.0 / 3.0),
    (5.0 / 6.0, 2.0 / 3.0),
)
ARCANE_NORTH_OPEN_CIRCLE_RADIUS_RATIO = 0.16


@dataclass(frozen=True)
class ArcaneDirectionCandidate:
    key: str
    label: str
    ratio: tuple[float, float]
    family: str
    north_family: bool = False
    bias: float = 0.0


ARCANE_DIRECTION_CANDIDATES: tuple[ArcaneDirectionCandidate, ...] = (
    ArcaneDirectionCandidate("north_primary", "2 o'clock primary", (0.84, 0.24), family="north", north_family=True, bias=0.22),
    ArcaneDirectionCandidate("north_soft", "2 o'clock soft", (0.79, 0.27), family="north", north_family=True, bias=0.18),
    ArcaneDirectionCandidate("north_sharp", "2 o'clock sharp", (0.89, 0.18), family="north", north_family=True, bias=0.18),
    ArcaneDirectionCandidate("left_turn", "10 o'clock turn", (0.15, 0.18), family="left", bias=0.04),
    ArcaneDirectionCandidate("left_soft", "10 o'clock soft", (0.21, 0.23), family="left", bias=0.03),
    ArcaneDirectionCandidate("bend_soft", "4 o'clock soft", (0.84, 0.70), family="right", bias=0.02),
    ArcaneDirectionCandidate("bend_drop", "4 o'clock bend", (0.90, 0.80), family="right", bias=0.00),
    ArcaneDirectionCandidate("north_recover", "2 o'clock recover", (0.88, 0.12), family="north", north_family=True, bias=0.14),
)


def run_arcane_north_go(session, capture) -> None:
    actions = session._resolve_arcane_character_actions()
    if actions is None or not actions.movement_skill_key:
        raise RuntimeError("Arcane North Test requires a character with movement_skill_key configured.")

    prepare_arcane_hub_start(session, capture, "north")
    zero_point_ratio = getattr(session, "_arcane_hub_focus_ratio", ARCANE_ZERO_POINT_CURSOR_RATIO)
    movement_state = MovementExecutionState()
    session._aim_relative_ratio(capture, *zero_point_ratio, apply_jitter=False)
    hub_action_phrase = apply_movement_intent(session, actions, movement_state, MOVEMENT_INTENT_REPOSITION)
    session.events.put(
        session.event_class(
            "info",
            f"Summoner: repositioning onto the Arcane hub focus at ratio=({zero_point_ratio[0]:.3f}, {zero_point_ratio[1]:.3f}) {hub_action_phrase}",
        )
    )
    session._sleep_range(*session.CLICK_SETTLE)
    release_movement_intent(session, actions, movement_state)
    session.events.put(
        session.event_class(
            "info",
            "Arcane North Test: assuming Arcane Sanctuary hub and buffs are already ready; beginning north wing test.",
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
        "pause_until": 0.0,
        "last_frame_id": -1,
        "last_fast_payload": None,
        "last_slow_payload": None,
        "last_monster_log_at": 0.0,
        "last_loot_log_at": 0.0,
        "last_direction_key": "north_primary",
        "last_direction_ratio": (0.84, 0.24),
        "route_family": "north",
        "route_family_steps": 0,
    }
    movement_state = MovementExecutionState()

    def _runtime_error(stage: str, exc: Exception) -> None:
        session.events.put(session.event_class("error", f"Arcane North Test {stage} thread failed: {exc}"))

    def _fast_vision(frame_packet) -> dict[str, object]:
        nonlocal previous_fast_frame
        current_frame = frame_packet.frame
        recent_frames = runtime.state.snapshot().recent_frames
        trend_change = _measure_arcane_progress_trend(recent_frames)
        fast_maps = _build_arcane_fast_maps(current_frame)
        direction_votes = _score_arcane_direction_candidates(
            current_frame, fast_maps, control["last_direction_ratio"], zero_point_ratio
        )
        family_signals = _score_arcane_family_signals(current_frame, fast_maps)
        payload = {
            "progress_change": _measure_arcane_progress_change(session, previous_fast_frame, current_frame),
            "progress_trend": trend_change,
            "direction_votes": direction_votes,
            "family_signals": family_signals,
        }
        previous_fast_frame = current_frame
        return payload

    def _slow_vision(frame_packet) -> dict[str, object]:
        loot_hit = loot_session.scan_frame(frame_packet.frame)
        return {
            "north_terminal": _detect_arcane_north_terminal(session, frame_packet.frame),
            "loot_label": loot_hit.label if loot_hit is not None else None,
            "monster_hit": session._scan_arcane_monsters(frame_packet.frame),
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
        fast_switch_fresh = fast_age_ms is not None and fast_age_ms <= ARCANE_FAST_SWITCH_LIMIT_MS
        fast_steer_fresh = fast_age_ms is not None and fast_age_ms <= ARCANE_FAST_STEER_LIMIT_MS
        slow_fresh = slow_age_ms is not None and slow_age_ms <= ARCANE_SLOW_STALE_LIMIT_MS

        if not fast_steer_fresh:
            fallback_candidate = _resolve_arcane_direction_candidate(control["last_direction_key"])
            cursor_ratio = control["last_direction_ratio"]
            direction_key = fallback_candidate.key
            direction_label = fallback_candidate.label
            direction_family = control["route_family"]
            direction_score = 0.0
            north_open = 0.0
            target_ref = _TargetRef(latest_frame.target)
            teleporter_hover_detected = slow_payload["teleporter_hover_detected"] if slow_payload is not None and slow_fresh else False
            if teleporter_hover_detected:
                release_movement_intent(session, actions, movement_state)
                session._sleep_range(*session.CLICK_SETTLE)
            final_ratio = _steer_arcane_movement(
                session,
                target_ref,
                latest_frame.frame,
                cursor_ratio,
                0,
                teleporter_hover_detected=teleporter_hover_detected,
            )
            action_phrase = apply_movement_intent(session, actions, movement_state, MOVEMENT_INTENT_TRAVEL)
            control["north_steps"] += 1
            control["last_direction_key"] = direction_key
            control["last_direction_ratio"] = final_ratio
            control["route_family_steps"] += 1
            session.events.put(
                session.event_class(
                    "info",
                    f"Arcane North Test: step {control['north_steps']}, family={direction_family}, choice={direction_label}, vote={direction_score:.3f}, north_open={north_open:.3f}, frame_change=None, frame_age={frame_age_ms}ms, fast_age={fast_age_ms}ms, slow_age={slow_age_ms}ms, base_ratio={cursor_ratio}, final_ratio={final_ratio} {action_phrase} (stale-fast hold)",
                )
            )
            session._sleep_range(*ARCANE_MOVE_STEP_SETTLE)
            return {
                "status": "stale_fast_hold",
                "frame_age_ms": frame_age_ms,
                "fast_age_ms": fast_age_ms,
                "slow_age_ms": slow_age_ms,
                "direction_key": direction_key,
                "final_ratio": final_ratio,
            }

        if slow_payload is not None and slow_fresh and slow_payload["north_terminal"] is not None:
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

        if slow_payload is not None and slow_fresh and slow_payload["monster_hit"] is not None:
            if now - control["last_monster_log_at"] >= 0.35:
                session.events.put(
                    session.event_class("info", f"Arcane North Test: monster detected during run -> {slow_payload['monster_hit']}.")
                )
                control["last_monster_log_at"] = now
            control["pause_until"] = max(control["pause_until"], now + random.uniform(*ARCANE_NORTH_TEST_TICK_SLEEP))
            return {"status": "pause_monster", "frame_age_ms": frame_age_ms, "fast_age_ms": fast_age_ms, "slow_age_ms": slow_age_ms}

        if slow_payload is not None and slow_fresh and slow_payload["loot_label"] is not None:
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
        frame_change = fast_payload.get("progress_change") if fast_payload is not None else None
        if frame_change is not None and fast_payload.get("progress_trend") is not None:
            frame_change = max(frame_change, fast_payload["progress_trend"])

        direction_choice = _choose_arcane_direction(
            fast_payload["direction_votes"] if fast_payload is not None else [],
            control["last_direction_key"],
            control["route_family"],
            dict(fast_payload.get("family_signals", {})) if fast_payload is not None else {},
            control["route_family_steps"],
            fast_switch_fresh,
        )
        direction_key = direction_choice["key"]
        direction_label = direction_choice["label"]
        direction_family = direction_choice["family"]
        cursor_ratio = direction_choice["ratio"]
        direction_score = direction_choice["score"]
        north_open = direction_choice["north_open"]
        target_ref = _TargetRef(latest_frame.target)
        teleporter_hover_detected = slow_payload["teleporter_hover_detected"] if slow_payload is not None and slow_fresh else False
        if teleporter_hover_detected:
            release_movement_intent(session, actions, movement_state)
            session._sleep_range(*session.CLICK_SETTLE)
        final_ratio = _steer_arcane_movement(
            session,
            target_ref,
            latest_frame.frame,
            cursor_ratio,
            0,
            teleporter_hover_detected=teleporter_hover_detected,
        )
        action_phrase = apply_movement_intent(session, actions, movement_state, MOVEMENT_INTENT_TRAVEL)
        control["north_steps"] += 1
        control["last_direction_key"] = direction_key
        control["last_direction_ratio"] = final_ratio
        if direction_family == control["route_family"]:
            control["route_family_steps"] += 1
        else:
            control["route_family"] = direction_family
            control["route_family_steps"] = 1
        session.events.put(
            session.event_class(
                "info",
                f"Arcane North Test: step {control['north_steps']}, family={direction_family}, choice={direction_label}, vote={direction_score:.3f}, north_open={north_open:.3f}, frame_change={frame_change}, frame_age={frame_age_ms}ms, fast_age={fast_age_ms}ms, slow_age={slow_age_ms}ms, base_ratio={cursor_ratio}, final_ratio={final_ratio} {action_phrase}",
            )
        )
        session._sleep_range(*ARCANE_MOVE_STEP_SETTLE)
        return {
            "status": "move",
            "frame_age_ms": frame_age_ms,
            "fast_age_ms": fast_age_ms,
            "slow_age_ms": slow_age_ms,
            "direction_key": direction_key,
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
        capture_fps=20.0,
        error_handler=_runtime_error,
    )
    try:
        session._aim_relative_ratio(capture, *zero_point_ratio, apply_jitter=False)
        runtime.start()
        while not session._stop_event.wait(0.05):
            pass
    finally:
        runtime.stop()
        release_movement_intent(session, actions, movement_state)

    session.events.put(session.event_class("info", "Arcane North Test: stopped."))


def _steer_arcane_movement(
    session, capture_target, frame: np.ndarray, base_ratio: tuple[float, float], path_stage: int, teleporter_hover_detected: bool = False
) -> tuple[float, float]:
    floor_ratio = _resolve_arcane_floor_guided_ratio(session, frame, base_ratio, path_stage)
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
    floor_ratio = palette_match_ratio(patch, ARCANE_FLOOR_GRAY_PALETTE_BGR, ARCANE_FLOOR_MAX_DISTANCE)
    star_void_ratio = palette_match_ratio(patch, ARCANE_STAR_VOID_PALETTE_BGR, ARCANE_STAR_VOID_MAX_DISTANCE)
    return (floor_ratio * 100.0) - (star_void_ratio * 80.0)


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


def _build_arcane_fast_maps(frame: np.ndarray) -> dict[str, np.ndarray]:
    scaled_frame = cv.resize(frame, None, fx=ARCANE_FAST_SCALE, fy=ARCANE_FAST_SCALE, interpolation=cv.INTER_AREA)
    floor_distance = palette_distance_map_bgr(scaled_frame, ARCANE_FLOOR_GRAY_PALETTE_BGR)
    star_distance = palette_distance_map_bgr(scaled_frame, ARCANE_STAR_VOID_PALETTE_BGR)
    floor_mask = (floor_distance <= ARCANE_FLOOR_MAX_DISTANCE).astype(np.float32)
    star_mask = (star_distance <= ARCANE_STAR_VOID_MAX_DISTANCE).astype(np.float32)
    return {"floor_mask": floor_mask, "star_mask": star_mask, "scale": ARCANE_FAST_SCALE}


def _score_arcane_direction_candidates(
    frame: np.ndarray,
    fast_maps: dict[str, np.ndarray],
    previous_ratio: tuple[float, float],
    zero_point_ratio: tuple[float, float],
) -> list[dict[str, object]]:
    votes: list[dict[str, object]] = []
    north_path_open = 0.0
    for candidate in ARCANE_DIRECTION_CANDIDATES:
        path_floor_ratio, path_star_void_ratio, openness = _score_arcane_direction_path(
            frame, fast_maps, zero_point_ratio, candidate.ratio
        )
        continuity_bonus = _arcane_direction_continuity_bonus(previous_ratio, candidate.ratio)
        score = openness + candidate.bias + continuity_bonus
        vote = {
            "key": candidate.key,
            "label": candidate.label,
            "ratio": candidate.ratio,
            "family": candidate.family,
            "score": score,
            "floor_ratio": path_floor_ratio,
            "star_void_ratio": path_star_void_ratio,
            "north_family": candidate.north_family,
        }
        votes.append(vote)
        if candidate.north_family:
            north_path_open = max(north_path_open, path_floor_ratio - (path_star_void_ratio * 0.65))
    for vote in votes:
        vote["north_open"] = north_path_open
    return votes


def _choose_arcane_direction(
    votes: list[dict[str, object]],
    previous_key: str,
    current_family: str,
    family_signals: dict[str, float],
    family_steps: int,
    allow_family_switch: bool,
) -> dict[str, object]:
    if not votes:
        fallback = ARCANE_DIRECTION_CANDIDATES[0]
        return {
            "key": fallback.key,
            "label": fallback.label,
            "family": fallback.family,
            "ratio": fallback.ratio,
            "score": 0.0,
            "north_open": 0.0,
        }
    north_open = max(0.0, float(family_signals.get("north", 0.0)))
    by_family: dict[str, list[dict[str, object]]] = {}
    for vote in votes:
        by_family.setdefault(str(vote["family"]), []).append(vote)

    best_by_family = {family: max(items, key=lambda vote: float(vote["score"])) for family, items in by_family.items()}
    best_vote = max(votes, key=lambda vote: float(vote["score"]))
    current_family_vote = best_by_family.get(current_family)
    north_family_vote = best_by_family.get("north")
    north_closed = north_open < ARCANE_NORTH_OPEN_FLOOR_RATIO
    candidate_families = (
        {family: vote for family, vote in best_by_family.items() if family != "north"}
        if north_closed
        else best_by_family
    )
    if not candidate_families:
        candidate_families = best_by_family
    best_vote = max(candidate_families.values(), key=lambda vote: float(vote["score"]))
    current_family_vote = best_by_family.get(current_family)
    north_family_vote = best_by_family.get("north")

    if north_closed and current_family == "north":
        current_family_vote = None
        north_family_vote = None

    if north_family_vote is not None and not north_closed:
        north_family_vote["north_open"] = north_open
        return north_family_vote

    if not allow_family_switch and current_family_vote is not None:
        best_vote = current_family_vote
    elif (
        current_family != "north"
        and current_family_vote is not None
        and north_family_vote is not None
        and north_open >= ARCANE_NORTH_OPEN_FLOOR_RATIO
        and (float(north_family_vote["score"]) - float(current_family_vote["score"])) >= ARCANE_NORTH_RECLAIM_MARGIN
    ):
        best_vote = north_family_vote
    elif current_family_vote is not None and (float(best_vote["score"]) - float(current_family_vote["score"])) <= ARCANE_FAMILY_KEEP_MARGIN:
        best_vote = current_family_vote
    elif current_family == "north" and north_open >= ARCANE_NORTH_OPEN_FLOOR_RATIO and north_family_vote is not None:
        best_vote = north_family_vote

    previous_vote = next((vote for vote in votes if vote["key"] == previous_key), None)
    if (
        previous_vote is not None
        and not (north_closed and bool(previous_vote.get("north_family")))
        and (float(best_vote["score"]) - float(previous_vote["score"])) <= ARCANE_VOTE_KEEP_MARGIN
    ):
        best_vote = previous_vote
    best_vote["north_open"] = north_open
    return best_vote


def _score_arcane_direction_path(
    frame: np.ndarray,
    fast_maps: dict[str, np.ndarray],
    zero_point_ratio: tuple[float, float],
    candidate_ratio: tuple[float, float],
) -> tuple[float, float, float]:
    floor_values: list[float] = []
    star_values: list[float] = []
    scaled_radius = max(ARCANE_FAST_MIN_PATCH_RADIUS, int(ARCANE_DIRECTION_PATCH_RADIUS * float(fast_maps.get("scale", 1.0))))
    for t in ARCANE_DIRECTION_PATH_SAMPLES:
        sample_ratio = _interpolate_ratio(zero_point_ratio, candidate_ratio, t)
        floor_patch = _crop_arcane_scalar_patch_at_ratio(fast_maps["floor_mask"], sample_ratio, scaled_radius)
        star_patch = _crop_arcane_scalar_patch_at_ratio(fast_maps["star_mask"], sample_ratio, scaled_radius)
        if floor_patch.size == 0 or star_patch.size == 0:
            continue
        floor_values.append(float(floor_patch.mean()))
        star_values.append(float(star_patch.mean()))
    if not floor_values:
        return 0.0, 1.0, -1.0
    floor_ratio = float(sum(floor_values) / len(floor_values))
    star_void_ratio = float(sum(star_values) / len(star_values))
    openness = floor_ratio - (star_void_ratio * 0.85)
    return floor_ratio, star_void_ratio, openness


def _score_arcane_north_open_signal(frame: np.ndarray, fast_maps: dict[str, np.ndarray]) -> float:
    floor_mask = fast_maps["floor_mask"]
    probe_values: list[float] = []
    for probe_ratio in ARCANE_NORTH_OPEN_CIRCLE_PROBES:
        floor_ratio = _sample_arcane_upper_right_circle(floor_mask, probe_ratio, ARCANE_NORTH_OPEN_CIRCLE_RADIUS_RATIO)
        probe_values.append(floor_ratio)
    if not probe_values:
        return 0.0
    return max(probe_values)


def _score_arcane_family_signals(frame: np.ndarray, fast_maps: dict[str, np.ndarray]) -> dict[str, float]:
    return {
        "north": _score_arcane_north_open_signal(frame, fast_maps),
        "left": 0.0,
        "right": 0.0,
    }


def _resolve_arcane_direction_candidate(key: str) -> ArcaneDirectionCandidate:
    for candidate in ARCANE_DIRECTION_CANDIDATES:
        if candidate.key == key:
            return candidate
    return ARCANE_DIRECTION_CANDIDATES[0]


def _interpolate_ratio(start: tuple[float, float], end: tuple[float, float], t: float) -> tuple[float, float]:
    return (start[0] + ((end[0] - start[0]) * t), start[1] + ((end[1] - start[1]) * t))


def _crop_arcane_patch_at_ratio(frame: np.ndarray, ratio: tuple[float, float], radius: int) -> np.ndarray:
    height, width = frame.shape[:2]
    center_x = int(width * ratio[0])
    center_y = int(height * ratio[1])
    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    return frame[top:bottom, left:right]


def _crop_arcane_scalar_patch_at_ratio(image: np.ndarray, ratio: tuple[float, float], radius: int) -> np.ndarray:
    height, width = image.shape[:2]
    center_x = int(width * ratio[0])
    center_y = int(height * ratio[1])
    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    return image[top:bottom, left:right]


def _sample_arcane_upper_right_circle(
    image: np.ndarray, probe_ratio: tuple[float, float], radius_ratio: float
) -> float:
    height, width = image.shape[:2]
    upper_right_left = width * 0.5
    upper_right_top = 0.0
    upper_right_width = width * 0.5
    upper_right_height = height * 0.5

    center_x = int(upper_right_left + (upper_right_width * probe_ratio[0]))
    center_y = int(upper_right_top + (upper_right_height * probe_ratio[1]))
    radius = max(
        ARCANE_FAST_MIN_PATCH_RADIUS,
        int(min(upper_right_width / 3.0, upper_right_height / 3.0) * radius_ratio * 3.0),
    )

    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    patch = image[top:bottom, left:right]
    if patch.size == 0:
        return 0.0

    patch_height, patch_width = patch.shape[:2]
    yy, xx = np.ogrid[:patch_height, :patch_width]
    circle_center_x = center_x - left
    circle_center_y = center_y - top
    mask = ((xx - circle_center_x) ** 2 + (yy - circle_center_y) ** 2) <= (radius * radius)
    if not np.any(mask):
        return 0.0
    return float(patch[mask].mean())


def _arcane_direction_continuity_bonus(previous_ratio: tuple[float, float], candidate_ratio: tuple[float, float]) -> float:
    dx = previous_ratio[0] - candidate_ratio[0]
    dy = previous_ratio[1] - candidate_ratio[1]
    distance = (dx * dx + dy * dy) ** 0.5
    return max(0.0, 0.18 - distance) * 0.7


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


def _measure_arcane_progress_change(session, previous_frame: np.ndarray | None, current_frame: np.ndarray) -> float | None:
    if previous_frame is None:
        return None
    prev_small = cv.resize(cv.cvtColor(_arcane_progress_roi(previous_frame), cv.COLOR_BGR2GRAY), (160, 90))
    curr_small = cv.resize(cv.cvtColor(_arcane_progress_roi(current_frame), cv.COLOR_BGR2GRAY), (160, 90))
    return float(cv.absdiff(prev_small, curr_small).mean())


def _measure_arcane_progress_trend(recent_frames: tuple) -> float | None:
    if len(recent_frames) < 3:
        return None
    values: list[float] = []
    for previous, current in zip(recent_frames[:-1], recent_frames[1:]):
        change = _measure_arcane_progress_change(None, previous.frame, current.frame)
        if change is not None:
            values.append(change)
    if not values:
        return None
    return float(sum(values) / len(values))


def _arcane_progress_roi(frame: np.ndarray) -> np.ndarray:
    height, width = frame.shape[:2]
    return frame[int(height * 0.08) : int(height * 0.82), int(width * 0.08) : int(width * 0.92)]
