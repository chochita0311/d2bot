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
    ARCANE_CHEST_HOVER_THRESHOLD,
    ARCANE_EAST_DIRECTION_POINTS,
    ARCANE_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_FLOOR_SCORE_RADIUS,
    ARCANE_FOUR_OCLOCK_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_HOVER_NUDGE_OFFSETS,
    ARCANE_MOVE_STEP_SETTLE,
    ARCANE_NORTH_DIRECTION_POINTS,
    ARCANE_NORTH_WEST_OPEN_CIRCLE_RADIUS_RATIO,
    ARCANE_NORTH_TERMINAL_THRESHOLD,
    ARCANE_NORTH_TEST_TICK_SLEEP,
    ARCANE_PROGRESS_CHANGE_THRESHOLD,
    ARCANE_SUMMONER_CLUE_THRESHOLD,
    ARCANE_TELEPORTER_HOVER_THRESHOLD,
    ARCANE_WEST_DIRECTION_POINTS,
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

# 마지막 fast 비전 결과가 이 시간보다 오래되면 family 전환을 매우 보수적으로 봄
ARCANE_FAST_SWITCH_LIMIT_MS = 120

# 조준 자체는 family 전환보다 조금 더 느슨하게 허용해 stale 프레임 고착을 줄임
ARCANE_FAST_STEER_LIMIT_MS = 350

# slow 비전(몬스터, loot, hover blocker)이 이 시간보다 오래되면 의사결정에 반영하지 않음
ARCANE_SLOW_STALE_LIMIT_MS = 450

# ray 경로 중간 샘플을 평가할 때 각 샘플 중심에서 잘라볼 패치 반경의 기본값
ARCANE_DIRECTION_PATCH_RADIUS = 42

# center -> candidate 직선 위에서 어느 지점을 샘플할지 정하는 비율
ARCANE_DIRECTION_PATH_SAMPLES = (0.45, 0.65, 0.85)

# fast 판단용 축소 맵 비율, 작을수록 빠르지만 세부 구조는 덜 봄
ARCANE_FAST_SCALE = 0.35

# 축소 후에도 패치가 너무 작아지지 않도록 보장하는 최소 반경
ARCANE_FAST_MIN_PATCH_RADIUS = 8

# north_open gate가 이 값보다 낮으면 "2시 방향이 닫혔다"고 판단
ARCANE_NORTH_OPEN_FLOOR_RATIO = 0.1

# 새 후보가 이전 후보보다 이 정도 이상 좋아야 바로 갈아타게 만드는 hysteresis 여유값
ARCANE_VOTE_KEEP_MARGIN = 0.045

# west/east family 자체를 바꿀 때 쓰는 더 큰 hysteresis 여유값
ARCANE_FAMILY_KEEP_MARGIN = 0.06

# side branch에서 north로 재진입했을 때 커서 재조준을 매우 빠르게 끝내기 위한 settle 값
ARCANE_NORTH_REOPEN_FAST_SETTLE = (0.0, 0.01)

# west/east 쪽은 커서를 화면 끝까지 뻗지 않고 중심 쪽으로 줄여 제어성을 높임
ARCANE_CURSOR_RADIUS_SCALE = 5.0 / 7.0


@dataclass(frozen=True)
class ArcaneDirectionCandidate:
    key: str
    label: str
    ratio: tuple[float, float]
    family: str
    north_family: bool = False
    bias: float = 0.0


