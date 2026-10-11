from __future__ import annotations

import json
import math
import time
from copy import copy
from dataclasses import asdict, replace
from pathlib import Path

from diablo2.common.supervised_field import resource_guard


def load_arcane_step_planner(*, automap=False, character_class="sorceress"):
    import cv2 as cv
    from diablo2.runs.summoner.navigation import ArcanePathObserver, PathSettings
    from diablo2.runs.summoner.scene_vision import FlashGroundObserver

    root = Path(__file__).resolve().parents[3]
    owner = root / "assets/character/sorceress/remastered-1922x1140"
    marker = json.loads((owner / "manifest.json").read_text(encoding="utf-8"))["marker"]
    calibration = json.loads(
        (root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/navigation-calibration.json").read_text(encoding="utf-8")
    )
    settings = dict(calibration["path"], seed_radius=96, maximum_step=120)
    body = marker.get("body", {})
    ground = FlashGroundObserver(
        cv.imread(str(owner / marker["path"])),
        search_box=tuple(marker["search_box"]),
        anchor_offset=tuple(marker["anchor_offset"]),
        minimum_score=marker["minimum_score"],
        variants=[cv.imread(str(owner / filename)) for filename in marker.get("variants", [])],
        body_templates=[cv.imread(str(owner / filename)) for filename in body.get("templates", [])],
        body_box=None if not body else tuple(body["box_offset"]),
        body_minimum_score=body.get("minimum_score", 0.82),
        tracking=marker.get("tracking"),
    )
    if character_class != "sorceress":
        from diablo2.runs.summoner.scene_vision import GroundReading

        class CameraOnlyGround:
            def observe(self, frame, *, captured_at=None):
                return GroundReading(None, "ground_unverified")

        ground = CameraOnlyGround()
    return ground, ArcanePathObserver(
        PathSettings(**settings),
        terrain=calibration["terrain"],
        teleport=calibration.get("teleport"),
        automap_overlay=calibration.get("automap_overlay") if automap else None,
    )


def plan_arcane_step(
    frame,
    wing,
    ground,
    path,
    captured_at,
    *,
    hub_point=None,
    position=(0.0, 0.0),
    visited=(),
    clock=time.monotonic,
    allow_detour=False,
    heading=None,
    prefer_frontier=False,
    allow_no_aim=False,
    excluded=(),
):
    reading = ground.observe(frame, captured_at=captured_at)
    if reading.point is None:
        raise RuntimeError(reading.reason)
    x, y = reading.point
    if hub_point is not None and math.hypot(x - hub_point[0], y - hub_point[1]) <= 220:
        # 확인한 중앙 장식 위에서는 별 무늬의 틈을 지나 실제 발판에서 출발한다.
        path = copy(path)
        path.settings = replace(path.settings, seed_radius=180, maximum_step=240)
    # 계단 위 발판이 좁은 지역 crop 밖으로 잘리지 않도록 텔레포트 후보를 넓게 본다.
    half_width, half_height = (720, 600) if path.teleport is not None else (360, 220)
    viewport = (max(24, x - half_width), max(55, y - half_height), min(1900, x + half_width), min(900, y + half_height))
    if prefer_frontier:
        viewport = (24, 55, 1900, 900)
    view = path.observe(frame, sequence=1, generation=0, captured_at=captured_at, viewport=viewport, anchor=reading.point)
    options = dict(position=position, visited=visited)
    if excluded:
        options["excluded"] = excluded
    if allow_detour:
        options["allow_detour"] = True
    if heading is not None:
        options["heading"] = heading
    if prefer_frontier:
        options["prefer_frontier"] = True
    choice = path.choose(view, wing, **options)
    if choice.aim is None and not allow_no_aim:
        raise RuntimeError(choice.reason)
    if not 0 <= clock() - captured_at < 0.25:
        raise RuntimeError("planned step frame is stale")
    return choice, reading, view


def dispatch_planned_step(
    backend, focused, stop, aim, key, captured_at, *, clock=time.monotonic, verify_aim=None, expires_at=None, diagnostics=None
):
    # 포인터 이동 지연도 같은 화면의 유효 시간을 소비한다. F2는 한 번만 누른다.
    def record(stage, reason, **values):
        if diagnostics is not None:
            diagnostics.update(stage=stage, reason=reason, **values)

    def allowed(stage):
        if stop.is_set():
            record(stage, "stop_requested", checked_at=None, frame_age_seconds=None)
            return False
        if not focused():
            record(stage, "focus_lost", checked_at=None, frame_age_seconds=None)
            return False
        now = clock()
        age = now - captured_at
        if not 0 <= age < 0.25:
            reason = "frame_time_reversed" if age < 0 else "frame_expired"
        elif expires_at is not None and not now < expires_at:
            reason = "deadline_expired"
        else:
            reason = "allowed"
        record(stage, reason, checked_at=now, frame_age_seconds=age)
        return reason == "allowed"

    for index, (action, args) in enumerate(((backend.move, aim), (backend.press, (key,)))):
        stage = "before_pointer" if index == 0 else "before_cast"
        if not allowed(stage):
            return False
        if index == 1 and verify_aim is not None:
            if not verify_aim():
                record("pointer_verification", "pointer_mismatch", checked_at=None, frame_age_seconds=None)
                return False
            if not allowed("after_pointer_verification"):
                return False
        action(*args)
        if index == 0:
            # 빠른 시전도 게임이 이전 포인터를 소비하지 않도록 한 화면 처리 시간을 둔다.
            stop.wait(0.05)
    record("cast", "sent")
    return True


def observe_step_landing(
    capture, hud, focused, policy, stop, frame, view, choice, ground, path, *, reference_seconds, clock=time.monotonic, expires_at=None
):
    from diablo2.runs.summoner.navigation import BackgroundMotionObserver

    if not 0 < reference_seconds < 1.5:
        raise ValueError("configured cast reference must fit the landing observation window")
    motion = BackgroundMotionObserver(
        minimum_matches=12,
        minimum_inlier_ratio=0.65,
        maximum_error=3,
        landmark_viewport=(120, 180, 1720, 890),
        actor_exclusion=(160, 230, 160, 75),
    )
    motion.observe(frame, view)
    started, total, stable, rows = clock(), [0.0, 0.0], 0, []
    desired = (choice.aim[0] - view.anchor[0], choice.aim[1] - view.anchor[1])
    desired_length = math.hypot(*desired)
    result = {"status": "arrival_unverified", "displacement": total, "observations": rows}
    for sequence in range(2, 34):
        if expires_at is not None and clock() >= expires_at:
            result["status"] = "segment_time_limit"
            break
        if stop.is_set() or clock() - started >= 1.5:
            break
        if not focused():
            result["status"] = "focus_lost"
            break
        acquired = clock()
        current = capture.grab().frame
        reading = hud.observe(current)
        blocked = resource_guard(reading, policy)
        if blocked == "resource_low":
            result["status"] = blocked
            break
        position = ground.observe(current, captured_at=acquired)
        current_view = path.observe(
            current,
            sequence=sequence,
            generation=view.generation,
            captured_at=acquired,
            viewport=view.viewport,
            anchor=position.point or view.anchor,
        )
        remaining = tuple(requested - observed for requested, observed in zip(desired, total))
        measured = motion.observe(current, current_view, overlap_hint=remaining)
        processed_at = clock()
        elapsed, age = processed_at - started, processed_at - acquired
        rows.append(
            dict(
                elapsed=elapsed,
                processing_seconds=age,
                ground=position.point,
                ground_reason=position.reason,
                motion=asdict(measured),
                resources=blocked,
                life=asdict(reading.life),
                mana=asdict(reading.mana),
                belt_visibility=reading.belt_visibility,
            )
        )
        if expires_at is not None and processed_at >= expires_at:
            result["status"] = "segment_time_limit"
            break
        if not 0 <= age < 0.25:
            result["status"] = "screen_stale"
            break
        if measured.displacement is not None:
            dx, dy = measured.displacement
            total[0], total[1] = total[0] + dx, total[1] + dy
        if measured.displacement is None or blocked:
            stable = 0
        else:
            dx, dy = measured.displacement
            stable = stable + 1 if math.hypot(dx, dy) < 2 else 0
            distance = math.hypot(*total)
            aligned = desired_length > 0 and distance > 0 and sum(a * b for a, b in zip(total, desired)) / distance / desired_length >= 0.8
            if (
                stable >= 2
                and position.point is not None
                and elapsed >= reference_seconds
                and 12 <= distance <= 2 * desired_length + 25
                and aligned
                and focused()
                and not stop.is_set()
            ):
                result["status"] = "arrival_observed"
                break
        stop.wait(0.02)
    return result
