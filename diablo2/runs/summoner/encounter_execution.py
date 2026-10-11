from dataclasses import dataclass, asdict
from collections import deque
import hashlib
import json
import math
from pathlib import Path
import threading
import time

import cv2 as cv

from diablo2.common.field_control import HeldFieldInput, FieldDirective
from diablo2.common.supervised_field import resource_guard
from diablo2.common.hostile_probe import NearbyHostileProbe
from diablo2.runs.summoner.continuous_navigation import near_ground
from diablo2.runs.summoner.navigation_execution import dispatch_planned_step


@dataclass(frozen=True)
class SummonerAppearance:
    box: tuple
    ground: tuple
    score: float


class SummonerAppearanceObserver:
    """현재 룬 발판의 검토된 로브 외형만 찾는다. 신원/사망 확정과 구분한다."""

    def __init__(self):
        owner = Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        data = json.loads((owner / "summoner-encounter.json").read_text(encoding="utf-8"))
        self.rule = data["parts"][0]
        path = owner / self.rule["path"]
        if hashlib.sha256(path.read_bytes()).hexdigest() != self.rule["sha256"]:
            raise ValueError("reviewed Summoner appearance fingerprint required")
        self.template = cv.imread(str(path), cv.IMREAD_COLOR)

    def observe(self, frame, platform):
        if frame.shape != (1140, 1922, 3):
            raise ValueError("native Summoner appearance frame required")
        if platform is None:
            return None
        patch = frame[55:900, 24:1900]
        scores = cv.matchTemplate(patch, self.template, cv.TM_CCOEFF_NORMED)
        _, score, _, (x, y) = cv.minMaxLoc(scores)
        if score < self.rule["threshold"]:
            return None
        x, y = x + 24, y + 55
        h, w = self.template.shape[:2]
        dx, dy = self.rule["ground_offset"]
        point = (x + dx, y + dy)
        boxes = platform.rune_boxes
        if not boxes or not min(b[0] for b in boxes) - 100 <= point[0] <= max(b[2] for b in boxes) + 100:
            return None
        if not min(b[1] for b in boxes) - 50 <= point[1] <= max(b[3] for b in boxes) + 500 or point[1] >= 900:
            return None
        return SummonerAppearance((x, y, x + w, y + h), point, float(score))