#
# 전체 화면 기준 candidate 대략 위치
#
# +--------------------------------------------------+
# |                                  H    P          |
# |                                         S        |
# |      T  W                                        |
# |                                                  |
# |                                                  |
# |                                                  |
# |                                E                 |
# |                                   D              |
# +--------------------------------------------------+
#
# north_primary is aligned with north_open probe B.
# north_sharp is the midpoint of probes A and B.
# north_soft is the midpoint of probes B and C.
# west family is the horizontal mirror of north family.
#
# P = north_primary = (0.9167, 0.1667)
# H = north_sharp   = (0.8750, 0.1667)
# S = north_soft    = (0.9167, 0.2500)
# U = west_primary  = (0.0833, 0.1667)
# T = west_sharp    = (0.1250, 0.1667)
# W = west_soft     = (0.0833, 0.2500)
# E = east_primary  = (0.9167, 0.9167)
# I = east_soft     = (0.6667, 0.8333)
# D = east_sharp    = (0.8333, 0.6667)
ARCANE_DIRECTION_CANDIDATES: tuple[ArcaneDirectionCandidate, ...] = (
    ArcaneDirectionCandidate(
        "north_primary",
        "2 o'clock primary",
        ARCANE_NORTH_DIRECTION_POINTS["primary"],
        family="north",
        north_family=True,
        bias=0.22,
    ),
    ArcaneDirectionCandidate(
        "north_soft",
        "2 o'clock soft",
        ARCANE_NORTH_DIRECTION_POINTS["soft"],
        family="north",
        north_family=True,
        bias=0.18,
    ),
    ArcaneDirectionCandidate(
        "north_sharp",
        "2 o'clock sharp",
        ARCANE_NORTH_DIRECTION_POINTS["sharp"],
        family="north",
        north_family=True,
        bias=0.18,
    ),
    ArcaneDirectionCandidate(
        "west_primary",
        "10 o'clock primary",
        ARCANE_WEST_DIRECTION_POINTS["primary"],
        family="west",
        bias=0.04,
    ),
    ArcaneDirectionCandidate(
        "west_soft",
        "10 o'clock soft",
        ARCANE_WEST_DIRECTION_POINTS["soft"],
        family="west",
        bias=0.03,
    ),
    ArcaneDirectionCandidate(
        "west_sharp",
        "10 o'clock sharp",
        ARCANE_WEST_DIRECTION_POINTS["sharp"],
        family="west",
        bias=0.03,
    ),
    ArcaneDirectionCandidate(
        "east_primary",
        "4 o'clock primary",
        ARCANE_EAST_DIRECTION_POINTS["primary"],
        family="east",
        bias=0.02,
    ),
    ArcaneDirectionCandidate(
        "east_soft",
        "4 o'clock soft",
        ARCANE_EAST_DIRECTION_POINTS["soft"],
        family="east",
        bias=0.01,
    ),
    ArcaneDirectionCandidate(
        "east_sharp",
        "4 o'clock sharp",
        ARCANE_EAST_DIRECTION_POINTS["sharp"],
        family="east",
        bias=0.00,
    ),
)


