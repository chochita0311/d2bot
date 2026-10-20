"""처치·전리품 확인 뒤 일지와 붉은 포탈을 거쳐 협곡 WP에서 마감한다."""

import time
import hashlib
import json
import math
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.supervised_field import resource_guard
from diablo2.common.frame_geometry import frame_to_desktop
from diablo2.runs.summoner.journal_vision import HorazonJournalObserver
from diablo2.runs.summoner.navigation_execution import dispatch_planned_step


class CanyonApproachObserver:
    def __init__(self):
        root = Path(__file__).resolve().parents[3]
        owner = root / "assets/regions/canyon-of-the-magi/remastered-ko-1922x1140"
        self.rule = json.loads((owner / "manifest.json").read_text(encoding="utf-8"))["parts"]["waypoint-south-steps"]
        path = root / self.rule["path"]
        if hashlib.sha256(path.read_bytes()).hexdigest() != self.rule["sha256"]:
            raise ValueError("reviewed Canyon step fingerprint required")
        self.template = cv.imread(str(path), cv.IMREAD_GRAYSCALE)

    def observe(self, frame):
        if frame.shape != (1140, 1922, 3):
            raise ValueError("native Canyon frame required")
        patch = cv.cvtColor(frame[55:900, 24:1900], cv.COLOR_BGR2GRAY)
        _, score, _, (x, y) = cv.minMaxLoc(cv.matchTemplate(patch, self.template, cv.TM_CCOEFF_NORMED))
        if score < self.rule["threshold"]:
            return None
        height, width = self.template.shape
        return (x + width / 2 + 24, y + height / 2 + 55)