def run_platform_encounter(
    capture,
    hud,
    backend,
    focused,
    profile,
    stop,
    planner,
    objective,
    appearance,
    *,
    buff_expires_at,
    reference_seconds,
    evidence,
    emit=print,
    clock=time.monotonic,
    target_observer=None,
    recover_resources=None,
    loot_controller=None,
):
    """현재 외형으로 제한된 접근/노바를 실행하고 처치 확인 전에는 복귀를 승인하지 않는다."""
    started = clock()
    deadline = min(started + 12, buff_expires_at)
    result = dict(status="kill_unconfirmed", observations=[], approach_requests=0, nova_dispatches=0, evidence=[])
    held = HeldFieldInput(
        backend, profile.actions.movement_skill_key, profile.actions.primary_attack_skill_key, clock=clock, repeat_seconds=reference_seconds
    )
    watchdog_stop = threading.Event()
    errors = []

    def watchdog():
        while not watchdog_stop.wait(0.02):
            try:
                if stop.is_set() or not focused() or clock() >= deadline:
                    held.release()
                else:
                    held.service(clock())
            except Exception as exc:
                errors.append(str(exc))
                stop.set()
                return

    worker = threading.Thread(target=watchdog, name="summoner-encounter-watchdog", daemon=True)
    worker.start()
    first_nova, missing_since, last, last_sequence = None, None, None, None
    prelude, saved, last_saved = deque(maxlen=4), 0, float("-inf")
    probe = (
        NearbyHostileProbe(target_observer, backend, (capture.target["left"], capture.target["top"]), clock=clock)
        if target_observer is not None
        else None
    )

    def retain(name, frame, sequence, acquired, phase):
        evidence.append((name, frame))
        result["evidence"].append(dict(file=name, sequence=sequence, acquired_at=acquired, phase=phase))

    try:
        for sequence in range(120):
            if stop.is_set() or not focused() or clock() >= deadline:
                break
            if loot_controller is not None and not loot_controller.movement_allowed:
                result["blocked"] = "item_labels_not_off"
                break
            acquired = clock()
            last = capture.grab().frame
            last_sequence = sequence
            prelude.append((sequence, acquired, last))
            if sequence == 0:
                retain("encounter-before.png", last, sequence, acquired, "initial")
                saved += 1
            row = dict(sequence=sequence, acquired_at=acquired, target=None, player=None, platform_ground=None)
            result["observations"].append(row)
            reading = hud.observe(last)
            blocked = resource_guard(reading, profile.survival)
            if blocked:
                row["blocked"] = blocked
                held.release()
                if blocked == "resource_low":
                    if recover_resources is not None and recover_resources(last, reading, acquired):
                        if probe is not None:
                            probe.reset()
                        continue
                    raise RuntimeError(blocked)
                continue
            prepare = getattr(planner[0], "prepare_frame", None)
            if prepare is not None:
                prepare(last, acquired, reading, focused())
            player = planner[0].observe(last, captured_at=acquired)
            platform = objective.observe(last)
            target = appearance.observe(last, platform)
            goal = target.ground if target is not None else getattr(platform, "ground_point", None)
            row.update(target=None if target is None else asdict(target), player=player.point, platform_ground=goal)
            now = clock()
            row["processed_at"] = now
            if first_nova is not None and saved < 23 and acquired - last_saved >= 0.12:
                retain("encounter-combat-{:03d}.png".format(sequence), last, sequence, acquired, "combat")
                saved += 1
                last_saved = acquired
            if player.point is None or goal is None or not 0 <= now - acquired < 0.25:
                held.release()
                missing_since = now if missing_since is None else missing_since
                if now - missing_since >= 1:
                    break
                continue
            missing_since = None
            if near_ground(goal, player.point):
                hostile = None
                if target is None and probe is not None:
                    if probe.pending is None:
                        held.release()
                    hostile = probe.observe(last, player.point, acquired)
                    row["hostile_panel"] = None if hostile is None else asdict(hostile)
                if target is None and hostile is None:
                    held.release()
                    row["action"] = "platform_hostile_probe" if probe is not None else "platform_hostile_unverified"
                    continue
                if first_nova is None:
                    for before_sequence, before_at, before_frame in prelude:
                        if saved >= 23:
                            break
                        retain(
                            "encounter-prefight-{:03d}.png".format(before_sequence), before_frame, before_sequence, before_at, "prefight"
                        )
                        saved += 1
                    last_saved = acquired
                first_nova = now if first_nova is None else first_nova
                if now - first_nova >= 6:
                    break
                held.apply(
                    FieldDirective(
                        "combat",
                        "current_platform_robe_candidate" if target is not None else "current_platform_hostile_panel",
                        deadline=min(acquired + 0.25, deadline),
                    ),
                    clock(),
                )
                row["action"] = "nova_on_current_appearance" if target is not None else "nova_on_current_hostile_panel"
            else:
                held.release()
                if result["approach_requests"] >= 2:
                    break
                ground, path = planner
                view = path.observe(
                    last, sequence=sequence, generation=0, captured_at=acquired, viewport=(120, 180, 1720, 890), anchor=player.point
                )
                destination = tuple(a - b for a, b in zip(goal, player.point))
                choice = path.choose(view, "north", destination=destination)
                if choice.aim is None or math.hypot(*(a - b for a, b in zip(choice.aim, goal))) > 100:
                    row["action"] = "target_floor_unverified"
                    break
                aim = (capture.target["left"] + choice.aim[0], capture.target["top"] + choice.aim[1])
                dispatch = dict(aim=aim, captured_at=acquired, expires_at=deadline)
                row["approach_dispatch"] = dispatch
                sent = dispatch_planned_step(
                    backend,
                    focused,
                    stop,
                    aim,
                    profile.actions.movement_skill_key,
                    acquired,
                    clock=clock,
                    verify_aim=lambda: backend.pointer_matches(aim),
                    expires_at=deadline,
                    diagnostics=dispatch,
                )
                row["action"] = "approach_requested" if sent else "approach_not_sent"
                result["approach_requests"] += int(sent)
                if not sent:
                    break
                stop.wait(reference_seconds)
        emit("SUMMONER_ENCOUNTER: bounded approach/Nova finished; disappearance is not death confirmation")
    finally:
        watchdog_stop.set()
        worker.join(timeout=1)
        held.close()
        result["nova_dispatches"] = held.snapshot()["dispatches"][profile.actions.primary_attack_skill_key]
        result["evidence_frames"] = saved + int(last is not None)
        if last is not None:
            retain("encounter-after.png", last, last_sequence, acquired, "final")
    if errors:
        raise RuntimeError("encounter watchdog: " + errors[0])
    if loot_controller is not None and result["nova_dispatches"]:
        result["loot_inspection"] = loot_controller.after_combat(
            buff_expires_at=buff_expires_at,
            reason="combat_ended_scan",
            combat_id=("encounter", started, result["nova_dispatches"]),
        )
    return result
