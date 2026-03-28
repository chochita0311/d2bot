from __future__ import annotations

import random
import time
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
    ARCANE_EAST_SOUTH_OPEN_CIRCLE_RADIUS_RATIO,
    ARCANE_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_FLOOR_SCORE_RADIUS,
    ARCANE_FOUR_OCLOCK_FLOOR_CANDIDATE_OFFSETS,
    ARCANE_HOVER_NUDGE_OFFSETS,
    ARCANE_MOVE_STEP_SETTLE,
    ARCANE_NORTH_DIRECTION_POINTS,
    ARCANE_NORTH_WEST_OPEN_CIRCLE_RADIUS_RATIO,
    ARCANE_NORTH_TEST_TICK_SLEEP,
    ARCANE_PROGRESS_CHANGE_THRESHOLD,
    ARCANE_SHRINE_HOVER_THRESHOLD,
    ARCANE_SOUTH_DIRECTION_POINTS,
    ARCANE_TELEPORTER_HOVER_THRESHOLD,
    ARCANE_WEST_DIRECTION_POINTS,
    ARCANE_WINGS,
    ARCANE_ZERO_POINT_CURSOR_RATIO,
    detect_arcane_end,
    detect_arcane_hover_blocker,
    prepare_arcane_hub_start,
)
from diablo2.runs.summoner.routes.common.arcane_palette import (
    ARCANE_FLOOR_GRAY_PALETTE_BGR,
    ARCANE_FLOOR_MAX_DISTANCE,
    ARCANE_STAR_VOID_MAX_DISTANCE,
    ARCANE_STAR_VOID_PALETTE_BGR,
)

# Arcane 북쪽 진행 루트 메타데이터.
# 현재 GUI의 `North Go Test` 버튼이 직접 사용하는 진입점이다.
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

# 마지막 fast 비전 결과가 이 시간보다 오래되면
# family 전환은 매우 보수적으로만 허용한다.
ARCANE_FAST_SWITCH_LIMIT_MS = 120

# 조향 자체는 family 전환보다 조금 더 느슨하게 허용한다.
# stale 프레임에서도 마지막 방향 유지 조향은 이어갈 수 있다.
ARCANE_FAST_STEER_LIMIT_MS = 350

# slow 비전(monster, loot, hover blocker)이 이 시간보다 오래되면
# 의사결정에 반영하지 않는다.
ARCANE_SLOW_STALE_LIMIT_MS = 1800

# ray 경로 중간 샘플을 볼 때 사용할 patch 반경 기본값.
ARCANE_DIRECTION_PATCH_RADIUS = 35

# center -> candidate 직선 경로 위에서 볼 샘플 지점 비율.
ARCANE_DIRECTION_PATH_SAMPLES = (0.75, 0.95,)

# fast 판단용 축소 맵 비율.
# 작을수록 빠르지만 구조를 너무 잃지 않는 선으로 잡는다.
ARCANE_FAST_SCALE = 0.35

# 축소 patch가 너무 작아지지 않도록 보장하는 최소 반경.
ARCANE_FAST_MIN_PATCH_RADIUS = 8

# north_open gate가 이 값보다 낮으면
# 2시 방향 북쪽 family를 닫힌 것으로 본다.
ARCANE_NORTH_OPEN_FLOOR_RATIO = 0.35

# 새 후보가 이전 후보보다 이 정도 이상 좋아야
# 바로 갈아타도록 만드는 hysteresis 여유값.
ARCANE_VOTE_KEEP_MARGIN = 0.035

# west/east side branch에서 north로 다시 붙을 때
# 커서를 빠르게 다시 잡기 위한 매우 짧은 settle 값.
ARCANE_NORTH_REOPEN_FAST_SETTLE = (0.0, 0.01)