# Arcane 북쪽 루트의 실시간 제어 루프 전체를 담당
# hub 정렬, fast/slow 비전 파이프라인, 방향 선택, 실제 커서 조준과 이동까지 여기서 이어짐
def run_arcane_north_go(session, capture) -> None:
    actions = session._resolve_arcane_character_actions()
    if actions is None or not actions.movement_skill_key:
        raise RuntimeError("Arcane North Test requires a character with movement_skill_key configured.")

    prepare_arcane_hub_start(session, capture, "north")
    zero_point_ratio = getattr(session, "_arcane_hub_focus_ratio", ARCANE_ZERO_POINT_CURSOR_RATIO)
    movement_state = MovementExecutionState()
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

    # runtime 콜백 안에서도 최신 target dict를 같은 형태로 넘기기 위한 얇은 Wrapper
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
        "last_direction_ratio": (11.0 / 12.0, 1.0 / 6.0),
        "route_family": "north",
    }
    movement_state = MovementExecutionState()

    # runtime worker에서 발생한 예외를 이벤트 로그로 올려 메인 UI가 이유를 알 수 있게 함
    def _runtime_error(stage: str, exc: Exception) -> None:
        session.events.put(session.event_class("error", f"Arcane North Test {stage} thread failed: {exc}"))

    # 고속 비전 단계
    # 방향 후보 vote와 north gate 신호처럼 자주 바뀌는 판단 값을 빠르게 계산
    def _fast_vision(frame_packet) -> dict[str, object]:
        nonlocal previous_fast_frame
        current_frame = frame_packet.frame
        recent_frames = runtime.state.snapshot().recent_frames
        trend_change = _measure_arcane_progress_trend(recent_frames)
        fast_maps = _build_arcane_fast_maps(current_frame)
        direction_votes = _score_arcane_direction_candidates(current_frame, fast_maps, control["last_direction_ratio"], zero_point_ratio)
        family_signals = _score_arcane_family_signals(current_frame, fast_maps)
        payload = {
            "progress_change": _measure_arcane_progress_change(session, previous_fast_frame, current_frame),
            "progress_trend": trend_change,
            "direction_votes": direction_votes,
            "family_signals": family_signals,
        }
        previous_fast_frame = current_frame
        return payload

    # 저속 비전 단계
    # monster, loot, hover blocker, terminal 같은 상대적으로 무거운 감지를 수행
    def _slow_vision(frame_packet) -> dict[str, object]:
        loot_hit = loot_session.scan_frame(frame_packet.frame)
        return {
            "north_terminal": _detect_arcane_north_terminal(session, frame_packet.frame),
            "loot_label": loot_hit.label if loot_hit is not None else None,
            "monster_hit": session._scan_arcane_monsters(frame_packet.frame),
            "hover_blocker_kind": _detect_arcane_hover_blocker(session, frame_packet.frame),
        }

    # fast/slow 비전 결과를 모아 실제로 어디를 가리킬지 결정하고 이동 명령까지 실행
    # stale frame 처리, north gate 우선 순위, family hysteresis, hover 회피가 모두 여기서 합쳐짐
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
        allow_side_family_switch = fast_steer_fresh if control["route_family"] in {"west", "east"} else fast_switch_fresh
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
            hover_blocker_kind = slow_payload["hover_blocker_kind"] if slow_payload is not None and slow_fresh else None
            if hover_blocker_kind is not None:
                release_movement_intent(session, actions, movement_state)
                session._sleep_range(*session.CLICK_SETTLE)
            final_ratio = _steer_arcane_movement(
                session,
                target_ref,
                latest_frame.frame,
                cursor_ratio,
                0,
                hover_blocker_kind=hover_blocker_kind,
                direction_family=direction_family,
            )
            action_phrase = apply_movement_intent(session, actions, movement_state, MOVEMENT_INTENT_TRAVEL)
            control["north_steps"] += 1
            control["last_direction_key"] = direction_key
            control["last_direction_ratio"] = final_ratio
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

        direction_votes = fast_payload["direction_votes"] if fast_payload is not None else []
        family_signals = dict(fast_payload.get("family_signals", {})) if fast_payload is not None else {}
        direction_choice = _choose_arcane_direction(
            direction_votes,
            control["last_direction_key"],
            control["route_family"],
            family_signals,
            allow_side_family_switch,
        )
        previous_route_family = control["route_family"]
        direction_key = direction_choice["key"]
        direction_label = direction_choice["label"]
        direction_family = direction_choice["family"]
        cursor_ratio = direction_choice["ratio"]
        direction_score = direction_choice["score"]
        north_open = direction_choice["north_open"]
        quick_reopen_steer = previous_route_family in {"west", "east"} and direction_family == "north"
        target_ref = _TargetRef(latest_frame.target)
        hover_blocker_kind = slow_payload["hover_blocker_kind"] if slow_payload is not None and slow_fresh else None
        if hover_blocker_kind is not None:
            release_movement_intent(session, actions, movement_state)
            session._sleep_range(*session.CLICK_SETTLE)
        final_ratio = _steer_arcane_movement(
            session,
            target_ref,
            latest_frame.frame,
            cursor_ratio,
            0,
            hover_blocker_kind=hover_blocker_kind,
            fast_reacquire=quick_reopen_steer,
            direction_family=direction_family,
        )
        action_phrase = apply_movement_intent(session, actions, movement_state, MOVEMENT_INTENT_TRAVEL)
        control["north_steps"] += 1
        control["last_direction_key"] = direction_key
        control["last_direction_ratio"] = final_ratio
        if direction_family != control["route_family"]:
            control["route_family"] = direction_family
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


# 최종 후보 ratio를 실제 커서 이동 ratio로 변환해 조준
# 필요하면 floor-guided 보정, side family 반경 축소, hover blocker 회피까지 한 번에 처리
def _steer_arcane_movement(
    session,
    capture_target,
    frame: np.ndarray,
    base_ratio: tuple[float, float],
    path_stage: int,
    hover_blocker_kind: str | None = None,
    fast_reacquire: bool = False,
    direction_family: str | None = None,
) -> tuple[float, float]:
    guided_ratio = base_ratio if fast_reacquire else _resolve_arcane_floor_guided_ratio(session, frame, base_ratio, path_stage)
    if direction_family in {"west", "east"}:
        floor_ratio = _scale_arcane_ratio_from_center(guided_ratio, ARCANE_CURSOR_RADIUS_SCALE)
    else:
        floor_ratio = guided_ratio
    session._aim_relative_ratio(capture_target, *floor_ratio, apply_jitter=False)
    if fast_reacquire:
        session._sleep_range(*ARCANE_NORTH_REOPEN_FAST_SETTLE)
    else:
        session._sleep_range(*session.CLICK_SETTLE)
    if hover_blocker_kind is None:
        return floor_ratio
    session.events.put(
        session.event_class("info", f"Arcane North Test: {hover_blocker_kind} hover detected while steering; nudging cursor away from it.")
    )
    for offset in ARCANE_HOVER_NUDGE_OFFSETS:
        nudged_ratio = session._apply_offset(floor_ratio, offset)
        session._aim_relative_ratio(capture_target, *nudged_ratio, apply_jitter=False)
        session._sleep_range(*session.CLICK_SETTLE)
        return nudged_ratio
    return floor_ratio


