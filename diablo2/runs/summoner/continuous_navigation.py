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
from diablo2.common.hostile_probe import NearbyHostileProbe, nearby_actor_proposals
from diablo2.common.scene_motion import BackgroundMotionObserver
from diablo2.common.supervised_field import resource_guard, validate_permit
from diablo2.runs.summoner.navigation_execution import dispatch_planned_step, plan_arcane_step
from diablo2.runs.summoner.navigation import HEADINGS
from diablo2.runs.summoner.steering import ArcaneRouteFlow, ContinuousSteering, floor_supports_aim
from diablo2.runs.summoner.terminal_vision import TerminalStabilityTracker
from diablo2.runs.summoner.scene_vision import HubStabilityTracker


OPPOSITE_WINGS = dict(north="south", east="west", south="north", west="east")


class ArrivalDwell:
    """keyUp 전에 시작된 시전까지 끝난 뒤 현재 근접·정지를 확인한다."""

    def __init__(self, reference_seconds):
        self.minimum_seconds = max(0.6, reference_seconds * 2)
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
    if (
        progress.get("continuous_status") != "interval_complete"
        or progress.get("wing") != wing
        or progress.get("return_from") != return_from
        or type(previous) not in (int, float)
        or not math.isfinite(previous)
        or not 0 <= now - previous < 0.5
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
        NearbyHostileProbe(target_vision, backend, (capture.target["left"], capture.target["top"]), clock=clock)
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
    hub_near_wait = False
    motion_missing_since = None
    motion_evidence, matched_frame = 0, None
    offset = len(rows)
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
                    stationary_since = None
                    hub_arrival.reset()
                    terminal_arrival.reset()
                    terminal_tracker.reset()
                    hub_tracker.reset()
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
                )
                terminal_work = None if terminal_vision is None or return_from is not None else pool.submit(terminal_vision.observe, frame)
                objective_work = (
                    None if objective_vision is None or return_from is not None else pool.submit(objective_vision.observe, frame)
                )
                hub_point = None if hub_vision is None else hub_vision.observe(frame)
                hub_evidence = None if hub_point is None else getattr(hub_vision, "last_evidence", None)
                if return_from is not None and hub_point is not None:
                    # 중앙 발견이 일반 조향/반복 입력보다 먼저 이동을 선점한다.
                    progress["hub_acquired"] = True
                    held.release()
                if return_from is None and progress.get("terminal_acquired"):
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
                terminal = None if terminal_work is None else terminal_work.result()
                valid_terminal = terminal is not None and terminal.kind in ("nonboss_chest_trio", "nonboss_chest_platform")
                if valid_terminal and 0 <= clock() - acquired < 0.25:
                    # 끝 실물이 보이면 몸/경로 판독보다 먼저 멈춘다. 도착은 아래에서 따로 확인한다.
                    if not progress.get("terminal_acquired"):
                        evidence.append(("terminal-first-acquired.png", frame))
                    progress["terminal_acquired"] = True
                    held.release()
                try:
                    choice, ground, view = planning_work.result()
                except RuntimeError as exc:
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
                if return_from is None and not rows and hub_vision is not None and not resumed_wing:
                    if hub_point is None or math.hypot(*(a - b for a, b in zip(ground.point, hub_point))) > 100:
                        raise RuntimeError("outbound_requires_central_waypoint")
                # HUD와 발 위치가 다시 판독되면 이전 가림의 시간을 끊는다.
                # 전투/중앙 접근은 아래의 일반 이동 분기를 거치지 않을 수 있다.
                unavailable_since = None
                view = replace(view, sequence=sequence)
                # 경로 기억/배경 대응보다 먼저 같은 새 화면의 현재 조준을 검사한다.
                # 이미 유지 중인 키만 연장하며 새 조준/해제된 키는 아래 최종 판단이 맡는다.
                observed_pointer = held.snapshot()["pointer"]
                observed_aim = (
                    (observed_pointer[0] - capture.target["left"], observed_pointer[1] - capture.target["top"])
                    if observed_pointer is not None
                    else last_aim or choice.aim
                )
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
                    proposals = nearby_actor_proposals(frame, ground.point) if attack_started is None else ()
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
                preflight_at, renewed_at = clock(), None
                active = held.snapshot()
                pointer = active["pointer"]
                if pointer is not None and active["held"] == (held.keys[0],):
                    current_aim = (pointer[0] - capture.target["left"], pointer[1] - capture.target["top"])
                    close = any(near_ground(candidate.ground, ground.point) for candidate in candidates)
                    close = close or (target is not None and near_ground(current_aim, ground.point))
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
                    else (pointer[0] - capture.target["left"] - ground.point[0], pointer[1] - capture.target["top"] - ground.point[1])
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
                    )
                    if choice.reason.startswith("current_hub_departure_") and last_aim is not None:
                        previous_direction = tuple(a - b for a, b in zip(last_aim, ground.point))
                        if ArcaneRouteFlow.axis(previous_direction) != HEADINGS[route_wing]:
                            if profile.actions.movement_skill_key in held.held:
                                held.release()
                            steering.aim = None
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
                    candidates=[asdict(c) for c in candidates],
                    hover=[asdict(c) for c in hover],
                    target=None if target is None else asdict(target),
                    nearby_hover_confirmed=probe_confirmed,
                    nearby_hover_pending=probe_pending,
                    nearby_actor_proposals=proposals,
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
                if choice.aim is not None:
                    aim_unavailable_since = None
                if route_memory is not None:
                    row["route_graph"] = route_memory.summary()
                    row["terrain_landings"] = [asdict(item) for item in landings]
                if not 0 <= now - acquired < 0.25 or now >= deadline:
                    held.release()
                    hub_arrival.reset()
                    terminal_arrival.reset()
                    row["action"] = "stale_hold"
                    continue
                close = any(near_ground(candidate.ground, ground.point) for candidate in candidates)
                close = close or probe_confirmed or (target is not None and near_ground(last_aim, ground.point))
                if probe_pending and attack_started is None and not close:
                    held.release()
                    row["action"] = "nearby_hover_probe"
                    continue
                if close:
                    if attack_started is None:
                        attack_started = now
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
                    if hub_point is None:
                        hub_arrival.reset()
                        hub_approach_settle.reset()
                        hub_missing_since = now if hub_missing_since is None else hub_missing_since
                        row["action"] = "central_reacquire_hold"
                        pair = hub_vision.current_pair_near(frame, ground.point)
                        stationary = measured.displacement is not None and math.hypot(*measured.displacement) < 2
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
                        status, reason = "field_paused", "central_approach_floor_unverified"
                        break
                    aim = (capture.target["left"] + choice.aim[0], capture.target["top"] + choice.aim[1])
                    sent = dispatch_planned_step(
                        backend, focused, stop, aim, profile.actions.movement_skill_key, acquired, clock=clock, expires_at=deadline
                    )
                    row["action"] = "central_approach_cast" if sent else "central_approach_stale_hold"
                    travel_frames += int(sent)
                    hub_approach_settle.reset()
                    continue
                hub_arrival.reset()
                terminal_target = terminal if valid_terminal else terminal_tracker.approach
                terminal_distance = (
                    None if terminal_target is None else math.hypot(*(a - b for a, b in zip(terminal_target.point, ground.point)))
                )
                if return_from is None and progress.get("terminal_acquired"):
                    held.release()
                    stationary_since = None
                    approach_radius = 400 if valid_terminal else 200
                    if terminal_target is None or (not valid_terminal and terminal_distance <= approach_radius):
                        terminal_arrival.reset()
                        hub_approach_settle.reset()
                        terminal_missing_since = now if terminal_missing_since is None else terminal_missing_since
                        row["action"] = "terminal_reacquire_hold"
                        if now - terminal_missing_since >= 1.5:
                            status, reason = "field_paused", "terminal_reacquisition_required"
                            break
                        continue
                    terminal_missing_since = None
                    if terminal_distance > approach_radius:
                        terminal_arrival.reset()
                        row["action"] = "terminal_approach_settle"
                        if not hub_approach_settle.update(True, measured.displacement, now):
                            continue
                        approach = path.choose(
                            view, route_wing, destination=tuple(a - b for a, b in zip(terminal_target.point, ground.point))
                        )
                        if approach.aim is None or math.hypot(*(a - b for a, b in zip(approach.aim, terminal_target.point))) > max(
                            approach_radius, terminal_distance - 200
                        ):
                            status, reason = "field_paused", "terminal_approach_floor_unverified"
                            break
                        sent = dispatch_planned_step(
                            backend,
                            focused,
                            stop,
                            (capture.target["left"] + approach.aim[0], capture.target["top"] + approach.aim[1]),
                            profile.actions.movement_skill_key,
                            acquired,
                            clock=clock,
                            expires_at=deadline,
                        )
                        row["action"] = "terminal_approach_cast" if sent else "stale_hold"
                        travel_frames += int(sent)
                        hub_approach_settle.reset()
                        continue
                if valid_terminal and terminal_distance <= 400:
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
                terminal_arrival.reset()
                if choice.aim is None:
                    held.release()
                    row["action"] = "path_hold"
                    aim_unavailable_since = acquired if aim_unavailable_since is None else aim_unavailable_since
                    if now - aim_unavailable_since >= 1.5:
                        status, reason = "field_paused", choice.reason
                        break
                    continue
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
                        )
                    else:
                        alternate = path.choose(
                            view, route_wing, excluded=exclusions, allow_detour=True, heading=flow.detour, prefer_frontier=True
                        )
                    if alternate.aim is None:
                        status, reason = "field_paused", "hover_path_unobserved"
                        break
                    backend.move(capture.target["left"] + alternate.aim[0], capture.target["top"] + alternate.aim[1])
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
                # 대응 실패는 정지 증거가 아니다. 바뀌는 화면에서 대응이 끊긴
                # 넓은 텔레포트를 막힘으로 세지 않고 현재 정지 대응을 요구한다.
                dispatched = held.snapshot()
                movement_casts = dispatched["dispatches"][held.keys[0]]
                if (now - last_changed_at >= 2 and movement_casts > last_changed_casts) or (
                    stationary_since is not None and now - stationary_since >= 2 and movement_casts > stationary_casts
                ):
                    if recoveries >= 3 or last_aim is None:
                        held.release()
                        status, reason = "field_paused", "similar_frames_stuck"
                        row["action"] = "stuck_hold"
                        break
                    radius = path.teleport["clearance_pixels"] * 2
                    cast_pointer = dispatched["cast_pointer"]
                    failed_aim = (cast_pointer[0] - capture.target["left"], cast_pointer[1] - capture.target["top"])
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
                    if alternate.aim is None:
                        held.release()
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
                aim = (capture.target["left"] + choice.aim[0], capture.target["top"] + choice.aim[1]) if steer or not held.held else None
                held.apply(FieldDirective("travel", "continuous_observed_floor", aim=aim, deadline=min(acquired + 0.25, deadline)), clock())
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
    emit("NORTH_CONTINUOUS_STOP: " + reason)
    return dict(status=status, **progress)