# west/east 조향 ratio를 화면 중심 기준으로 다시 스케일한다.
# 1.0보다 작으면 안쪽으로 줄고, 1.0이면 원래 점을 유지하며, 1.0보다 크면 더 바깥으로 뻗는다.
ARCANE_CURSOR_RADIUS_SCALE = 7.9 / 7.0
# side gate 차이가 이 값 이하면 기존 east/west family를 유지한다.
ARCANE_SIDE_GATE_KEEP_MARGIN = 0.9
# side branch에서 frame change가 이 값 이하로 반복되면 정체로 본다.
ARCANE_SIDE_STUCK_FRAME_CHANGE_THRESHOLD = 4.5
# side 정체가 이 횟수 이상 쌓이면 반대 방향 turn-around를 시도한다.
ARCANE_SIDE_STUCK_BREAK_STEPS = 3
# fast frame 판단 직후 커서 조준이 바로 이어지도록 north_go 전용 settle을 더 짧게 둔다.
ARCANE_CURSOR_FAST_SETTLE = (0.0, 0.004)
# hover blocker 때문에 movement를 놓았을 때 다시 조준하기 전 짧게만 기다린다.
ARCANE_HOVER_RELEASE_SETTLE = (0.0, 0.006)
# north_go runtime이 최신 frame을 더 자주 받도록 capture FPS를 별도로 올린다.
ARCANE_RUNTIME_CAPTURE_FPS = 30.0
# fast vision worker가 새 frame을 더 촘촘하게 다시 보도록 polling 간격을 줄인다.
ARCANE_RUNTIME_FAST_INTERVAL = 0.002
# decision worker가 최신 fast 결과를 더 빨리 반영하도록 decision 간격을 줄인다.
ARCANE_RUNTIME_DECISION_INTERVAL = 0.002
# 한 번 steering한 뒤 다음 decision으로 넘어가기 전 대기를 최소화한다.
ARCANE_DECISION_STEP_SETTLE = (0.0, 0.002)
# west/east side reposition을 몇 번마다 잠깐 멈출지 정하는 간격.
ARCANE_SIDE_REPOSITION_PAUSE_STEPS = 1
# side reposition 직후 화면이 너무 빠르게 바뀌지 않도록 짧게 멈추는 시간.
ARCANE_SIDE_REPOSITION_PAUSE = (0.23, 0.23)

# decision이 최신 frame보다 조금 뒤처진 fast snapshot은 허용하되
# 너무 오래된 결과는 cached fast payload가 있을 때만 건너뛴다.
ARCANE_FAST_SEQUENCE_GAP_LIMIT = 5

# progress trend는 cost가 큰 편이라 fast frame마다 다 계산하지 않고
# 몇 frame마다 한 번만 다시 계산해 cached 값을 재사용한다.
ARCANE_FAST_TREND_INTERVAL = 3

# ray sample 중 최고 openness와 크게 차이 나지 않으면
# 더 먼 sample을 steer target으로 유지해 너무 가까운 곳만 찍지 않게 한다.
ARCANE_STEER_SAMPLE_KEEP_MARGIN = 0.06

# steer target이 캐릭터 중심 근처 안쪽 원에 들어오지 않도록 막는 최소 중심 거리.
# 중심 (0.5, 0.5) 기준 실제 거리이며, 안쪽이면 같은 방향으로 이 경계까지 밀어낸다.
ARCANE_MIN_STEER_CENTER_DISTANCE = 0.235

@dataclass(frozen=True)
class ArcaneDirectionCandidate:
    key: str
    label: str
    ratio: tuple[float, float]
    family: str
    north_family: bool = False
    bias: float = 0.0