# 현재 프레임에서 커서 hover 때문에 이동을 방해할 수 있는 오브젝트를 찾음
# chest 계열(큰 chest, 작은 chest, 잠긴 chest, coffin)과 teleporter를 구분해서 이후 nudge 강도를 다르게 적용
def _detect_arcane_hover_blocker(session, frame: np.ndarray) -> str | None:
    chest_hover_templates = (
        session._arcane_chest_hover_template,
        session._arcane_small_chest_hover_template,
        session._arcane_small_locked_chest_hover_template,
        session._arcane_small_coffin_hover_template,
    )
    for chest_template in chest_hover_templates:
        if session._locate_template(frame, chest_template, ARCANE_CHEST_HOVER_THRESHOLD) is not None:
            return "chest"
    if session._locate_template(frame, session._arcane_teleporter_hover_template, ARCANE_TELEPORTER_HOVER_THRESHOLD) is not None:
        return "teleporter"
    return None


# 화면 중심을 기준으로 목표 ratio를 안쪽으로 당김
# 주로 west/east side branch에서 너무 먼 조준으로 제어가 거칠어지는 것을 막기 위해 사용
def _scale_arcane_ratio_from_center(ratio: tuple[float, float], scale: float) -> tuple[float, float]:
    center_x, center_y = 0.5, 0.5
    scaled_x = center_x + ((ratio[0] - center_x) * scale)
    scaled_y = center_y + ((ratio[1] - center_y) * scale)
    return (scaled_x, scaled_y)


# base_ratio 주변 후보들 중 실제 floor-like 패치가 가장 좋은 점을 골라 조준점을 미세 보정
# 특히 좁은 Arcane 통로에서 텅 빈 void를 직접 찍지 않도록 도와줌
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


# 작은 패치 하나가 얼마나 "Arcane 바닥답게" 보이는지 점수화
# floor gray는 가산하고 star/void는 감산
def _score_arcane_floor_patch(patch: np.ndarray) -> float:
    floor_ratio = palette_match_ratio(patch, ARCANE_FLOOR_GRAY_PALETTE_BGR, ARCANE_FLOOR_MAX_DISTANCE)
    star_void_ratio = palette_match_ratio(patch, ARCANE_STAR_VOID_PALETTE_BGR, ARCANE_STAR_VOID_MAX_DISTANCE)
    return (floor_ratio * 100.0) - (star_void_ratio * 80.0)


# 특정 화면 ratio 주변 패치를 잘라 _score_arcane_floor_patch에 넘긴다.
# floor-guided 조준 보정의 기본 점수 함수다.
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


# fast 판단용으로 원본 프레임을 축소하고 floor/star 여부를 빠르게 볼 수 있는 mask를 만듦
# 이후 ray 샘플 점수와 north_open probe가 이 결과를 재사용
def _build_arcane_fast_maps(frame: np.ndarray) -> dict[str, np.ndarray]:
    scaled_frame = cv.resize(frame, None, fx=ARCANE_FAST_SCALE, fy=ARCANE_FAST_SCALE, interpolation=cv.INTER_AREA)
    floor_distance = palette_distance_map_bgr(scaled_frame, ARCANE_FLOOR_GRAY_PALETTE_BGR)
    star_distance = palette_distance_map_bgr(scaled_frame, ARCANE_STAR_VOID_PALETTE_BGR)
    floor_mask = (floor_distance <= ARCANE_FLOOR_MAX_DISTANCE).astype(np.float32)
    star_mask = (star_distance <= ARCANE_STAR_VOID_MAX_DISTANCE).astype(np.float32)
    return {"floor_mask": floor_mask, "star_mask": star_mask, "scale": ARCANE_FAST_SCALE}


