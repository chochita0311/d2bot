from __future__ import annotations

import math
import json
import threading
import time
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
from collections import deque
from dataclasses import asdict, replace

import cv2 as cv
import numpy as np

from diablo2.common.field_control import FieldDirective, HeldFieldInput
from diablo2.common.frame_geometry import desktop_to_frame, frame_to_desktop
from diablo2.common.hostile_probe import NearbyHostileProbe, nearby_actor_proposals
from diablo2.common.scene_motion import BackgroundMotionObserver
from diablo2.common.supervised_field import ensure_automap_on, resource_guard, validate_permit
from diablo2.runs.summoner.navigation_execution import dispatch_planned_step, plan_arcane_step
from diablo2.runs.summoner.navigation import HEADINGS
from diablo2.runs.summoner.steering import ArcaneRouteFlow, ContinuousSteering, floor_supports_aim
from diablo2.runs.summoner.terminal_vision import TerminalStabilityTracker
from diablo2.runs.summoner.scene_vision import HubStabilityTracker


OPPOSITE_WINGS = dict(north="south", east="west", south="north", west="east")
TERMINAL_STATIONARY_REACQUISITION_SECONDS = 6.0
# 왼쪽 위는 커서의 푸른 효과가 용병 얼굴을 가린다. 기준 프레임의 제목 표시줄 중앙으로 치운다.
POINTER_CLEAR_POINT = (961, 24)
HUB_LANDING_MINIMUM_DISPLACEMENT = 64


def current_hub_landing(progress, hub, player, evidence, displacement, acquired, sequence):
    """큰 화면 이동으로 배경 대응이 끊겼으면 현재 전체 WP 두 장의 상대 이동만 확인한다."""
    origin = progress.get("hub_approach_origin")
    relative = tuple(a - b for a, b in zip(hub, player))
    if (
        origin is None
        or not 0 < acquired - origin["at"] < 1.5
        or not isinstance(evidence, dict)
        or evidence.get("kind") != "blue_pair_and_floor_ornaments"
        or displacement is None
        or math.hypot(*displacement) >= 2
        or math.hypot(*(a - b for a, b in zip(relative, origin["relative"]))) < HUB_LANDING_MINIMUM_DISPLACEMENT
    ):
        progress.pop("hub_approach_landing", None)
        return False
    previous = progress.get("hub_approach_landing")
    progress["hub_approach_landing"] = dict(relative=relative, at=acquired, sequence=sequence)
    return (
        previous is not None
        and sequence == previous["sequence"] + 1
        and 0 < acquired - previous["at"] < 0.25
        and math.hypot(*(a - b for a, b in zip(relative, previous["relative"]))) < 2
    )


class ArrivalDwell:
    """keyUp 전에 시작된 시전까지 끝난 뒤 현재 근접·정지를 확인한다."""

    def __init__(self, reference_seconds):
        self.minimum_seconds = reference_seconds
        self.reset()

    def reset(self):
        self.since, self.stable = None, 0

    def update(self, near, displacement, now):
        if not near:
            self.reset()
            return False
        if self.since is None or displacement is None or math.hypot(*displacement) >= 2:
            self.since, self.stable = now, 0
        if displacement is not None and math.hypot(*displacement) < 2:
            self.stable += 1
        return self.stable >= 2 and now - self.since >= self.minimum_seconds


def restore_route_flow(report_path, frame, *, wing, return_from, maximum_pending_distance=600):
    """같은 현재 지형의 대응을 검증한 뒤 국소 꺾임만 복원한다. 과거 위치/버프는 복원하지 않는다."""
    report_path = Path(report_path)
    report = json.loads(report_path.read_text(encoding="utf-8"))
    navigation = report.get("navigation", {})
    # 최초 요청 뒤 같은 프로세스에서 끝→복귀로 전환했을 수 있다. 마지막 실제 관측 단계에 맞춰 재개한다.
    if (
        report.get("continuous") is not True
        or navigation.get("wing", report.get("wing")) != wing
        or navigation.get("return_from", report.get("return_from")) != return_from
    ):
        raise RuntimeError("continuation route contract differs")
    detour = navigation.get("route_detour")
    if "route_detour" not in navigation or (
        detour is not None
        and (
            not isinstance(detour, (list, tuple))
            or len(detour) != 2
            or any(type(v) not in (int, float) or not math.isfinite(v) for v in detour)
            or not math.hypot(*detour)
        )
    ):
        raise RuntimeError("continuation bend missing")
    rows = navigation.get("continuous_frames", [])
    if not rows or rows[-1].get("ground", {}).get("point") is None:
        raise RuntimeError("continuation ground missing")
    final_frame = navigation.get("final_frame") or "continuous-{:04d}-after.png".format(len(rows))
    if not isinstance(final_frame, str) or Path(final_frame).name != final_frame:
        raise RuntimeError("continuation frame must belong to report directory")
    source = cv.imread(str(report_path.parent / final_frame))
    if source is None or source.shape != frame.shape or frame.shape != (1140, 1922, 3):
        raise RuntimeError("continuation native scene missing")
    mask = np.zeros(frame.shape[:2], np.uint8)
    mask[180:890, 120:1720] = 255
    x, y = rows[-1]["ground"]["point"]
    mask[max(180, y - 230) : min(890, y + 120), max(120, x - 260) : min(1720, x + 260)] = 0
    observer = BackgroundMotionObserver(minimum_matches=24, minimum_inlier_ratio=0.65, maximum_error=3)
    observer.observe_mask(source, mask, sequence=1, captured_at=0)
    measured = observer.observe_mask(frame, mask, sequence=2, captured_at=1)
    # keyUp 직전 시작된 마지막 텔레포트는 저장 이후에도 완료될 수 있다.
    # 같은 지형의 실측 대응만 허용하고 최대 한 시전 범위보다 먼 위치는 이어받지 않는다.
    if measured.displacement is None or math.hypot(*measured.displacement) > maximum_pending_distance:
        raise RuntimeError("continuation current scene unverified")
    return None if detour is None else tuple(detour), asdict(measured)


def near_ground(point, player, radius=(300, 180)):
    return point is not None and math.hypot((point[0] - player[0]) / radius[0], (point[1] - player[1]) / radius[1]) <= 1


def validate_travel_permit(proof, policy, now):
    if proof.get("source") == "runtime_screen":
        from diablo2.common.automatic_field import validate_runtime_permit

        validate_runtime_permit(proof, now)
        return
    # 전투 허용은 이동 시험에만 적용하며 버프의 주변 안전 확인을 대체하지 않는다.
    if type(proof.get("monsters_clear")) is not bool:
        raise ValueError("current nearby hostile presence must be explicitly observed")
    validate_permit(dict(proof, monsters_clear=True), "flash", policy, now, buff=True)


def validate_live_continuation(progress, wing, return_from, now):
    """같은 프로세스의 방금 끝난 연속 구간만 잇는다. 오래된 감독 확인 시각을 갱신하지 않는다."""
    rows = progress.get("continuous_frames", [])
    previous = None if not rows else rows[-1].get("processed_at")
    finished = progress.get("interval_finished_at", previous)
    if (
        progress.get("continuous_status") != "interval_complete"
        or progress.get("wing") != wing
        or progress.get("return_from") != return_from
        or type(previous) not in (int, float)
        or not math.isfinite(previous)
        or type(finished) not in (int, float)
        or not math.isfinite(finished)
        or finished < previous
        or not 0 <= now - finished < 0.5
    ):
        raise ValueError("immediate same-process field continuation required")


def validate_terminal_return(previous, wing, return_from, now):
    """실제 세 상자 끝·정지 직후에만 같은 날개의 중앙 복귀 목표로 전환한다."""
    rows = previous.get("continuous_frames", [])
    last = rows[-1] if rows else {}
    at = last.get("processed_at")
    terminal = previous.get("terminal", {})
    reference_at = terminal.get("reference_observed_at")
    observed_at = last.get("acquired_at")
    parts = terminal.get("current_parts", ())
    tracked = (
        previous.get("continuous_status") == "recent_terminal_layout_and_player_settled"
        and terminal.get("kind") in ("nonboss_chest_trio", "nonboss_chest_platform")
        and terminal.get("provenance") in ("current_static_two_parts", "current_relocalized_two_parts", "current_motion_settled_two_parts")
        and isinstance(parts, (list, tuple))
        and len(parts) == 3
        and all(type(value) is bool for value in parts)
        and sum(parts) >= 2
        and all(type(value) in (int, float) and math.isfinite(value) for value in (reference_at, observed_at))
        and 0 <= observed_at - reference_at <= 2
    )
    direct = (terminal.get("kind"), previous.get("continuous_status")) in (
        ("nonboss_chest_trio", "current_chest_trio_and_player_settled"),
        ("nonboss_chest_platform", "current_terminal_landmarks_and_player_settled"),
    )
    if (
        previous.get("status") != "wing_end_observed"
        or not (tracked or direct)
        or previous.get("wing") != wing
        or previous.get("return_from") is not None
        or return_from != wing
        or last.get("action") != "terminal_arrival_hold"
        or type(at) not in (int, float)
        or not math.isfinite(at)
        or not 0 <= now - at < 0.5
    ):
        raise ValueError("immediate observed terminal-to-hub transition required")