# 전체 화면 기준 candidate 상대 위치.
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
# P = north_primary, H = north_upper_sharp, S = north_soft
# T/W = west family, E/D = east family 보조 지점
ARCANE_DIRECTION_CANDIDATES: tuple[ArcaneDirectionCandidate, ...] = (
    ArcaneDirectionCandidate(
        "north_primary",
        "2 o'clock primary",
        ARCANE_NORTH_DIRECTION_POINTS["primary"],
        family="north",
        north_family=True,
        bias=0.0,
    ),
    ArcaneDirectionCandidate(
        "north_soft",
        "2 o'clock soft",
        ARCANE_NORTH_DIRECTION_POINTS["soft"],
        family="north",
        north_family=True,
        bias=0.0,
    ),
    ArcaneDirectionCandidate(
        "north_sharp",
        "2 o'clock sharp",
        ARCANE_NORTH_DIRECTION_POINTS["sharp"],
        family="north",
        north_family=True,
        bias=0.0,
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
        "west_lower_soft",
        "10 o'clock lower soft",
        (1.0 / 12.0, 63.0 / 128.0),
        family="west",
        bias=0.00,
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
    ArcaneDirectionCandidate(
        "east_upper_sharp",
        "4 o'clock upper sharp",
        (65.0 / 128.0, 3.0 / 4.0),
        family="east",
        bias=0.00,
    ),
)


# Arcane 북쪽 루트의 실시간 제어 루프 전체를 담당한다.
# hub 정렬, fast/slow 비전 파이프라인, 방향 선택,
# 실제 커서 조향과 이동까지 여기서 이어진다.
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

    # runtime 콜백 안에서도 최신 target dict를 같은 형태로 넘기기 위한 얇은 wrapper.
    class _TargetRef:
        def __init__(self, target: dict[str, int]):
            self.target = target

    control = {
        "north_steps": 0,
        "pause_until": 0.0,
        "last_frame_id": -1,
        "last_fast_payload": None,
        "last_fast_sequence_id": -1,
        "last_progress_trend": None,
        "last_slow_payload": None,
        "last_monster_log_at": 0.0,
        "last_loot_log_at": 0.0,
        "last_direction_key": "north_primary",
        "last_direction_ratio": ARCANE_NORTH_DIRECTION_POINTS["primary"],
        "route_family": "north",
        "side_stuck_steps": 0,
        "side_reposition_steps": 0,
    }
    movement_state = MovementExecutionState()

    # runtime worker에서 발생한 예외를 이벤트 로그로 올려
    # 메인 UI가 실패 이유를 바로 볼 수 있게 한다.
    def _runtime_error(stage: str, exc: Exception) -> None:
        session.events.put(session.event_class("error", f"Arcane North Test {stage} thread failed: {exc}"))

    # 고속 비전 단계.
    # 방향 후보 vote와 north gate 신호처럼 자주 갱신돼야 하는 값을 빠르게 계산한다.
    def _fast_vision(frame_packet) -> dict[str, object]:
        nonlocal previous_fast_frame
        started_at = time.perf_counter()
        current_frame = frame_packet.frame
        trend_change = control["last_progress_trend"]
        if trend_change is None or frame_packet.sequence_id % ARCANE_FAST_TREND_INTERVAL == 0:
            recent_frames = runtime.state.snapshot().recent_frames
            trend_change = _measure_arcane_progress_trend(recent_frames)
            control["last_progress_trend"] = trend_change
        fast_maps = _build_arcane_fast_maps(current_frame)
        direction_votes = _score_arcane_direction_candidates(current_frame, fast_maps, control["last_direction_ratio"], zero_point_ratio)
        family_signals = _score_arcane_family_signals(current_frame, fast_maps)
        payload = {
            "progress_change": _measure_arcane_progress_change(session, previous_fast_frame, current_frame),
            "progress_trend": trend_change,
            "direction_votes": direction_votes,
            "family_signals": family_signals,
            "fast_proc_ms": round((time.perf_counter() - started_at) * 1000, 1),
        }
        previous_fast_frame = current_frame
        return payload

    # 저속 비전 단계.
    # monster, loot, hover blocker, end 같은 상대적으로 무거운 감지를 수행한다.
    def _slow_vision(frame_packet) -> dict[str, object]:
        loot_hit = loot_session.scan_frame(frame_packet.frame)
        return {
            "end": detect_arcane_end(session, frame_packet.frame),
            "loot_label": loot_hit.label if loot_hit is not None else None,
            "monster_hit": session._scan_arcane_monsters(frame_packet.frame),
            "hover_blocker_kind": detect_arcane_hover_blocker(session, frame_packet.frame),
        }

    # fast/slow 비전 결과를 모아 실제로 어디를 가리킬지 결정하고 이동 명령까지 수행한다.
    # stale frame 처리, north gate 우선순위, family hysteresis, hover 회피가 모두 여기서 합쳐진다.
    def _decision(snapshot: RuntimeSnapshot) -> dict[str, object] | None:
        if session._check_for_user_interrupt():
            raise RuntimeError("stopped by user interference")
        latest_frame = snapshot.latest_frame
        if latest_frame is None or latest_frame.sequence_id == control["last_frame_id"]:
            return None
        control["last_frame_id"] = latest_frame.sequence_id

        latest_end = detect_arcane_end(session, latest_frame.frame)
        if latest_end is not None:
            session.events.put(
                session.event_class("info", "Arcane North Test: detected Arcane goal center on latest frame; stopping north run here.")
            )
            session.request_stop()
            return {"status": "end_latest_frame"}

        now = snapshot.sampled_at
        fast_payload = control["last_fast_payload"]
        fast_sequence_gap = None
        if snapshot.fast_vision is not None:
            fast_sequence_gap = latest_frame.sequence_id - snapshot.fast_vision.source_sequence_id
            if fast_sequence_gap <= ARCANE_FAST_SEQUENCE_GAP_LIMIT or fast_payload is None:
                fast_payload = snapshot.fast_vision.payload
                control["last_fast_payload"] = fast_payload
                control["last_fast_sequence_id"] = snapshot.fast_vision.source_sequence_id
        slow_payload = control["last_slow_payload"]
        if snapshot.slow_vision is not None:
            slow_payload = snapshot.slow_vision.payload
            control["last_slow_payload"] = slow_payload

        frame_age_ms = int((now - latest_frame.captured_at) * 1000)
        fast_age_ms = int((now - snapshot.fast_vision.source_captured_at) * 1000) if snapshot.fast_vision is not None else None
        slow_age_ms = int((now - snapshot.slow_vision.source_captured_at) * 1000) if snapshot.slow_vision is not None else None
        fast_proc_ms = fast_payload.get("fast_proc_ms") if fast_payload is not None else None
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
                session._sleep_range(*ARCANE_HOVER_RELEASE_SETTLE)
            final_ratio = _steer_arcane_movement(
                session,
                target_ref,
                latest_frame.frame,
                cursor_ratio,
                0,
                hover_blocker_kind=hover_blocker_kind,
                direction_family=direction_family,
            )
            movement_intent = MOVEMENT_INTENT_TRAVEL if direction_family == "north" else MOVEMENT_INTENT_REPOSITION
            action_phrase = apply_movement_intent(session, actions, movement_state, movement_intent)
            control["north_steps"] += 1
            control["last_direction_key"] = direction_key
            control["last_direction_ratio"] = final_ratio
            side_pause_note = _apply_arcane_side_reposition_pause(session, control, direction_family)
            session.events.put(
                session.event_class(
                    "info",
                    f"Arcane North Test: step {control['north_steps']}, family={direction_family}, choice={direction_label}, vote={direction_score:.3f}, north_open={north_open:.3f}, frame_change=None, frame_age={frame_age_ms}ms, fast_age={fast_age_ms}ms, slow_age={slow_age_ms}ms, fast_gap={fast_sequence_gap}, fast_proc={fast_proc_ms}ms, base_ratio={cursor_ratio}, final_ratio={final_ratio} {action_phrase}{side_pause_note} (stale-fast hold)",
                )
            )
            session._sleep_range(*ARCANE_DECISION_STEP_SETTLE)
            return {
                "status": "stale_fast_hold",
                "frame_age_ms": frame_age_ms,
                "fast_age_ms": fast_age_ms,
                "slow_age_ms": slow_age_ms,
                "fast_sequence_gap": fast_sequence_gap,
                "direction_key": direction_key,
                "final_ratio": final_ratio,
            }

        if slow_payload is not None and slow_fresh and slow_payload["end"] is not None:
            session.events.put(
                session.event_class("info", "Arcane North Test: detected Arcane goal center; stopping north run here.")
            )
            session.request_stop()
            return {"status": "end", "frame_age_ms": frame_age_ms, "fast_age_ms": fast_age_ms, "slow_age_ms": slow_age_ms}

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
        if control["route_family"] in {"west", "east"} and frame_change is not None:
            if frame_change <= ARCANE_SIDE_STUCK_FRAME_CHANGE_THRESHOLD:
                control["side_stuck_steps"] += 1
            else:
                control["side_stuck_steps"] = 0
        else:
            control["side_stuck_steps"] = 0

        direction_votes = fast_payload["direction_votes"] if fast_payload is not None else []
        family_signals = dict(fast_payload.get("family_signals", {})) if fast_payload is not None else {}
        side_stuck_break = control["side_stuck_steps"] >= ARCANE_SIDE_STUCK_BREAK_STEPS
        direction_choice = _choose_arcane_direction(
            direction_votes,
            control["last_direction_key"],
            control["route_family"],
            family_signals,
            allow_side_family_switch,
            side_stuck_break,
        )
        previous_route_family = control["route_family"]
        direction_key = direction_choice["key"]
        direction_label = direction_choice["label"]
        direction_family = direction_choice["family"]
        cursor_ratio = direction_choice.get("steer_ratio", direction_choice["ratio"])
        direction_score = direction_choice["score"]
        north_open = direction_choice["north_open"]
        quick_reopen_steer = previous_route_family in {"west", "east"} and direction_family == "north"
        target_ref = _TargetRef(latest_frame.target)
        hover_blocker_kind = slow_payload["hover_blocker_kind"] if slow_payload is not None and slow_fresh else None
        if hover_blocker_kind is not None:
            release_movement_intent(session, actions, movement_state)
            session._sleep_range(*ARCANE_HOVER_RELEASE_SETTLE)
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
        movement_intent = MOVEMENT_INTENT_TRAVEL if direction_family == "north" else MOVEMENT_INTENT_REPOSITION
        action_phrase = apply_movement_intent(session, actions, movement_state, movement_intent)
        control["north_steps"] += 1
        control["last_direction_key"] = direction_key
        control["last_direction_ratio"] = final_ratio
        if direction_family != control["route_family"]:
            control["route_family"] = direction_family
        if direction_family not in {"west", "east"} or direction_family != previous_route_family:
            control["side_stuck_steps"] = 0
        side_pause_note = _apply_arcane_side_reposition_pause(session, control, direction_family)
        session.events.put(
            session.event_class(
                "info",
                f"Arcane North Test: step {control['north_steps']}, family={direction_family}, choice={direction_label}, vote={direction_score:.3f}, north_open={north_open:.3f}, frame_change={frame_change}, side_stuck_steps={control['side_stuck_steps']}, frame_age={frame_age_ms}ms, fast_age={fast_age_ms}ms, slow_age={slow_age_ms}ms, fast_gap={fast_sequence_gap}, fast_proc={fast_proc_ms}ms, base_ratio={cursor_ratio}, final_ratio={final_ratio} {action_phrase}{side_pause_note}",
            )
        )
        session._sleep_range(*ARCANE_DECISION_STEP_SETTLE)
        return {
            "status": "move",
            "frame_age_ms": frame_age_ms,
            "fast_age_ms": fast_age_ms,
            "slow_age_ms": slow_age_ms,
            "fast_sequence_gap": fast_sequence_gap,
            "direction_key": direction_key,
            "final_ratio": final_ratio,
        }

    runtime = RealtimeVisionRuntime(
        session.config.capture,
        session._stop_event,
        _fast_vision,
        _slow_vision,
        _decision,
        fast_interval=ARCANE_RUNTIME_FAST_INTERVAL,
        slow_interval=0.03,
        decision_interval=ARCANE_RUNTIME_DECISION_INTERVAL,
        capture_fps=ARCANE_RUNTIME_CAPTURE_FPS,
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


def _apply_arcane_side_reposition_pause(session, control: dict[str, object], direction_family: str) -> str:
    if direction_family not in {"west", "east"}:
        control["side_reposition_steps"] = 0
        return ""
    control["side_reposition_steps"] = int(control["side_reposition_steps"]) + 1
    if int(control["side_reposition_steps"]) < ARCANE_SIDE_REPOSITION_PAUSE_STEPS:
        return ""
    control["side_reposition_steps"] = 0
    session._sleep_range(*ARCANE_SIDE_REPOSITION_PAUSE)
    return " (side reposition pause)"


# 최종 후보 ratio를 실제 커서 이동 ratio로 바꿔 조정한다.
# 필요하면 floor-guided 보정, side family 반경 축소, hover blocker 회피까지 한 번에 처리한다.
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
        session._sleep_range(*ARCANE_CURSOR_FAST_SETTLE)
    if hover_blocker_kind is None:
        return floor_ratio
    session.events.put(
        session.event_class("info", f"Arcane North Test: {hover_blocker_kind} hover detected while steering; nudging cursor away from it.")
    )
    for offset in ARCANE_HOVER_NUDGE_OFFSETS:
        nudged_ratio = session._apply_offset(floor_ratio, offset)
        session._aim_relative_ratio(capture_target, *nudged_ratio, apply_jitter=False)
        session._sleep_range(*ARCANE_CURSOR_FAST_SETTLE)
        return nudged_ratio
    return floor_ratio


# 화면 중심을 기준으로 목표 ratio를 안쪽으로 줄인다.
# 주로 west/east side branch에서 커서를 너무 바깥으로 보내는 일을 막기 위해 사용한다.
def _scale_arcane_ratio_from_center(ratio: tuple[float, float], scale: float) -> tuple[float, float]:
    center_x, center_y = 0.5, 0.5
    scaled_x = center_x + ((ratio[0] - center_x) * scale)
    scaled_y = center_y + ((ratio[1] - center_y) * scale)
    return (scaled_x, scaled_y)


# base_ratio 주변 후보 중 실제 floor-like 위치가 가장 좋아 보이는 점을 골라
# 조향 비율을 미세 보정한다.
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


# 작은 patch 하나가 얼마나 Arcane 바닥답게 보이는지 점수화한다.
# floor gray는 가산하고 star/void는 감산한다.
def _score_arcane_floor_patch(patch: np.ndarray) -> float:
    floor_ratio = palette_match_ratio(patch, ARCANE_FLOOR_GRAY_PALETTE_BGR, ARCANE_FLOOR_MAX_DISTANCE)
    star_void_ratio = palette_match_ratio(patch, ARCANE_STAR_VOID_PALETTE_BGR, ARCANE_STAR_VOID_MAX_DISTANCE)
    return (floor_ratio * 100.0) - (star_void_ratio * 80.0)


# 특정 화면 ratio 주변 patch를 잘라
# floor-guided 조향 보정에 쓸 기본 점수를 계산한다.
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


# fast 판단용으로 원본 프레임을 축소하고
# floor/star 여부를 빠르게 볼 수 있는 mask를 만든다.
def _build_arcane_fast_maps(frame: np.ndarray) -> dict[str, np.ndarray]:
    scaled_frame = cv.resize(frame, None, fx=ARCANE_FAST_SCALE, fy=ARCANE_FAST_SCALE, interpolation=cv.INTER_AREA)
    floor_distance = palette_distance_map_bgr(scaled_frame, ARCANE_FLOOR_GRAY_PALETTE_BGR)
    star_distance = palette_distance_map_bgr(scaled_frame, ARCANE_STAR_VOID_PALETTE_BGR)
    floor_mask = (floor_distance <= ARCANE_FLOOR_MAX_DISTANCE).astype(np.float32)
    star_mask = (star_distance <= ARCANE_STAR_VOID_MAX_DISTANCE).astype(np.float32)
    return {"floor_mask": floor_mask, "star_mask": star_mask, "scale": ARCANE_FAST_SCALE}


# 모든 방향 candidate에 대해
# ray 기반 openness 점수, bias, continuity bonus를 합친 vote를 만든다.
def _score_arcane_direction_candidates(
    frame: np.ndarray,
    fast_maps: dict[str, np.ndarray],
    previous_ratio: tuple[float, float],
    zero_point_ratio: tuple[float, float],
) -> list[dict[str, object]]:
    votes: list[dict[str, object]] = []
    north_path_open = 0.0
    for candidate in ARCANE_DIRECTION_CANDIDATES:
        path_floor_ratio, path_star_void_ratio, openness, steer_ratio = _score_arcane_direction_path(
            frame,
            fast_maps,
            zero_point_ratio,
            candidate.ratio,
        )
        continuity_bonus = _arcane_direction_continuity_bonus(previous_ratio, candidate.ratio)
        score = openness + candidate.bias + continuity_bonus
        vote = {
            "key": candidate.key,
            "label": candidate.label,
            "ratio": candidate.ratio,
            "steer_ratio": steer_ratio,
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


# north gate와 west/east family hysteresis를 적용해
# 이번 tick의 최종 방향 후보 하나를 고른다.
def _choose_arcane_direction(
    votes: list[dict[str, object]],
    previous_key: str,
    current_family: str,
    family_signals: dict[str, float],
    allow_family_switch: bool,
    side_stuck_break: bool = False,
) -> dict[str, object]:
    if not votes:
        fallback = ARCANE_DIRECTION_CANDIDATES[0]
        return {
            "key": fallback.key,
            "label": fallback.label,
            "family": fallback.family,
            "ratio": fallback.ratio,
            "steer_ratio": fallback.ratio,
            "score": 0.0,
            "north_open": 0.0,
        }
    north_open = max(0.0, float(family_signals.get("north", 0.0)))
    west_open = max(0.0, float(family_signals.get("west", 0.0)))
    east_open = max(0.0, float(family_signals.get("east", 0.0)))
    by_family: dict[str, list[dict[str, object]]] = {}
    for vote in votes:
        by_family.setdefault(str(vote["family"]), []).append(vote)

    best_by_family = {family: max(items, key=lambda vote: float(vote["score"])) for family, items in by_family.items()}
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

    if side_stuck_break and current_family in {"west", "east"}:
        turnaround_family = "east" if current_family == "west" else "west"
        turnaround_vote = best_by_family.get(turnaround_family)
        if turnaround_vote is not None:
            turnaround_vote["north_open"] = north_open
            return turnaround_vote

    side_family_vote = _choose_arcane_side_family_vote(
        west_family_vote,
        east_family_vote,
        current_family_vote,
        current_family,
        previous_key,
        west_open,
        east_open,
        allow_family_switch,
        side_stuck_break,
    )
    if side_family_vote is not None:
        best_vote = side_family_vote
    elif current_family_vote is not None:
        best_vote = current_family_vote
    else:
        best_vote = max(votes, key=lambda vote: float(vote["score"]))
    best_vote["north_open"] = north_open
    return best_vote


# west/east side family 중 어느 쪽을 유지하거나 전환할지 고른다.
# gate 신호와 이전 선택 유지 여유값을 함께 본다.
def _choose_arcane_side_family_vote(
    west_family_vote: dict[str, object] | None,
    east_family_vote: dict[str, object] | None,
    current_family_vote: dict[str, object] | None,
    current_family: str,
    previous_key: str,
    west_open: float,
    east_open: float,
    allow_family_switch: bool,
    side_stuck_break: bool = False,
) -> dict[str, object] | None:
    side_votes: dict[str, dict[str, object]] = {}
    side_gates: dict[str, float] = {}
    if west_family_vote is not None:
        side_votes["west"] = west_family_vote
        side_gates["west"] = west_open
    if east_family_vote is not None:
        side_votes["east"] = east_family_vote
        side_gates["east"] = east_open
    if not side_votes:
        return None

    best_family = max(side_gates, key=side_gates.get)
    best_vote = side_votes[best_family]

    if side_stuck_break:
        current_family_vote = None
    elif not allow_family_switch and current_family in side_votes and current_family_vote is not None:
        return current_family_vote

    if not side_stuck_break and current_family in side_votes and current_family_vote is not None and best_family != current_family:
        gate_margin = side_gates[best_family] - side_gates[current_family]
        if gate_margin <= ARCANE_SIDE_GATE_KEEP_MARGIN:
            best_family = current_family
            best_vote = current_family_vote

    previous_vote = None if side_stuck_break else next((vote for vote in side_votes.values() if vote["key"] == previous_key), None)
    if previous_vote is not None and previous_vote["family"] == best_family:
        score_margin = float(best_vote["score"]) - float(previous_vote["score"])
        if score_margin <= ARCANE_VOTE_KEEP_MARGIN:
            best_vote = previous_vote

    return best_vote


# center에서 candidate까지 가는 경로 중간 샘플을 모아 openness를 계산한다.
# 급한 bend가 있어도 중간 ray 샘플에서 구조를 읽도록 만든다.
def _score_arcane_direction_path(
    frame: np.ndarray,
    fast_maps: dict[str, np.ndarray],
    zero_point_ratio: tuple[float, float],
    candidate_ratio: tuple[float, float],
) -> tuple[float, float, float, tuple[float, float]]:
    floor_values: list[float] = []
    star_values: list[float] = []
    sample_scores: list[tuple[float, float, tuple[float, float]]] = []
    scaled_radius = max(ARCANE_FAST_MIN_PATCH_RADIUS, int(ARCANE_DIRECTION_PATCH_RADIUS * float(fast_maps.get("scale", 1.0))))
    for t in ARCANE_DIRECTION_PATH_SAMPLES:
        sample_ratio = _interpolate_ratio(zero_point_ratio, candidate_ratio, t)
        left, right, top, bottom = _resolve_arcane_patch_bounds(fast_maps["floor_mask"], sample_ratio, scaled_radius)
        floor_patch = fast_maps["floor_mask"][top:bottom, left:right]
        star_patch = fast_maps["star_mask"][top:bottom, left:right]
        if floor_patch.size == 0 or star_patch.size == 0:
            continue
        floor_value = float(floor_patch.mean())
        star_value = float(star_patch.mean())
        floor_values.append(floor_value)
        star_values.append(star_value)
        sample_openness = floor_value - (star_value * 0.85)
        sample_scores.append((sample_openness, t, sample_ratio))
    if not floor_values:
        return 0.0, 1.0, -1.0, candidate_ratio
    floor_ratio = float(sum(floor_values) / len(floor_values))
    star_void_ratio = float(sum(star_values) / len(star_values))
    openness = floor_ratio - (star_void_ratio * 0.85)
    best_sample_openness = max(score for score, _, _ in sample_scores)
    eligible_samples = [
        (t, sample_ratio)
        for score, t, sample_ratio in sample_scores
        if score >= (best_sample_openness - ARCANE_STEER_SAMPLE_KEEP_MARGIN)
    ]
    if not eligible_samples:
        steer_ratio = candidate_ratio
    else:
        steer_ratio = max(eligible_samples, key=lambda item: item[0])[1]
    steer_ratio = _enforce_arcane_min_center_distance(steer_ratio, (0.5, 0.5), ARCANE_MIN_STEER_CENTER_DISTANCE)
    return floor_ratio, star_void_ratio, openness, steer_ratio


# north_open gate 계산용 함수.
# upper-right 계열 probe 3개를 훑어 가장 많이 열린 floor-like 값을 고른다.
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


# west family probe 기준으로 열린 정도를 계산한다.
def _score_arcane_west_open_signal(frame: np.ndarray, fast_maps: dict[str, np.ndarray]) -> float:
    floor_mask = fast_maps["floor_mask"]
    probe_values: list[float] = []
    for probe_ratio in ARCANE_WEST_DIRECTION_POINTS.values():
        floor_ratio = _sample_arcane_circle_at_ratio(
            floor_mask,
            probe_ratio,
            ARCANE_NORTH_WEST_OPEN_CIRCLE_RADIUS_RATIO,
        )
        probe_values.append(floor_ratio)
    if not probe_values:
        return 0.0
    return max(probe_values)


# east family probe 기준으로 열린 정도를 계산한다.
def _score_arcane_east_open_signal(frame: np.ndarray, fast_maps: dict[str, np.ndarray]) -> float:
    floor_mask = fast_maps["floor_mask"]
    probe_values: list[float] = []
    for probe_ratio in ARCANE_EAST_DIRECTION_POINTS.values():
        floor_ratio = _sample_arcane_circle_at_ratio(
            floor_mask,
            probe_ratio,
            ARCANE_EAST_SOUTH_OPEN_CIRCLE_RADIUS_RATIO,
        )
        probe_values.append(floor_ratio)
    if not probe_values:
        return 0.0
    return max(probe_values)


# family별 gate 신호를 한 번에 계산한다.
def _score_arcane_family_signals(frame: np.ndarray, fast_maps: dict[str, np.ndarray]) -> dict[str, float]:
    return {
        "north": _score_arcane_north_open_signal(frame, fast_maps),
        "west": _score_arcane_west_open_signal(frame, fast_maps),
        "east": _score_arcane_east_open_signal(frame, fast_maps),
    }


# 문자열 key에 맞는 candidate 정의를 찾는다.
# stale-fast hold에서 fallback 후보를 복원할 때 쓴다.
def _resolve_arcane_direction_candidate(key: str) -> ArcaneDirectionCandidate:
    for candidate in ARCANE_DIRECTION_CANDIDATES:
        if candidate.key == key:
            return candidate
    return ARCANE_DIRECTION_CANDIDATES[0]


# 두 ratio 사이를 선형 보간한다.
def _interpolate_ratio(start: tuple[float, float], end: tuple[float, float], t: float) -> tuple[float, float]:
    return (start[0] + ((end[0] - start[0]) * t), start[1] + ((end[1] - start[1]) * t))


# steer target이 캐릭터 중심 근처 안쪽 원에 들어오면
# 같은 방향으로 최소 반경 경계까지 밀어내어 너무 가까운 조준을 막는다.
def _enforce_arcane_min_center_distance(
    ratio: tuple[float, float],
    center_ratio: tuple[float, float],
    min_distance: float,
) -> tuple[float, float]:
    dx = ratio[0] - center_ratio[0]
    dy = ratio[1] - center_ratio[1]
    distance = float((dx * dx + dy * dy) ** 0.5)
    if distance <= 0.0:
        return ratio
    if distance >= min_distance:
        return ratio
    scale = min_distance / distance
    adjusted_x = center_ratio[0] + (dx * scale)
    adjusted_y = center_ratio[1] + (dy * scale)
    return (max(0.0, min(1.0, adjusted_x)), max(0.0, min(1.0, adjusted_y)))


# 컬러 프레임에서 특정 ratio 주변 patch를 잘라낸다.
def _crop_arcane_patch_at_ratio(frame: np.ndarray, ratio: tuple[float, float], radius: int) -> np.ndarray:
    height, width = frame.shape[:2]
    center_x = int(width * ratio[0])
    center_y = int(height * ratio[1])
    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    return frame[top:bottom, left:right]


# 1채널 mask 이미지에서 특정 ratio 주변 patch를 잘라낸다.
def _crop_arcane_scalar_patch_at_ratio(image: np.ndarray, ratio: tuple[float, float], radius: int) -> np.ndarray:
    left, right, top, bottom = _resolve_arcane_patch_bounds(image, ratio, radius)
    return image[top:bottom, left:right]


def _resolve_arcane_patch_bounds(image: np.ndarray, ratio: tuple[float, float], radius: int) -> tuple[int, int, int, int]:
    height, width = image.shape[:2]
    center_x = int(width * ratio[0])
    center_y = int(height * ratio[1])
    left = max(0, center_x - radius)
    right = min(width, center_x + radius)
    top = max(0, center_y - radius)
    bottom = min(height, center_y + radius)
    return left, right, top, bottom


# probe 원 안의 평균값으로 해당 지점이 얼마나 floor-like 한지 본다.
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


# 이전 방향과 너무 멀지 않은 후보에 작은 연속성 보너스를 준다.
def _arcane_direction_continuity_bonus(previous_ratio: tuple[float, float], candidate_ratio: tuple[float, float]) -> float:
    dx = previous_ratio[0] - candidate_ratio[0]
    dy = previous_ratio[1] - candidate_ratio[1]
    distance = (dx * dx + dy * dy) ** 0.5
    return max(0.0, 0.18 - distance) * 0.7


# 두 프레임의 ROI 차이를 평균값으로 구해
# 진행 변화가 있었는지 빠르게 본다.
def _measure_arcane_progress_change(session, previous_frame: np.ndarray | None, current_frame: np.ndarray) -> float | None:
    if previous_frame is None:
        return None
    prev_small = cv.resize(cv.cvtColor(_arcane_progress_roi(previous_frame), cv.COLOR_BGR2GRAY), (160, 90))
    curr_small = cv.resize(cv.cvtColor(_arcane_progress_roi(current_frame), cv.COLOR_BGR2GRAY), (160, 90))
    return float(cv.absdiff(prev_small, curr_small).mean())


# 최근 몇 프레임의 progress change 평균을 내서
# 한 프레임 노이즈보다 더 안정적인 변화량을 얻는다.
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


# UI 가장자리 영향을 줄이기 위해 내부 영역만 progress 비교에 사용한다.
def _arcane_progress_roi(frame: np.ndarray) -> np.ndarray:
    height, width = frame.shape[:2]
    return frame[int(height * 0.08) : int(height * 0.82), int(width * 0.08) : int(width * 0.92)]