# 모든 방향 candidate에 대해 ray 기반 openness 점수와 bias, continuity bonus를 합쳐 vote를 만듦
# north family 후보들의 경로 점수는 north_open 보조값도 함께 계산해 둠
def _score_arcane_direction_candidates(
    frame: np.ndarray,
    fast_maps: dict[str, np.ndarray],
    previous_ratio: tuple[float, float],
    zero_point_ratio: tuple[float, float],
) -> list[dict[str, object]]:
    votes: list[dict[str, object]] = []
    north_path_open = 0.0
    for candidate in ARCANE_DIRECTION_CANDIDATES:
        path_floor_ratio, path_star_void_ratio, openness = _score_arcane_direction_path(frame, fast_maps, zero_point_ratio, candidate.ratio)
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


# north gate와 west/east family hysteresis를 적용해 이번 tick의 최종 방향 후보 하나를 고름
# "2시가 열리면 north 우선" 규칙과 "조금만 좋아서는 바로 갈아타지 않음" 규칙이 여기에 있음
def _choose_arcane_direction(
    votes: list[dict[str, object]],
    previous_key: str,
    current_family: str,
    family_signals: dict[str, float],
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
    west_family_vote = best_by_family.get("west")
    east_family_vote = best_by_family.get("east")

    if north_closed and current_family == "north":
        current_family_vote = None
        north_family_vote = None

    if north_family_vote is not None and not north_closed:
        north_family_vote["north_open"] = north_open
        return north_family_vote

    family_compare_scores: dict[str, float] = {}
    if west_family_vote is not None:
        family_compare_scores["west"] = float(west_family_vote["score"])
    if east_family_vote is not None:
        family_compare_scores["east"] = float(east_family_vote["score"])

    if family_compare_scores:
        best_family = max(family_compare_scores, key=family_compare_scores.get)
        best_vote = best_by_family[best_family]
    elif current_family_vote is not None:
        best_vote = current_family_vote
    else:
        best_vote = max(votes, key=lambda vote: float(vote["score"]))

    if not allow_family_switch and current_family_vote is not None:
        best_vote = current_family_vote
    elif (
        current_family in {"west", "east"}
        and current_family_vote is not None
        and best_vote["family"] != current_family
        and (float(best_vote["score"]) - float(current_family_vote["score"])) <= ARCANE_FAMILY_KEEP_MARGIN
    ):
        best_vote = current_family_vote

    previous_vote = next((vote for vote in votes if vote["key"] == previous_key), None)
    if (
        previous_vote is not None
        and not (north_closed and bool(previous_vote.get("north_family")))
        and (float(best_vote["score"]) - float(previous_vote["score"])) <= ARCANE_VOTE_KEEP_MARGIN
    ):
        best_vote = previous_vote
    best_vote["north_open"] = north_open
    return best_vote


# center에서 candidate까지 가는 직선 경로 위 중간 샘플들을 보고 openness를 계산
# 현재 bend 문제가 생기는 핵심 로직이기도 하며, 곡선 길을 직선 ray로 본다는 한계가 있음
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


# north_open gate 전용 점수
# upper-right 사분면 안의 원형 probe 3개를 검사하고, 그중 가장 floor-like 한 값을 사용
def _score_arcane_north_open_signal(frame: np.ndarray, fast_maps: dict[str, np.ndarray]) -> float:
    floor_mask = fast_maps["floor_mask"]
    probe_values: list[float] = []
    for probe_ratio in ARCANE_NORTH_DIRECTION_POINTS.values():
        floor_ratio = _sample_arcane_circle_at_ratio(
            floor_mask,
            probe_ratio,
            ARCANE_NORTH_WEST_OPEN_CIRCLE_RADIUS_RATIO,
        )
        probe_values.append(floor_ratio)
    if not probe_values:
        return 0.0
    return max(probe_values)


# family별 별도 gate 신호를 모음
# 현재는 north만 별도 open gate가 있고 west/east는 0으로 둠
def _score_arcane_family_signals(frame: np.ndarray, fast_maps: dict[str, np.ndarray]) -> dict[str, float]:
    return {
        "north": _score_arcane_north_open_signal(frame, fast_maps),
        "west": 0.0,
        "east": 0.0,
    }


# 저장해 둔 key를 다시 candidate 객체로 되돌림
# stale-fast hold 같은 fallback 구간에서 마지막 방향 복원에 사용
def _resolve_arcane_direction_candidate(key: str) -> ArcaneDirectionCandidate:
    for candidate in ARCANE_DIRECTION_CANDIDATES:
        if candidate.key == key:
            return candidate
    return ARCANE_DIRECTION_CANDIDATES[0]


# 두 ratio 사이를 선형 보간
# ray 샘플 지점 계산의 가장 기초가 되는 유틸
def _interpolate_ratio(start: tuple[float, float], end: tuple[float, float], t: float) -> tuple[float, float]:
    return (start[0] + ((end[0] - start[0]) * t), start[1] + ((end[1] - start[1]) * t))


# 컬러 프레임에서 특정 ratio 주변 정사각 패치를 잘라냄
# 주로 시각 디버깅이나 추가 색상 기반 점수화에 재사용할 수 있는 기본 도구
def _crop_arcane_patch_at_ratio(frame: np.ndarray, ratio: tuple[float, float], radius: int) -> np.ndarray:
    height, width = frame.shape[:2]
    center_x = int(width * ratio[0])
    center_y = int(height * ratio[1])
    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    return frame[top:bottom, left:right]


# 1채널 스칼라 이미지(mask 등)에서 특정 ratio 주변 패치를 잘라냄
# ray 샘플 구간의 floor/star 평균 계산에 사용
def _crop_arcane_scalar_patch_at_ratio(image: np.ndarray, ratio: tuple[float, float], radius: int) -> np.ndarray:
    height, width = image.shape[:2]
    center_x = int(width * ratio[0])
    center_y = int(height * ratio[1])
    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    return image[top:bottom, left:right]


# upper-right 사분면 내부의 상대 좌표 probe 하나를 원형 영역으로 샘플링
# north_open gate는 이 함수를 세 번 호출해 3개 probe 중 가장 좋은 값을 사용
def _sample_arcane_circle_at_ratio(image: np.ndarray, probe_ratio: tuple[float, float], radius_ratio: float) -> float:
    height, width = image.shape[:2]
    center_x = int(width * probe_ratio[0])
    center_y = int(height * probe_ratio[1])
    radius = max(
        ARCANE_FAST_MIN_PATCH_RADIUS,
        int(min(width, height) * 0.5 * radius_ratio),
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


# 직전 조준 방향과 가까운 후보에 작은 가산점을 줘서 미세한 흔들림을 줄임
# 큰 전략 변경이 아니라 tie-breaker에 가까운 안정화 장치
def _arcane_direction_continuity_bonus(previous_ratio: tuple[float, float], candidate_ratio: tuple[float, float]) -> float:
    dx = previous_ratio[0] - candidate_ratio[0]
    dy = previous_ratio[1] - candidate_ratio[1]
    distance = (dx * dx + dy * dy) ** 0.5
    return max(0.0, 0.18 - distance) * 0.7


# 북쪽 루트의 끝(소환사 배치 또는 비소환사 dead end)에 도달했는지 감지
# template와 summoner clue를 함께 사용해 north_go를 언제 멈출지 결정
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


# 소환사 방 근처에서만 보이는 보조 단서들을 찾아 terminal 감지를 보강
def _detect_arcane_summoner_clues(session, frame: np.ndarray) -> bool:
    for template in (
        session._arcane_horazon_journal_template,
        session._arcane_summoner_location_template,
        session._arcane_summoner_location_background_template,
    ):
        if session._locate_template(frame, template, ARCANE_SUMMONER_CLUE_THRESHOLD) is not None:
            return True
    return False


# 연속 두 프레임의 ROI 차이를 평균값으로 계산해 "얼마나 실제로 화면이 변했는지" 측정
# 텔포는 빨라도 화면 변화가 적으면 제자리 왕복이나 dead movement로 볼 수 있음
def _measure_arcane_progress_change(session, previous_frame: np.ndarray | None, current_frame: np.ndarray) -> float | None:
    if previous_frame is None:
        return None
    prev_small = cv.resize(cv.cvtColor(_arcane_progress_roi(previous_frame), cv.COLOR_BGR2GRAY), (160, 90))
    curr_small = cv.resize(cv.cvtColor(_arcane_progress_roi(current_frame), cv.COLOR_BGR2GRAY), (160, 90))
    return float(cv.absdiff(prev_small, curr_small).mean())


# 최근 여러 프레임의 progress change 평균을 계산해 순간 노이즈보다 조금 더 안정적인 진척도를 만듦
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


# UI 가장자리 영향을 줄이기 위해 화면 중앙부만 잘라 progress 비교에 사용
def _arcane_progress_roi(frame: np.ndarray) -> np.ndarray:
    height, width = frame.shape[:2]
    return frame[int(height * 0.08) : int(height * 0.82), int(width * 0.08) : int(width * 0.92)]