def validate_hub_departure(previous, wing, return_from, now):
    """중앙 복귀 정지 확인 직후에만 다음 날개의 출발을 연결한다."""
    order = ("north", "east", "south", "west")
    previous_wing = previous.get("wing")
    rows = previous.get("continuous_frames", [])
    last = rows[-1] if rows else {}
    at = last.get("processed_at")
    if (
        previous.get("status") != "hub_arrived"
        or previous.get("continuous_status") != "central_waypoint_and_player_settled"
        or previous_wing not in order[:-1]
        or previous.get("return_from") != previous_wing
        or wing != order[order.index(previous_wing) + 1]
        or return_from is not None
        or last.get("action") != "central_arrival_hold"
        or type(at) not in (int, float)
        or not math.isfinite(at)
        or not 0 <= now - at < 0.5
    ):
        raise ValueError("immediate observed hub-to-next-wing transition required")


def run_continuous_route(
    capture,
    hud,
    backend,
    focused,
    profile,
    stop,
    planner,
    proof,
    seconds,
    *,
    evidence,
    hazards,
    hover_vision,
    reference_seconds,
    hub_vision=None,
    wing="north",
    return_from=None,
    target_vision=None,
    engage_monsters=True,
    progress=None,
    buff_expires_at=None,
    emit=print,
    clock=time.monotonic,
    continue_current_field=False,
    transition_from=None,
    route_memory=None,
    terrain_patterns=None,
    terminal_vision=None,
    objective_vision=None,
    recover_resources=None,
    loot_controller=None,
    automap_vision=None,
    panels_open=None,
):
    """현재 화면으로 넓게 조향하며 F2를 유지한다. 전투 모듈은 선택적으로 연결한다."""
    if type(seconds) not in (int, float) or not 1 <= seconds <= 60:
        raise ValueError("continuous supervised interval must be within one to sixty seconds")
    if type(reference_seconds) not in (int, float) or not 0 < reference_seconds < 1.5:
        raise ValueError("configured cast reference required for continuous steering")
    if wing not in HEADINGS or (return_from is not None and return_from not in HEADINGS):
        raise ValueError("confirmed Arcane wing required")
    if type(engage_monsters) is not bool:
        raise ValueError("explicit combat toggle required")
    if automap_vision is not None and not callable(panels_open):
        raise ValueError("current panel observer required for temporary map inspection")
    route_wing = OPPOSITE_WINGS[return_from] if return_from is not None else wing
    if transition_from is not None:
        if continue_current_field:
            raise ValueError("terminal transition and interval continuation are distinct")
        if transition_from.get("status") == "hub_arrived":
            validate_hub_departure(transition_from, wing, return_from, clock())
        else:
            validate_terminal_return(transition_from, wing, return_from, clock())
    elif continue_current_field:
        validate_live_continuation(progress or {}, wing, return_from, clock())
    else:
        validate_travel_permit(proof, profile.survival, time.time())
    started = clock()
    deadline = min(started + seconds, buff_expires_at) if buff_expires_at is not None else started + seconds
    progress = dict(steps=[], position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)]) if progress is None else progress
    rows = progress.setdefault("continuous_frames", [])
    held = HeldFieldInput(
        backend,
        profile.actions.movement_skill_key,
        profile.actions.primary_attack_skill_key,
        clock=clock,
        repeat_seconds=reference_seconds,
        aim_settle_seconds=0.05,
    )
    nearby_probe = (
        NearbyHostileProbe(
            target_vision,
            backend,
            (capture.target["left"], capture.target["top"]),
            clock=clock,
            to_desktop=lambda point: frame_to_desktop(capture, point),
        )
        if engage_monsters and target_vision is not None
        else None
    )
    watchdog_stop, watchdog_errors = threading.Event(), []

    def watchdog():
        while not watchdog_stop.wait(0.02):
            try:
                if stop.is_set() or not focused() or clock() >= deadline:
                    held.release()
                else:
                    held.service(clock())
            except Exception as exc:
                watchdog_errors.append(str(exc))
                stop.set()
                return

    motion = BackgroundMotionObserver(
        minimum_matches=12,
        minimum_inlier_ratio=0.65,
        maximum_error=3,
        landmark_viewport=(120, 180, 1720, 890),
        actor_exclusion=(260, 230, 260, 120),
    )
    path = planner[1]
    if path.teleport is None:
        raise ValueError("explicit reviewed teleport geometry required for continuous steering")
    path.teleport = dict(path.teleport, preferred_distance=path.teleport["maximum_distance"])
    worker = threading.Thread(target=watchdog, name="summoner-held-input-watchdog", daemon=True)
    worker.start()
    last_aim, first_frame, last_frame = progress.get("last_aim"), None, None
    last_changed_at, comparison = started, None
    last_changed_casts, stationary_casts = 0, 0
    attack_started, quiet_frames, combat_dispatch_baseline = None, 0, 0
    position_current = progress.get("position_current", True)
    status, reason, travel_frames = "north_segment_observed", "interval_complete", 0
    unavailable_since, aim_unavailable_since, stationary_since = None, None, None
    steering = ContinuousSteering(reference_seconds, path.teleport["clearance_pixels"], aim=last_aim)
    flow = ArcaneRouteFlow(progress.get("route_detour"))
    accepted_detour = flow.detour
    recent_turns = deque(maxlen=8)
    blocked_aims, recoveries = deque(maxlen=8), 0
    hub_arrival, terminal_arrival = ArrivalDwell(reference_seconds), ArrivalDwell(reference_seconds)
    hub_approach_settle = ArrivalDwell(reference_seconds)
    terminal_tracker = TerminalStabilityTracker(terminal_vision)
    hub_tracker = HubStabilityTracker(hub_vision)
    hub_missing_since, terminal_missing_since = None, None
    departure_pointer_cleared = False
    approach_pointer = {}
    terminal_approach_cast_at = None
    hub_near_wait = False
    hub_floor_pointer_cleared = False
    hub_approach_cast_at = progress.get("hub_approach_cast_at")
    map_hidden = False
    map_inspection_kind = None
    motion_missing_since = None
    motion_evidence, matched_frame = 0, None
    offset = len(rows)

    def restore_map():
        nonlocal map_hidden
        inspection = progress[map_inspection_kind + "_map_inspection"]
        if stop.is_set() or not focused():
            raise RuntimeError("map restoration interrupted or unfocused")
        backend.press("tab")
        # 복원 요청 뒤 제목 확인이 실패해도 다시 토글해 OFF로 되돌리지 않는다.
        map_hidden = False
        inspection["restoration"] = ensure_automap_on(
            capture,
            backend,
            focused,
            stop,
            hud,
            profile.survival,
            automap_vision,
            panels_open,
            clock=clock,
            allow_toggle=False,
            timeout=0.5,
        )
        inspection.update(status="on_observed", restored_at=clock())

    if return_from is not None and transition_from is None and not continue_current_field and proof.get("at_central_waypoint") is True:
        # 최신 감독의 중앙 재개 위치는 이동 보류만 건다. 자동 도착 근거를 대신하지 않는다.
        progress["hub_acquired"] = True
    try:
        with ThreadPoolExecutor(max_workers=3) as pool:
            for sequence in range(1, 1801):
                if stop.is_set() or not focused():
                    raise RuntimeError("continuous navigation interrupted or unfocused")
                if loot_controller is not None and not loot_controller.movement_allowed:
                    held.release()
                    status, reason = "field_paused", "item_labels_not_off"
                    break
                if clock() >= deadline:
                    if buff_expires_at is not None and clock() >= buff_expires_at:
                        status, reason = "field_paused", "buff_renewal_due"
                    break
                acquired = clock()
                input_before = held.snapshot()
                frame = capture.grab().frame
                last_frame = frame
                if first_frame is None:
                    first_frame = frame
                hud_reading = hud.observe(frame)
                prepare_ground = getattr(planner[0], "prepare_frame", None)
                if prepare_ground is not None:
                    prepare_ground(frame, acquired, hud_reading, focused())
                blocked = resource_guard(hud_reading, profile.survival)
                if blocked:
                    held.release()
                    approach_pointer.clear()
                    stationary_since = None
                    hub_arrival.reset()
                    terminal_arrival.reset()
                    terminal_tracker.reset()
                    hub_tracker.reset()
                    if blocked == "mercenary_unverified":
                        evidence.append(("mercenary-unverified.png", frame))
                        raise RuntimeError(blocked)
                    if blocked == "resource_low":
                        if recover_resources is not None and recover_resources(frame, hud_reading, acquired):
                            if nearby_probe is not None:
                                nearby_probe.reset()
                            attack_started, quiet_frames, comparison = None, 0, None
                            continue
                        raise RuntimeError(blocked)
                    unavailable_since = acquired if unavailable_since is None else unavailable_since
                    if clock() - unavailable_since >= 1.5:
                        status, reason = "field_paused", blocked
                        break
                    continue
                if map_hidden:
                    inspection = progress[map_inspection_kind + "_map_inspection"]
                    if "first_hidden_at" not in inspection:
                        inspection["first_hidden_at"] = acquired
                        evidence.append((map_inspection_kind + "-map-first-hidden.png", frame))
                    if clock() - inspection["requested_at"] >= 1.5:
                        held.release()
                        try:
                            restore_map()
                        except RuntimeError as exc:
                            inspection.update(status="restore_unverified", error=str(exc))
                            status, reason = "field_paused", map_inspection_kind + "_map_restore_unverified"
                            break
                        # 숨김 시한은 끝낸다. 복원 뒤 새 전체 근거로 기존 근접/정지를 다시 확인한다.
                        inspection["outcome"] = "timeout_then_normal_reacquisition"
                        motion.reset()
                        if map_inspection_kind == "terminal":
                            terminal_tracker.require_relocalization()
                            terminal_arrival.reset()
                            terminal_missing_since = None
                        else:
                            hub_tracker.reset()
                            hub_arrival.reset()
                            hub_missing_since, hub_near_wait = None, False
                        hub_approach_settle.reset()
                        approach_pointer.clear()
                        steering.aim, last_aim = None, None
                        emit(
                            map_inspection_kind.upper()
                            + "_MAP_INSPECTION: ON restored; require fresh target in normal bounded reacquisition"
                        )
                        continue
                ground = planner[0].observe(frame, captured_at=acquired)
                if ground.point is not None:
                    # 지형·출발점 판독보다 현재 발과 근접 위협의 방어를 먼저 처리한다.
                    observed_pointer = held.snapshot()["pointer"]
                    observed_aim = desktop_to_frame(capture, observed_pointer) if observed_pointer is not None else last_aim
                    dangerous = (
                        pool.submit(hazards.observe, frame, ground.point, acquired, clock(), aim=observed_aim)
                        if engage_monsters
                        else pool.submit(tuple)
                    )
                    hovering = pool.submit(hover_vision.observe, frame, observed_aim) if observed_aim else pool.submit(tuple)
                    target = None if not engage_monsters or target_vision is None else target_vision.observe(frame)
                    candidates, hover = dangerous.result(), hovering.result()
                    probe_confirmed, probe_pending, proposals = False, False, ()
                    if nearby_probe is not None and not any(near_ground(candidate.ground, ground.point) for candidate in candidates):
                        proposals = (
                            nearby_actor_proposals(frame, ground.point)
                            if nearby_probe.pending is not None or attack_started is None
                            else ()
                        )
                        if (
                            (nearby_probe.pending is not None or (proposals and clock() >= nearby_probe.next_proposal_at))
                            and not stop.is_set()
                            and focused()
                            and clock() < deadline
                        ):
                            # 색 효과만으로 공격하지 않는다. F2를 먼저 놓고 새 호버 화면의 적 패널을 요구한다.
                            if held.keys[0] in held.held or held.snapshot()["pending"] == held.keys[0]:
                                held.release()
                            probed = nearby_probe.observe(frame, ground.point, acquired, points=proposals)
                            probe_confirmed = probed is not None
                            probe_pending = nearby_probe.pending is not None and not probe_confirmed
                            if probe_confirmed:
                                target = probed
                    close = any(near_ground(candidate.ground, ground.point) for candidate in candidates)
                    # 호버로 옮긴 커서의 패널을 과거 이동 조준 위치에 있는 적으로 해석하지 않는다.
                    direct_hover_near = (
                        target is not None
                        and observed_pointer is not None
                        and near_ground(observed_aim, ground.point)
                        and backend.pointer_matches(observed_pointer)
                    )
                    close = close or probe_confirmed or direct_hover_near
                    combat_dispatched_at = None
                    # 현재 근접 위협은 끝의 전역 탐색이나 배경 대응을 기다리지 않는다.
                    if close and not stop.is_set() and focused():
                        ready_at = clock()
                        if 0 <= ready_at - acquired < 0.25 and ready_at < deadline:
                            if attack_started is None:
                                attack_started = ready_at
                                approach_pointer.clear()
                                terminal_missing_since, hub_missing_since = None, None
                                combat_dispatch_baseline = held.snapshot()["dispatches"][held.keys[1]]
                                emit("NORTH_COMBAT: current nearby hostile -> release F2 -> F4 before route analysis")
                                evidence.append(("continuous-{:04d}-combat.png".format(offset + sequence), frame))
                            held.apply(FieldDirective("combat", "current_near_hostile", deadline=min(acquired + 0.25, deadline)), ready_at)
                            combat_dispatched_at = clock()
                            timing = dict(acquired_at=acquired, requested_at=combat_dispatched_at, player=ground.point)
                            progress.setdefault("first_nearby_defense", timing)
                            progress["last_nearby_defense"] = timing
                else:
                    close = False
                    candidates, hover, proposals = (), (), ()
                    target, observed_pointer, observed_aim = None, None, None
                    probe_confirmed, probe_pending, direct_hover_near = False, False, False
                    combat_dispatched_at = None
                # 같은 화면의 독립 판독을 겹쳐 이전 F2 임대 안에 새 안전 검사를 마친다.
                planning_work = pool.submit(
                    plan_arcane_step,
                    frame,
                    route_wing,
                    *planner,
                    acquired,
                    position=tuple(progress["position"]),
                    visited=(),
                    clock=clock,
                    allow_detour=True,
                    prefer_frontier=route_memory is None,
                    allow_no_aim=True,
                    excluded=tuple(blocked_aims),
                    ground_reading=ground,
                )
                terminal_work = None if terminal_vision is None or return_from is not None else pool.submit(terminal_vision.observe, frame)
                objective_work = (
                    None if objective_vision is None or return_from is not None else pool.submit(objective_vision.observe, frame)
                )
                hub_point = None if hub_vision is None else hub_vision.observe(frame)
                hub_evidence = None if hub_point is None else getattr(hub_vision, "last_evidence", None)
                if return_from is not None and hub_point is not None:
                    # 중앙 발견이 일반 조향/반복 입력보다 먼저 이동을 선점한다.
                    if not progress.get("hub_acquired"):
                        evidence.append(("hub-first-acquired.png", frame))
                    progress["hub_acquired"] = True
                    if held.keys[0] in held.held or held.snapshot()["pending"] == held.keys[0]:
                        held.release()
                if return_from is None and progress.get("terminal_acquired"):
                    if held.keys[0] in held.held or held.snapshot()["pending"] == held.keys[0]:
                        held.release()
                objective = None if objective_work is None else objective_work.result()
                if objective is not None and 0 <= clock() - acquired < 0.25:
                    held.release()
                    row = dict(
                        index=offset + sequence - 1,
                        acquired_at=acquired,
                        processed_at=clock(),
                        action="summoner_platform_hold",
                        objective=asdict(objective),
                    )
                    rows.append(row)
                    progress["objective"] = asdict(objective)
                    evidence.append(("summoner-platform.png", frame))
                    status, reason = "summoner_platform_observed", objective.evidence_kind
                    break
                try:
                    choice, ground, view = planning_work.result()
                except RuntimeError as exc:
                    terminal = None if terminal_work is None else terminal_work.result()
                    if terminal is not None and 0 <= clock() - acquired < 0.25:
                        if not progress.get("terminal_acquired"):
                            evidence.append(("terminal-first-acquired.png", frame))
                        progress["terminal_acquired"] = True
                        held.release()
                    # 시전 중 몸통 가림은 이미 승인된 입력의 남은 임대만 소비한다.
                    # 이 화면으로 임대를 연장하거나 새 조준/반복 입력을 승인하지 않는다.
                    held.expire(clock())
                    failures = progress.setdefault("ground_failures", [])
                    resets = getattr(planner[0], "reset_events", ())
                    failures.append(
                        dict(
                            acquired_at=acquired,
                            reason=str(exc),
                            life_ratio=hud_reading.life.ratio,
                            mana_ratio=hud_reading.mana.ratio,
                            belt_visibility=hud_reading.belt_visibility,
                            camera_center=getattr(planner[0], "center", None),
                            last_camera_reset=resets[-1] if resets else None,
                        )
                    )
                    if "first_ground_failure" not in progress:
                        # 뒤의 제한 로그에 최초 모순이 밀려나도 원인 화면과 판독을 함께 남긴다.
                        progress["first_ground_failure"] = dict(failures[-1], evidence_file="continuous-ground-failure-first.png")
                        evidence.append(("continuous-ground-failure-first.png", frame.copy()))
                    if str(exc) == "camera_actor_displaced" and "first_camera_displacement" not in progress:
                        # 앞선 일시 가림과 뒤의 카메라 폐기 원인을 별도 한 장으로 구분한다.
                        progress["first_camera_displacement"] = dict(failures[-1], evidence_file="continuous-camera-displacement-first.png")
                        evidence.append(("continuous-camera-displacement-first.png", frame.copy()))
                    del failures[:-16]
                    stationary_since = None
                    hub_arrival.reset()
                    terminal_arrival.reset()
                    terminal_tracker.reset()
                    hub_tracker.reset()
                    unavailable_since = acquired if unavailable_since is None else unavailable_since
                    if clock() - unavailable_since >= 1.5:
                        held.release()
                        status, reason = "field_paused", str(exc)
                        break
                    continue
                resumed_wing = proof.get("current_wing") == wing and proof.get("wing_phase") == "outbound"
                if (
                    return_from is None
                    and hub_vision is not None
                    and not resumed_wing
                    and not progress.get("central_departure_observed")
                    and attack_started is None
                ):
                    if hub_point is None or math.hypot(*(a - b for a, b in zip(ground.point, hub_point))) > 100:
                        held.release()
                        if terminal_work is not None:
                            terminal_work.result()
                        since = progress.setdefault("central_departure_wait_started_at", acquired)
                        row = dict(
                            index=offset + sequence - 1,
                            acquired_at=acquired,
                            processed_at=clock(),
                            action="outbound_central_reacquire_hold",
                            player=asdict(ground),
                            hub_point=hub_point,
                        )
                        rows.append(row)
                        # 버프 효과·커서 가림 한 화면으로 출발을 실패시키지 않는다. 현재 중앙 확인 전 시전은 금지한다.
                        if not departure_pointer_cleared and not stop.is_set() and focused() and 0 <= clock() - acquired < 0.25:
                            backend.move(*frame_to_desktop(capture, POINTER_CLEAR_POINT))
                            departure_pointer_cleared = True
                            row["pointer_cleared"] = True
                        if clock() - since >= 1.5:
                            raise RuntimeError("outbound_requires_central_waypoint")
                        continue
                    progress["central_departure_observed"] = True
                    progress.pop("central_departure_wait_started_at", None)
                # HUD와 발 위치가 다시 판독되면 이전 가림의 시간을 끊는다.
                # 전투/중앙 접근은 아래의 일반 이동 분기를 거치지 않을 수 있다.
                unavailable_since = None
                view = replace(view, sequence=sequence)
                if observed_aim is None and choice.aim is not None:
                    # 첫 이동 후보의 착지 주변/호버도 새 지형 계획 뒤 별도로 검사한다.
                    observed_aim = choice.aim
                    hover = hover_vision.observe(frame, observed_aim)
                    if engage_monsters:
                        candidates = hazards.observe(frame, ground.point, acquired, clock(), aim=observed_aim)
                terminal = None if terminal_work is None else terminal_work.result()
                valid_terminal = terminal is not None and terminal.kind in ("nonboss_chest_trio", "nonboss_chest_platform")
                if valid_terminal and 0 <= clock() - acquired < 0.25:
                    if not progress.get("terminal_acquired"):
                        evidence.append(("terminal-first-acquired.png", frame))
                    progress["terminal_acquired"] = True
                    # 현재 근접 방어는 유지하고 이동만 해제한다. 도착은 아래에서 별도 확인한다.
                    if held.keys[0] in held.held or held.snapshot()["pending"] == held.keys[0]:
                        held.release()
                preflight_at, renewed_at = clock(), None
                active = held.snapshot()
                pointer = active["pointer"]
                if pointer is not None and active["held"] == (held.keys[0],):
                    current_aim = desktop_to_frame(capture, pointer)
                    close = any(near_ground(candidate.ground, ground.point) for candidate in candidates)
                    close = close or direct_hover_near
                    if close or hover:
                        held.release()
                    elif (
                        attack_started is None
                        and current_aim == observed_aim
                        and not progress.get("hub_acquired")
                        and not progress.get("terminal_acquired")
                        and (return_from is None or hub_point is None)
                        and not valid_terminal
                        and not any(
                            left <= current_aim[0] < right and top <= current_aim[1] < bottom for left, top, right, bottom in blocked_aims
                        )
                        and floor_supports_aim(view, current_aim, path.teleport["clearance_pixels"])
                        and not stop.is_set()
                        and focused()
                        and 0 <= clock() - acquired < 0.25
                        and clock() < deadline
                        and held.renew_lease(held.keys[0], pointer, min(acquired + 0.25, deadline), clock())
                    ):
                        renewed_at = clock()
                pointer = held.cast_pointer
                hint = (
                    None
                    if pointer is None
                    else (
                        desktop_to_frame(capture, pointer)[0] - ground.point[0],
                        desktop_to_frame(capture, pointer)[1] - ground.point[1],
                    )
                )
                moving = pool.submit(motion.observe, frame, view, overlap_hint=hint)
                pattern_work = None if terrain_patterns is None else pool.submit(terrain_patterns.observe, frame)
                measured = moving.result()
                if measured.displacement is not None and math.hypot(*measured.displacement) < 2:
                    if stationary_since is None:
                        stationary_casts = input_before["dispatches"][held.keys[0]]
                    stationary_since = acquired if stationary_since is None else stationary_since
                else:
                    stationary_since = None
                landings = () if pattern_work is None else pattern_work.result()
                terminal = terminal_tracker.update(frame, terminal, acquired, measured.displacement)
                hub_point, hub_evidence = hub_tracker.update(frame, hub_point, hub_evidence, acquired, measured.displacement)
                valid_terminal = terminal is not None and terminal.kind in ("nonboss_chest_trio", "nonboss_chest_platform")
                if route_memory is not None:
                    relocated = route_memory.observe(motion.features, ground.point, measured.displacement, acquired)
                    position_current = route_memory.localized
                    progress["position"] = list(route_memory.position)
                    progress["breadcrumbs"] = list(route_memory.places)
                    if relocated:
                        motion.reset()
                        motion.observe(frame, view)
                    if measured.displacement is not None and math.hypot(*measured.displacement) >= 12:
                        blocked_aims.clear()
                        recoveries = 0
                    if measured.displacement is not None:
                        matched_frame = frame
                elif measured.displacement is None:
                    if held.held and matched_frame is not None and motion_evidence < 3 and motion_missing_since is None:
                        prefix = "continuous-{:04d}-motion".format(offset + sequence)
                        evidence.extend(((prefix + "-before.png", matched_frame), (prefix + "-after.png", frame)))
                        motion_evidence += 1
                    # 텔레포트 중간 프레임은 마지막 검증 지형과 다시 대응할 짧은 여유를 둔다.
                    if input_before["dispatches"][held.keys[0]] or offset or matched_frame is not None:
                        position_current = False
                        motion_missing_since = acquired if motion_missing_since is None else motion_missing_since
                        if acquired - motion_missing_since >= reference_seconds * 2:
                            # 끝내 대응하지 못한 구간의 이동량은 만들지 않고 새 국소 원점을 시작한다.
                            progress["position_generation"] = progress.get("position_generation", 0) + 1
                            progress.update(position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)], position_reference="local_scene")
                            position_current, motion_missing_since = True, None
                            motion.reset()
                            motion.observe(frame, view)
                            matched_frame = frame
                else:
                    position_current, motion_missing_since = True, None
                    matched_frame = frame
                    progress["position"] = [value + delta for value, delta in zip(progress["position"], measured.displacement)]
                    if math.hypot(*measured.displacement) >= 12:
                        progress["breadcrumbs"].append(tuple(progress["position"]))
                        blocked_aims.clear()
                        recoveries = 0
                # 현재 화면의 실측 위치를 반영한 뒤 방문 흔적을 적용한다.
                if route_memory is None and position_current and len(progress["breadcrumbs"]) > 1:
                    choice = path.choose(
                        view,
                        route_wing,
                        position=tuple(progress["position"]),
                        visited=tuple(progress["breadcrumbs"]),
                        allow_detour=True,
                        prefer_frontier=True,
                        excluded=tuple(blocked_aims),
                    )
                global_choice = choice
                now = clock()
                row = dict(
                    index=offset + sequence - 1,
                    acquired_at=acquired,
                    processed_at=now,
                    preflight_processed_at=preflight_at,
                    preflight_input=active,
                    lease_renewed_at=renewed_at,
                    ground=asdict(ground),
                    choice=asdict(choice),
                    global_choice=asdict(global_choice),
                    flow_reason=flow.reason,
                    route_detour=flow.detour,
                    motion=asdict(measured),
                    terminal_approach_displacement=terminal_tracker.approach_displacement,
                    terminal_approach_static_pairs=terminal_tracker.approach_static_pairs,
                    candidates=[asdict(c) for c in candidates],
                    hover=[asdict(c) for c in hover],
                    target=None if target is None else asdict(target),
                    nearby_hover_confirmed=probe_confirmed,
                    direct_hover_near=direct_hover_near,
                    nearby_hover_pending=probe_pending,
                    nearby_actor_proposals=proposals,
                    combat_preflight_at=combat_dispatched_at,
                    hub_point=hub_point,
                    hub_evidence=hub_evidence,
                    route_heading=HEADINGS[route_wing],
                    engage_monsters=engage_monsters,
                    terminal=None if terminal is None else asdict(terminal),
                    terminal_approach=None if terminal_tracker.approach is None else asdict(terminal_tracker.approach),
                    position=tuple(progress["position"]),
                    position_generation=progress.get("position_generation", 0),
                    position_current=position_current,
                    input_before=input_before,
                )
                rows.append(row)
                if route_memory is not None:
                    row["route_graph"] = route_memory.summary()
                    row["terrain_landings"] = [asdict(item) for item in landings]
                if not 0 <= now - acquired < 0.25 or now >= deadline:
                    held.release()
                    hub_arrival.reset()
                    terminal_arrival.reset()
                    row["action"] = "stale_hold"
                    continue
                terminal_distance = None if not valid_terminal else math.hypot(*(a - b for a, b in zip(terminal.point, ground.point)))
                row["terminal_distance"] = terminal_distance
                if return_from is None and valid_terminal and terminal_distance <= 400 and (attack_started is None or not close):
                    # 끝 도착 확인은 새 호버 탐색으로 미루지 않는다. 복귀 단계는 적을 다시 현재 관찰한다.
                    held.release()
                    row["action"] = "terminal_arrival_hold"
                    arrived = terminal_arrival.update(True, measured.displacement, now)
                    row["arrival_hold_seconds"] = now - terminal_arrival.since
                    if arrived:
                        status = "wing_end_observed"
                        reason = (
                            "recent_terminal_layout_and_player_settled"
                            if terminal.provenance
                            in ("current_static_two_parts", "current_relocalized_two_parts", "current_motion_settled_two_parts")
                            else (
                                "current_chest_trio_and_player_settled"
                                if terminal.kind == "nonboss_chest_trio"
                                else "current_terminal_landmarks_and_player_settled"
                            )
                        )
                        progress["terminal"] = asdict(terminal)
                        evidence.append(("wing-end.png", frame))
                        break
                    continue
                close = any(near_ground(candidate.ground, ground.point) for candidate in candidates)
                close = close or probe_confirmed or direct_hover_near
                if probe_pending and attack_started is None and not close:
                    held.release()
                    row["action"] = "nearby_hover_probe"
                    # 새 적 패널을 볼 수 있는 기존 50ms 정착 뒤 다음 화면을 촬영한다.
                    ready_at = nearby_probe.pending[1] + 0.05
                    wait_started = clock()
                    remaining = min(ready_at, deadline) - wait_started
                    if remaining > 0:
                        stop.wait(remaining)
                    row.update(hover_capture_ready_at=ready_at, hover_capture_wait_seconds=clock() - wait_started)
                    continue
                if close:
                    if attack_started is None:
                        attack_started = now
                        approach_pointer.clear()
                        # 재획득 대기의 시계가 새 전투/ALT 검사 시간을 소비하지 않게 한다.
                        terminal_missing_since, hub_missing_since = None, None
                        combat_dispatch_baseline = held.snapshot()["dispatches"][held.keys[1]]
                        emit("NORTH_COMBAT: release F2 -> hold F4 Nova for current nearby hostile evidence")
                        evidence.append(("continuous-{:04d}-combat.png".format(offset + sequence), frame))
                    quiet_frames = 0
                elif attack_started is not None:
                    quiet_frames += 1
                if attack_started is not None:
                    stationary_since = None
                    if now - attack_started >= 6:
                        held.release()
                        status, reason = "field_paused", "nova_target_requires_review"
                        break
                    if quiet_frames >= 3 and now - attack_started >= 1:
                        held.release()
                        emit("NORTH_COMBAT: Nova block finished; inspect approved loot (kill unconfirmed)")
                        attack_started, comparison, last_changed_at = None, None, now
                        dispatches = held.snapshot()["dispatches"][held.keys[1]]
                        if loot_controller is not None and dispatches > combat_dispatch_baseline:
                            row["loot_inspection"] = loot_controller.after_combat(
                                buff_expires_at=buff_expires_at,
                                reason="combat_ended_scan",
                                combat_id=("navigation", started, dispatches),
                            )
                            row["action"] = "post_combat_loot_inspection"
                            steering.aim, last_aim = None, None
                            hub_arrival.reset()
                            terminal_arrival.reset()
                            inspection = row["loot_inspection"]
                            if (
                                terminal_tracker.reference is not None
                                and inspection.get("status") == "inspection_complete"
                                and inspection.get("labels_state") == "off"
                                and inspection.get("movement_allowed") is True
                                and inspection.get("clicks") == []
                                and inspection.get("teleports") == []
                            ):
                                # ALT만 토글한 검사는 원래2초를 보존한다. 다음 현재 두 부위로 다시 위치를 확인한다.
                                row["terminal_reference_retained_at"] = terminal_tracker.observed_at
                                terminal_tracker.require_relocalization()
                            else:
                                terminal_tracker.reset()
                            hub_tracker.reset()
                            if nearby_probe is not None:
                                nearby_probe.reset()
                            # 토글/클릭 전 지형 화면으로 이동을 재개하지 않는다.
                            continue
                    else:
                        held.apply(FieldDirective("combat", "near_hostile_training", deadline=min(acquired + 0.25, deadline)), clock())
                        row["action"] = "hold_nova"
                        hub_approach_settle.reset()
                        continue
                if return_from is not None and progress.get("hub_acquired"):
                    stationary_since = None
                    held.release()
                    if hub_approach_cast_at is not None:
                        moved = measured.displacement is not None and math.hypot(*measured.displacement) >= 2
                        landmark_landing = (
                            not moved
                            and hub_point is not None
                            and current_hub_landing(
                                progress, hub_point, ground.point, hub_evidence, measured.displacement, acquired, offset + sequence - 1
                            )
                        )
                        if moved or landmark_landing:
                            # 이동 확인은 현재 WP의 가림/근접 분기보다 먼저 소비하며 도착은 별도로 확인한다.
                            row["hub_landing_confirmed"] = (
                                "current_background_displacement" if moved else "current_waypoint_relative_displacement"
                            )
                            hub_approach_cast_at = None
                            progress.pop("hub_approach_cast_at", None)
                            progress.pop("hub_approach_origin", None)
                            progress.pop("hub_approach_landing", None)
                    if hub_point is None:
                        hub_arrival.reset()
                        hub_approach_settle.reset()
                        hub_missing_since = now if hub_missing_since is None else hub_missing_since
                        row["action"] = "central_reacquire_hold"
                        if (
                            (last_aim is not None or approach_pointer)
                            and not progress.get("hub_hover_cleared")
                            and not stop.is_set()
                            and focused()
                        ):
                            # 직전 조준의 이름표가 문양을 가릴 수 있다. 클릭/시전 없이 치우고 새 화면을 요구한다.
                            backend.move(*frame_to_desktop(capture, POINTER_CLEAR_POINT))
                            approach_pointer.clear()
                            progress["hub_hover_cleared"] = True
                            row["action"] = "central_reacquire_pointer_clear"
                            continue
                        pair = hub_vision.current_pair_near(frame, ground.point)
                        stationary = measured.displacement is not None and math.hypot(*measured.displacement) < 2
                        if (
                            automap_vision is not None
                            and not progress.get("central_map_inspection")
                            and attack_started is None
                            and not close
                            and stationary
                            and automap_vision.visible(frame, "arcane_sanctuary")
                            and not panels_open(frame)
                            and not stop.is_set()
                            and focused()
                            and 0 <= clock() - acquired < 0.25
                            and clock() < deadline
                        ):
                            # 현재 지도 선이 중앙 문양/불꽃을 가리면 한 번만 숨긴다. 이전 위치는 도착에 쓰지 않는다.
                            backend.press("tab")
                            map_hidden, map_inspection_kind = True, "central"
                            progress["central_map_inspection"] = dict(status="hide_requested", requested_at=clock())
                            evidence.append(("central-map-before.png", frame))
                            hub_tracker.reset()
                            motion.reset()
                            approach_pointer.clear()
                            steering.aim, last_aim = None, None
                            hub_missing_since, hub_near_wait = None, False
                            row["action"] = "central_map_hide"
                            continue
                        pair_wait = isinstance(pair, tuple) and stationary
                        hub_near_wait = stationary and (hub_near_wait or pair_wait)
                        row["current_near_blue_pair"] = pair if pair_wait else None
                        row["central_reacquisition_limit"] = 6 if hub_near_wait else 1.5
                        # 마지막 접근 뒤 불꽃 쌍·정지가 현재 보이면 문양이 다시 드러날 때까지 입력 없이 기다린다.
                        # 대기만 연장하며 이전 좌표를 이동시키거나 도착 근거로 승격하지 않는다.
                        # 불꽃 애니메이션 한 프레임 누락으로 이미 허용한 정지 대기를 줄이지 않는다.
                        # 현재 배경 정지가 끊기면 즉시 짧은 대기로 돌아가며 도착은 새 전체 근거를 요구한다.
                        if now - hub_missing_since >= row["central_reacquisition_limit"]:
                            status, reason = "field_paused", "central_reacquisition_required"
                            break
                        continue
                    hub_missing_since = None
                    hub_near_wait = False
                    progress["hub_hover_cleared"] = False
                    distance = math.hypot(*(a - b for a, b in zip(hub_point, ground.point)))
                    if distance <= 100:
                        hub_approach_settle.reset()
                        row["action"] = "central_arrival_hold"
                        arrived = hub_arrival.update(True, measured.displacement, now)
                        row["arrival_hold_seconds"] = now - hub_arrival.since
                        if arrived:
                            status, reason = "hub_arrived", "central_waypoint_and_player_settled"
                            evidence.append(("hub-arrived.png", frame))
                            break
                        continue
                    hub_arrival.reset()
                    if hub_approach_cast_at is not None:
                        row["action"] = "central_approach_landing_hold"
                        if now - hub_approach_cast_at >= 1.5:
                            status, reason = "field_paused", "central_approach_landing_unconfirmed"
                            break
                        continue
                    row["action"] = "central_approach_settle"
                    # 이전 넓은 시전이 끝난 뒤 현재 중앙 근처의 바닥으로 한 번만 접근한다.
                    if not hub_approach_settle.update(True, measured.displacement, now):
                        continue
                    choice = path.choose(view, route_wing, destination=tuple(a - b for a, b in zip(hub_point, ground.point)))
                    row["choice"] = asdict(choice)
                    remaining = None if choice.aim is None else math.hypot(*(a - b for a, b in zip(choice.aim, hub_point)))
                    # 접근 후보의 격자 중심 오차만 허용한다. 실제 도착의 100px 기준은 위에서 그대로 확인한다.
                    approach_limit = max(100, distance - 200) + view.cell_pixels / math.sqrt(2)
                    if remaining is None or remaining >= distance or remaining > approach_limit:
                        if not hub_floor_pointer_cleared and not stop.is_set() and focused() and 0 <= clock() - acquired < 0.25:
                            backend.move(*frame_to_desktop(capture, POINTER_CLEAR_POINT))
                            hub_floor_pointer_cleared = True
                            row["action"] = "central_floor_pointer_clear"
                            continue
                        status, reason = "field_paused", "central_approach_floor_unverified"
                        break
                    aim = frame_to_desktop(capture, choice.aim)
                    dispatch = {}
                    # 다음 프레임의 WP 이름표로 조준을 바꾸지 않는다. 원래 250ms 안에서 실제 커서·50ms를 확인한다.
                    sent = dispatch_planned_step(
                        backend,
                        focused,
                        stop,
                        aim,
                        profile.actions.movement_skill_key,
                        acquired,
                        clock=clock,
                        verify_aim=lambda: backend.pointer_matches(aim),
                        expires_at=min(deadline, progress["central_map_inspection"]["requested_at"] + 1.5) if map_hidden else deadline,
                        diagnostics=dispatch,
                    )
                    row["approach_dispatch"] = dispatch
                    row["action"] = "central_approach_cast" if sent else "central_approach_" + dispatch["reason"]
                    travel_frames += int(sent)
                    if sent:
                        hub_approach_cast_at = progress["hub_approach_cast_at"] = clock()
                        if isinstance(hub_evidence, dict) and hub_evidence.get("kind") == "blue_pair_and_floor_ornaments":
                            progress["hub_approach_origin"] = dict(
                                relative=tuple(a - b for a, b in zip(hub_point, ground.point)), at=acquired
                            )
                        else:
                            progress.pop("hub_approach_origin", None)
                        progress.pop("hub_approach_landing", None)
                        hub_floor_pointer_cleared = False
                        hub_approach_settle.reset()
                    continue
                hub_arrival.reset()
                terminal_target = terminal if valid_terminal else terminal_tracker.approach
                terminal_distance = (
                    None if terminal_target is None else math.hypot(*(a - b for a, b in zip(terminal_target.point, ground.point)))
                )
                row["terminal_target_distance"] = terminal_distance
                if return_from is None and progress.get("terminal_acquired"):
                    held.release()
                    stationary_since = None
                    approach_radius = 400 if valid_terminal else 200
                    if terminal_target is None or (not valid_terminal and terminal_distance <= approach_radius):
                        terminal_arrival.reset()
                        hub_approach_settle.reset()
                        terminal_missing_since = now if terminal_missing_since is None else terminal_missing_since
                        row["action"] = "terminal_reacquire_hold"
                        if (
                            automap_vision is not None
                            and not progress.get("terminal_map_inspection")
                            and attack_started is None
                            and not close
                            and measured.displacement is not None
                            and math.hypot(*measured.displacement) < 2
                            and automap_vision.visible(frame, "arcane_sanctuary")
                            and not panels_open(frame)
                            and not stop.is_set()
                            and focused()
                            and 0 <= clock() - acquired < 0.25
                            and clock() < deadline
                        ):
                            # 방금 확인한 끝을 지도가 가리면 한 번만 숨긴다. 다음 새 화면의 전체 근거로 다시 승인한다.
                            backend.press("tab")
                            map_hidden, map_inspection_kind = True, "terminal"
                            progress["terminal_map_inspection"] = dict(
                                status="hide_requested", requested_at=clock(), reference_observed_at=terminal_tracker.observed_at
                            )
                            evidence.append(("terminal-map-before.png", frame))
                            approach_pointer.clear()
                            # 지도 토글은 세계 배치를 바꾸지 않는다. 원래 2초 안의 현재 두 부위 대응만 이어받는다.
                            terminal_tracker.require_relocalization()
                            terminal_arrival.reset()
                            motion.reset()
                            steering.aim, last_aim = None, None
                            terminal_missing_since = None
                            row["action"] = "terminal_map_hide"
                            continue
                        # 불벽 등 일시 가림은 정지 중에만 더 기다린다. 과거 배치의 유효 시간은 늘리지 않는다.
                        reacquisition_limit = (
                            TERMINAL_STATIONARY_REACQUISITION_SECONDS
                            if measured.displacement is not None and math.hypot(*measured.displacement) < 2
                            else 1.5
                        )
                        row["terminal_reacquisition_limit"] = reacquisition_limit
                        if now - terminal_missing_since >= reacquisition_limit:
                            status, reason = "field_paused", "terminal_reacquisition_required"
                            break
                        continue
                    terminal_missing_since = None
                    if terminal_distance > approach_radius:
                        terminal_arrival.reset()
                        row["action"] = "terminal_approach_settle"
                        approach_displacement = measured.displacement
                        if approach_displacement is None:
                            approach_displacement = terminal_tracker.approach_displacement
                        approach_ready = hub_approach_settle.update(True, approach_displacement, now)
                        # 연속 세 현재 전체 배치가 정지했고 마지막 실제 시전의 참고 시간도 끝났으면
                        # 영상 처리 완료 시점부터 같은 시전 대기를 다시 시작하지 않는다. 도착 승인은 별도다.
                        last_cast = max(held.snapshot()["last_dispatch_at"] or started, terminal_approach_cast_at or started)
                        if (
                            not approach_ready
                            and measured.displacement is None
                            and terminal_tracker.approach_static_pairs >= 2
                            and now - last_cast >= reference_seconds
                        ):
                            approach_ready = True
                            row["terminal_approach_ready_source"] = "three_current_static_parts_after_cast_reference"
                        if not approach_ready:
                            continue
                        prepared = approach_pointer.get("aim")
                        prepared = None if prepared is None else desktop_to_frame(capture, prepared)
                        approach_limit = max(approach_radius, terminal_distance - 200)
                        prepared_floor = prepared is not None and floor_supports_aim(view, prepared, path.teleport["clearance_pixels"])
                        prepared_distance = (
                            None if prepared is None else math.hypot(*(a - b for a, b in zip(prepared, terminal_target.point)))
                        )
                        row.update(
                            terminal_prepared_aim=prepared,
                            terminal_prepared_floor=prepared_floor,
                            terminal_prepared_target_distance=prepared_distance,
                        )
                        # 새 바닥 후보의 작은 흔들림으로 커서 정착을 다시 시작하지 않는다. 현재 바닥/목표로 재승인한다.
                        if prepared_floor and prepared_distance <= approach_limit:
                            approach = replace(choice, aim=prepared, reason="current_terminal_prepared_floor")
                        else:
                            approach = path.choose(
                                view, route_wing, destination=tuple(a - b for a, b in zip(terminal_target.point, ground.point))
                            )
                        row["terminal_approach_choice"] = asdict(approach)
                        if approach.aim is None or math.hypot(*(a - b for a, b in zip(approach.aim, terminal_target.point))) > max(
                            approach_radius, terminal_distance - 200
                        ):
                            status, reason = "field_paused", "terminal_approach_floor_unverified"
                            break
                        # 짧은 지도 검사 중에는 원래 250ms에 50ms 정착이 들어갈 때만 같은 화면으로 시전한다.
                        # 느린 화면은 기존 다음 새 화면의 포인터 준비를 유지하며 입력 직전 시한/실제 커서를 재검사한다.
                        same_frame_pointer = map_hidden and 0 <= clock() - acquired < 0.20
                        if same_frame_pointer:
                            approach_pointer.clear()
                        aim = frame_to_desktop(capture, approach.aim)
                        approach_deadline = (
                            min(deadline, progress["terminal_map_inspection"]["requested_at"] + 1.5) if map_hidden else deadline
                        )
                        dispatch = dict(pointer_mode="same_frame" if same_frame_pointer else "next_frame", expires_at=approach_deadline)
                        sent = dispatch_planned_step(
                            backend,
                            focused,
                            stop,
                            aim,
                            profile.actions.movement_skill_key,
                            acquired,
                            clock=clock,
                            expires_at=approach_deadline,
                            diagnostics=dispatch,
                            verify_aim=lambda: backend.pointer_matches(aim),
                            pointer_preparation=None if same_frame_pointer else approach_pointer,
                        )
                        row["approach_dispatch"] = dispatch
                        row["action"] = "terminal_approach_cast" if sent else "terminal_approach_" + dispatch["reason"]
                        travel_frames += int(sent)
                        if sent:
                            terminal_approach_cast_at = clock()
                            hub_approach_settle.reset()
                        elif dispatch.get("reason") == "pointer_settling":
                            # 어차피 쓸 수 없는 이른 화면 대신 기존 50ms 정착 뒤 새 화면을 촬영한다.
                            ready_at = approach_pointer["requested_at"] + 0.05
                            wait_started = clock()
                            remaining = min(ready_at, approach_deadline) - wait_started
                            if remaining > 0:
                                stop.wait(remaining)
                            row.update(approach_capture_ready_at=ready_at, approach_capture_wait_seconds=clock() - wait_started)
                        continue
                terminal_arrival.reset()
                # 전투·검사·도착 대기 동안에는 이동하지 않을 출구를 선택해 의도를 바꾸지 않는다.
                if route_memory is None:
                    choice = flow.choose(
                        path,
                        view,
                        route_wing,
                        choice,
                        clock(),
                        position=tuple(progress["position"]),
                        visited=tuple(progress["breadcrumbs"]) if position_current else (),
                        excluded=tuple(blocked_aims),
                    )
                else:
                    choice = route_memory.choose(
                        path,
                        view,
                        route_wing,
                        clock(),
                        excluded=tuple(blocked_aims),
                        landings=landings,
                        hub=hub_point if return_from is None else None,
                        prefer_goal_exit=return_from is not None,
                    )
                    row["route_graph"] = route_memory.summary()
                    if choice.reason.startswith("current_hub_departure_") and last_aim is not None:
                        previous_direction = tuple(a - b for a, b in zip(last_aim, ground.point))
                        if ArcaneRouteFlow.axis(previous_direction) != HEADINGS[route_wing]:
                            if profile.actions.movement_skill_key in held.held:
                                held.release()
                            steering.aim = None
                row.update(choice=asdict(choice), flow_reason=flow.reason, route_detour=flow.detour, processed_at=clock())
                now = clock()
                if not 0 <= now - acquired < 0.25 or now >= deadline:
                    held.release()
                    row["action"] = "stale_hold"
                    continue
                if choice.aim is None:
                    held.release()
                    row["action"] = "path_hold"
                    if aim_unavailable_since is None or (len(rows) > 1 and rows[-2].get("action") != "path_hold"):
                        aim_unavailable_since = acquired
                    if now - aim_unavailable_since >= 1.5:
                        status, reason = "field_paused", choice.reason
                        break
                    continue
                # 임시 바닥 후보가 최종 graph의 출구 부재 시간을 지우면
                # 같은 장소의 path_hold가 무한히 이어진다. 최종 후보만 인정한다.
                aim_unavailable_since = None
                direction = tuple(a - b for a, b in zip(choice.aim, ground.point))
                global_heading = HEADINGS[route_wing]
                length = math.hypot(*direction) * math.hypot(*global_heading)
                alignment = sum(a * b for a, b in zip(direction, global_heading)) / length if length else 1
                if (
                    hub_point is None
                    and alignment <= math.cos(math.radians(150))
                    and flow.reason not in ("follow_observed_bend", "observed_corner_turn", "observed_corner_probe")
                    and route_memory is None
                ):
                    held.release()
                    status, reason = "field_paused", "opposite_wing_aim_rejected"
                    row["action"] = "opposite_direction_hold"
                    break
                if hover:
                    held.release()
                    # 이름표 박스는 실제 상자 경계가 아니다. 호버를 만든 커서 위치도 피해야 같은 상자를 다시 고르지 않는다.
                    hovered_aim = last_aim or choice.aim
                    radius = path.teleport["clearance_pixels"] * 2
                    blocked_aims.append(
                        (hovered_aim[0] - radius, hovered_aim[1] - radius, hovered_aim[0] + radius, hovered_aim[1] + radius)
                    )
                    exclusions = tuple(blocked_aims) + tuple(candidate.evidence_box for candidate in hover)
                    if route_memory is not None:
                        alternate = route_memory.choose(
                            path,
                            view,
                            route_wing,
                            clock(),
                            excluded=exclusions,
                            landings=landings,
                            hub=hub_point if return_from is None else None,
                            prefer_goal_exit=return_from is not None,
                        )
                    else:
                        alternate = path.choose(
                            view, route_wing, excluded=exclusions, allow_detour=True, heading=flow.detour, prefer_frontier=True
                        )
                    if alternate.aim is None:
                        status, reason = "field_paused", "hover_path_unobserved"
                        break
                    backend.move(*frame_to_desktop(capture, alternate.aim))
                    last_aim = alternate.aim
                    steering.aim = None
                    progress["last_aim"] = last_aim
                    row["action"] = "hover_reaim_only"
                    row["hovered_aim_excluded"] = hovered_aim
                    row["hover_alternate"] = asdict(alternate)
                    continue
                current = cv.resize(cv.cvtColor(frame[180:890, 120:1720], cv.COLOR_BGR2GRAY), (160, 71))
                if comparison is not None and float(np.abs(current.astype(np.int16) - comparison).mean()) >= 3:
                    last_changed_at = now
                    last_changed_casts = held.snapshot()["dispatches"][held.keys[0]]
                    comparison = current.astype(np.int16)
                elif comparison is None:
                    comparison = current.astype(np.int16)
                    # 전투/전리품 검사 뒤 첫 새 화면은 이전 시전·검사 시간을 막힘으로 누적하지 않는다.
                    last_changed_at = now
                    last_changed_casts = held.snapshot()["dispatches"][held.keys[0]]
                # 대응 실패는 정지 증거가 아니다. 바뀌는 화면에서 대응이 끊긴
                # 넓은 텔레포트를 막힘으로 세지 않고 현재 정지 대응을 요구한다.
                dispatched = held.snapshot()
                movement_casts = dispatched["dispatches"][held.keys[0]]
                if (now - last_changed_at >= 2 and movement_casts > last_changed_casts) or (
                    stationary_since is not None and now - stationary_since >= 2 and movement_casts > stationary_casts
                ):
                    # 막힌 시전은 먼저 해제한다. 새 착지점도 같은 누름의 연장으로 보내지 않는다.
                    held.release()
                    if recoveries >= 3 or last_aim is None:
                        status, reason = "field_paused", "similar_frames_stuck"
                        row["action"] = "stuck_hold"
                        break
                    radius = path.teleport["clearance_pixels"] * 2
                    cast_pointer = dispatched["cast_pointer"]
                    failed_aim = desktop_to_frame(capture, cast_pointer)
                    blocked_aims.append((failed_aim[0] - radius, failed_aim[1] - radius, failed_aim[0] + radius, failed_aim[1] + radius))
                    if route_memory is not None:
                        route_memory.fail_landing(failed_aim, ground.point)
                        alternate = route_memory.choose(
                            path,
                            view,
                            route_wing,
                            clock(),
                            excluded=tuple(blocked_aims),
                            landings=landings,
                            hub=hub_point if return_from is None else None,
                            prefer_goal_exit=return_from is not None,
                        )
                    else:
                        alternate = path.choose(
                            view,
                            route_wing,
                            excluded=tuple(blocked_aims),
                            heading=flow.detour,
                            allow_detour=flow.detour is None,
                            prefer_frontier=flow.detour is None,
                        )
                    row["recovery_choice"] = asdict(alternate)
                    if alternate.aim is None:
                        held.release()
                        if alternate.reason == "graph_frame_stale":
                            # 늦어진 회복 계산을 탈출 바닥 부재로 오인하지 않고 새 화면에서 다시 고른다.
                            recoveries += 1
                            steering.aim = None
                            last_changed_at, last_changed_casts = now, movement_casts
                            stationary_since = None
                            row["action"] = "stuck_replan_fresh_frame"
                            continue
                        status, reason = "field_paused", "stuck_no_observed_escape"
                        row["action"] = "stuck_hold"
                        break
                    recoveries += 1
                    choice, steering.aim, last_changed_at = alternate, None, now
                    stationary_since = None
                    row.update(choice=asdict(choice), recovery="blocked_aim_excluded", blocked_aims=tuple(blocked_aims))
                    evidence.append(("continuous-{:04d}-reaim.png".format(offset + sequence), frame))
                    emit("NORTH_RECOVERY: fresh observed alternate floor; blocked aim excluded")
                steer, steering_reason = steering.update(view, choice, now)
                if steering_reason == "opposite_floor_recheck":
                    held.release()
                    row.update(action="turn_recheck_hold", steering_changed=False, steering_reason=steering_reason)
                    continue
                # 아직 F2가 나가지 않았어도 현재 바닥이 지지하는 조준은 유지한다.
                # 새 후보의 작은 흔들림으로 커서 대기를 매 화면 다시 시작하지 않는다.
                travel_aim = steering.aim if steering_reason == "hold_current_aim" else choice.aim
                aim = frame_to_desktop(capture, travel_aim) if steer or not held.held else None
                held.apply(
                    FieldDirective("travel", "continuous_observed_floor", aim=aim, deadline=min(acquired + 0.25, deadline)),
                    clock(),
                )
                if held.held:
                    accepted_detour = flow.detour
                if steer:
                    last_aim = choice.aim
                    progress.update(last_aim=last_aim)
                    if (
                        flow.reason in ("bend_entered", "forward_reopened")
                        or steering_reason == "previous_floor_closed"
                        or (route_memory is not None and (len(rows) < 2 or rows[-2].get("choice", {}).get("reason") != choice.reason))
                    ):
                        recent_turns.append(("continuous-{:04d}-turn.png".format(offset + sequence), frame))
                row["input_after"] = held.snapshot()
                row["action"] = "hold_teleport" if held.keys[0] in row["input_after"]["held"] else "teleport_aim_settling"
                row["steering_changed"], row["steering_reason"] = steer, steering_reason
                if sequence == 1 or sequence % 10 == 0:
                    emit(
                        "NORTH_CONTINUOUS frame={} action={} age_ms={} aim={}".format(
                            sequence, row["action"], round((now - acquired) * 1000), choice.aim
                        )
                    )
    finally:
        watchdog_stop.set()
        worker.join(timeout=1)
        held.close()
        if map_hidden:
            inspection = progress[map_inspection_kind + "_map_inspection"]
            if stop.is_set() or not focused():
                inspection["status"] = "restore_interrupted"
                status, reason = "field_paused", map_inspection_kind + "_map_restore_interrupted"
            else:
                try:
                    restore_map()
                    if status in ("wing_end_observed", "hub_arrived") and clock() - rows[-1]["processed_at"] >= 0.5:
                        status, reason = "field_paused", map_inspection_kind + "_map_restore_delayed"
                except RuntimeError as exc:
                    inspection.update(status="restore_unverified", error=str(exc))
                    status, reason = "field_paused", map_inspection_kind + "_map_restore_unverified"
        progress["input_dispatches"] = held.snapshot()["dispatches"]
        progress["approach_dispatches"] = travel_frames
        if route_memory is not None:
            progress["route_graph"] = route_memory.summary()
        travel_frames += progress["input_dispatches"][held.keys[0]]
        evidence.extend(recent_turns)
        if first_frame is not None:
            evidence.append(("continuous-{:04d}-before.png".format(offset), first_frame))
        if last_frame is not None:
            evidence.append(("continuous-{:04d}-after.png".format(len(rows)), last_frame))
    if watchdog_errors:
        raise RuntimeError("continuous watchdog: " + watchdog_errors[0])
    if not travel_frames and status == "north_segment_observed":
        status, reason = "field_paused", "no_valid_steering"
    progress.pop("steering_heading", None)
    progress.update(
        continuous_status=reason,
        position_current=position_current,
        wing=wing,
        return_from=return_from,
        engage_monsters=engage_monsters,
        route_detour=accepted_detour,
    )
    if route_memory is not None:
        progress["route_graph"] = route_memory.summary()
    # ALT 검사/입력 정리가 마지막 지형 프레임보다 늦게 끝날 수 있다.
    # 이 시각은 같은 프로세스의 즉시 인계만 증명한다. 다음 입력은 새 HUD/발/지형을 다시 요구한다.
    progress["interval_finished_at"] = clock()
    emit("NORTH_CONTINUOUS_STOP: " + reason)
    return dict(status=status, **progress)