def red_portal(frame):
    """현재 월드의 유일한 붉은 타원 고리만 후보로 반환한다. 지역은 호출자가 확인한다."""
    if frame.shape != (1140, 1922, 3):
        raise ValueError("native portal frame required")
    patch = frame[55:900, 24:1900]
    hsv = cv.cvtColor(patch, cv.COLOR_BGR2HSV)
    mask = cv.inRange(hsv, (0, 95, 160), (12, 255, 255)) | cv.inRange(hsv, (170, 95, 160), (179, 255, 255))
    mask = cv.morphologyEx(mask, cv.MORPH_CLOSE, np.ones((9, 9), np.uint8))
    _, _, stats, _ = cv.connectedComponentsWithStats(mask)
    candidates = []
    for x, y, width, height, area in stats[1:]:
        if not (70 <= width <= 250 and 120 <= height <= 360 and 0.3 <= width / height <= 0.85 and area >= 900):
            continue
        core = hsv[y + height // 3 : y + height * 2 // 3, x + width // 3 : x + width * 2 // 3]
        if core.size and np.mean(core[:, :, 2] < 150) >= 0.6:
            candidates.append((int(x + width // 2 + 24), int(y + height // 2 + 55)))
    return candidates[0] if len(candidates) == 1 else None


def enter_canyon(
    backend,
    capture,
    focused,
    stop,
    hud,
    profile,
    planner,
    town_vision,
    *,
    kill_confirmed,
    loot_complete,
    evidence,
    buff_expires_at,
    emit=print,
    clock=time.monotonic,
    reference_seconds=0.32,
):
    """확인되지 않은 처치/줍기는 거부한다. 한 단계에 현재 화면의 입력 한 번만 보낸다."""
    if kill_confirmed is not True or loot_complete is not True:
        raise ValueError("confirmed Summoner death and completed loot required before journal exit")
    phase, actions, canyon_approaches, journal_approaches = "journal", [], 0, 0
    canyon_steps = CanyonApproachObserver()
    journal = HorazonJournalObserver()
    deadline = min(clock() + 30, buff_expires_at)

    def guard():
        if stop.is_set() or not focused():
            raise RuntimeError("canyon finish interrupted or unfocused")
        if clock() >= deadline:
            raise RuntimeError("canyon finish deadline reached")

    def click_world(point, acquired):
        if not 24 <= point[0] < 1900 or not 55 <= point[1] < 900:
            raise RuntimeError("canyon finish target outside world")
        guard()
        aim = frame_to_desktop(capture, point)
        backend.move(*aim)
        guard()
        if not 0 <= clock() - acquired < 0.25 or not backend.pointer_matches(aim):
            raise RuntimeError("canyon finish screen or pointer stale")
        backend.click()

    while True:
        guard()
        acquired = clock()
        frame = capture.grab().frame
        reading = hud.observe(frame)
        blocked = resource_guard(reading, profile.survival)
        if blocked:
            if phase == "arrival" and blocked == "observation_unverified":
                stop.wait(0.04)
                continue
            raise RuntimeError(blocked)
        gray = town_vision.normalize(frame)
        if town_vision.find(gray, "area_canyon") and town_vision.find(gray, "hud"):
            if town_vision.find(gray, "world_canyon_waypoint") is not None:
                evidence.append(("canyon-arrival.png", frame))
                return dict(status="canyon_waypoint_observed", actions=actions)
            point = canyon_steps.observe(frame)
            if point is None or canyon_approaches >= 2:
                raise RuntimeError("Canyon waypoint approach unverified")
            if not 0 <= clock() - acquired < 0.25:
                raise RuntimeError("canyon finish screen stale")
            guard()
            click_world(point, acquired)
            actions.append(dict(reason="Approach current Canyon waypoint stairs", point=point, acquired_at=acquired))
            evidence.append(("canyon-approach-{:02d}.png".format(canyon_approaches), frame))
            canyon_approaches += 1
            stop.wait(0.4)
            continue
        if town_vision.find(gray, "save_exit") or town_vision.find(gray, "waypoint_header"):
            raise RuntimeError("unexpected panel during journal exit")
        point, reason = None, None
        arcane = town_vision.find(gray, "area_arcane")
        if phase == "journal" and arcane is not None:
            point = journal.observe(frame)
            if point is not None:
                ground, path = planner
                prepare = getattr(ground, "prepare_frame", None)
                if prepare is not None:
                    prepare(frame, acquired, reading, focused())
                player = ground.observe(frame, captured_at=acquired).point
                if player is None:
                    raise RuntimeError("journal approach ground unverified")
                # 먼 일지 클릭은 보행을 시작한다. 즉시 바닥 클릭이 그 보행을 취소하지 않도록 먼저 F2로 접근한다.
                if math.hypot(point[0] - player[0], point[1] - player[1]) > 220:
                    if journal_approaches >= 2:
                        raise RuntimeError("journal approach limit reached")
                    goal = (point[0], point[1] + 100)
                    view = path.observe(
                        frame, sequence=len(actions), generation=0, captured_at=acquired, viewport=(120, 180, 1720, 890), anchor=player
                    )
                    choice = path.choose(view, "north", destination=(goal[0] - player[0], goal[1] - player[1]))
                    if choice.aim is None or math.hypot(choice.aim[0] - goal[0], choice.aim[1] - goal[1]) > 100:
                        raise RuntimeError("journal approach floor unverified")
                    aim = frame_to_desktop(capture, choice.aim)
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
                    )
                    if sent:
                        journal_approaches += 1
                        actions.append(dict(reason="Teleport near current Horazon journal", point=choice.aim, acquired_at=acquired))
                        evidence.append(("journal-approach-{:02d}.png".format(journal_approaches), frame))
                        emit("SUMMONER_FINISH: Teleport near current Horazon journal")
                        stop.wait(reference_seconds)
                    continue
                reason, phase = "Click current Horazon journal", "dismiss"
        elif phase == "dismiss":
            ground = planner[0]
            prepare = getattr(ground, "prepare_frame", None)
            if prepare is not None:
                prepare(frame, acquired, reading, focused())
            player = ground.observe(frame, captured_at=acquired).point
            if player is not None:
                point, reason, phase = (player[0] - 45, player[1] + 30), "Click nearby floor immediately after journal", "portal"
        elif phase == "portal" and arcane is not None:
            portal = red_portal(frame)
            if portal is not None:
                point, reason, phase = portal, "Enter current red portal to Canyon of the Magi", "arrival"
        if not 0 <= clock() - acquired < 0.25:
            raise RuntimeError("canyon finish screen stale")
        if point is not None:
            click_world(point, acquired)
            actions.append(dict(reason=reason, point=point, acquired_at=acquired))
            evidence.append(("journal-exit-{:02d}.png".format(len(actions)), frame))
            emit("SUMMONER_FINISH: " + reason)
        stop.wait(0.04)
