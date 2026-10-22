"""처치·전리품 확인 뒤 일지와 붉은 포탈을 거쳐 협곡 WP에서 마감한다."""

import time
import hashlib
import json
import math
from copy import copy
from functools import lru_cache
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.supervised_field import resource_guard
from diablo2.common.frame_geometry import frame_to_desktop
from diablo2.runs.summoner.journal_vision import HorazonJournalObserver
from diablo2.runs.summoner.navigation_execution import dispatch_planned_step


JOURNAL_CLICK_RADIUS = 220
JOURNAL_SIDE_OFFSET = 160
JOURNAL_FLOOR_OFFSET = 80
JOURNAL_MINIMUM_SIDE_CLEARANCE = 120
JOURNAL_GOAL_TOLERANCE = 100


def journal_landing(path, view, journal, player):
    """책을 몸으로 가리지 않는 현재 안전 바닥을 가까운 쪽부터 고른다."""
    teleport = getattr(path, "teleport", None)
    if isinstance(teleport, dict):
        # 탐색의 짧은 보폭 우선값으로 책 앞에 멈추지 않게 한다. 기존 안전 폭/최대 거리는 보존한다.
        path = copy(path)
        path.teleport = dict(teleport, preferred_distance=teleport["maximum_distance"])
    side = -1 if player[0] < journal[0] else 1
    for direction in (side, -side):
        goal = (journal[0] + direction * JOURNAL_SIDE_OFFSET, journal[1] + JOURNAL_FLOOR_OFFSET)
        choice = path.choose(view, "north", destination=(goal[0] - player[0], goal[1] - player[1]))
        if (
            choice.aim is not None
            and math.hypot(choice.aim[0] - goal[0], choice.aim[1] - goal[1]) <= JOURNAL_GOAL_TOLERANCE
            and math.hypot(choice.aim[0] - journal[0], choice.aim[1] - journal[1]) <= JOURNAL_CLICK_RADIUS
            and direction * (choice.aim[0] - journal[0]) >= JOURNAL_MINIMUM_SIDE_CLEARANCE
        ):
            return choice.aim
    return None


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


@lru_cache(maxsize=1)
def _portal_hover_label():
    owner = Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
    rule = json.loads((owner / "journal-exit.json").read_text(encoding="utf-8"))["portal_hover"]
    path = owner / rule["path"]
    if hashlib.sha256(path.read_bytes()).hexdigest() != rule["sha256"]:
        raise ValueError("reviewed portal hover fingerprint required")
    return rule, cv.imread(str(path), cv.IMREAD_GRAYSCALE)


def red_portal(frame):
    """현재 월드의 유일한 붉은 타원 고리만 후보로 반환한다. 지역은 호출자가 확인한다."""
    if frame.shape != (1140, 1922, 3):
        raise ValueError("native portal frame required")
    patch = frame[55:900, 24:1900]
    hsv = cv.cvtColor(patch, cv.COLOR_BGR2HSV)
    mask = cv.inRange(hsv, (0, 95, 160), (12, 255, 255)) | cv.inRange(hsv, (170, 95, 160), (179, 255, 255))
    rule, template = _portal_hover_label()
    scores = cv.matchTemplate(cv.cvtColor(patch, cv.COLOR_BGR2GRAY), template, cv.TM_CCOEFF_NORMED)
    height, width = template.shape
    labels = []
    for _ in range(3):
        _, score, _, (x, y) = cv.minMaxLoc(scores)
        if score < rule["threshold"]:
            break
        labels.append((x, y))
        scores[max(0, y - height) : y + height + 1, max(0, x - width) : x + width + 1] = -1
    if len(labels) > 1:
        return None
    if labels:
        x, y = labels[0]

        def crop(source, box):
            a, b, c, d = box
            return source[max(0, y + b) : y + d, max(0, x + a) : x + c]

        red = crop(mask, rule["red_support_box"])
        core = crop(hsv, rule["dark_core_box"])
        # 호버 때 고리가 희게 변해도 현재 목적지 글자·붉은 실물·어두운 내부를 함께 요구한다.
        if (
            np.count_nonzero(red) >= rule["minimum_red_pixels"]
            and core.size
            and np.mean(core[:, :, 2] < 150) >= rule["minimum_dark_fraction"]
        ):
            return (x + width // 2 + 24, y + height // 2 + 55)
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
    last_frame_index = None
    pending_click = None
    canyon_steps = CanyonApproachObserver()
    journal = HorazonJournalObserver()
    deadline = min(clock() + 30, buff_expires_at)

    def guard():
        if stop.is_set() or not focused():
            raise RuntimeError("canyon finish interrupted or unfocused")
        if clock() >= deadline:
            raise RuntimeError("canyon finish deadline reached")

    def click_world(point, acquired):
        nonlocal pending_click
        if not 24 <= point[0] < 1900 or not 55 <= point[1] < 900:
            raise RuntimeError("canyon finish target outside world")
        guard()
        aim = frame_to_desktop(capture, point)
        if pending_click is None or max(abs(a - b) for a, b in zip(pending_click["aim"], aim)) > 3:
            backend.move(*aim)
            pending_click = dict(aim=aim, at=clock())
            return False
        guard()
        if not 0 <= clock() - acquired < 0.25 or not backend.pointer_matches(aim):
            raise RuntimeError("canyon finish screen or pointer stale")
        # OS 커서만 옮긴 같은 순간에 클릭하지 않고 다음 현재 화면의 같은 목표를 다시 확인한다.
        if clock() - pending_click["at"] < 0.05:
            return False
        backend.click()
        pending_click = None
        return True

    while True:
        guard()
        acquired = clock()
        frame = capture.grab().frame
        # 시간 초과 때도 실제 마지막 화면 하나만 남겨 포탈 미검출과 지역 미확인을 구분한다.
        if last_frame_index is None:
            last_frame_index = len(evidence)
            evidence.append(("journal-exit-last.png", frame))
        else:
            evidence[last_frame_index] = ("journal-exit-last.png", frame)
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
            if click_world(point, acquired):
                actions.append(dict(reason="Approach current Canyon waypoint stairs", point=point, acquired_at=acquired))
                evidence.append(("canyon-approach-{:02d}.png".format(canyon_approaches), frame))
                canyon_approaches += 1
                stop.wait(0.4)
            else:
                stop.wait(0.04)
            continue
        if town_vision.find(gray, "save_exit") or town_vision.find(gray, "waypoint_header"):
            raise RuntimeError("unexpected panel during journal exit")
        point, reason = None, None
        next_phase = phase
        arcane = town_vision.find(gray, "area_arcane")
        portal = red_portal(frame) if phase in ("journal", "portal") and arcane is not None else None
        if portal is not None:
            # 같은 방 재개 때 이미 열린 현재 포탈은 일지를 다시 누르지 않고 소비한다.
            point, reason, next_phase = portal, "Enter current red portal to Canyon of the Magi", "arrival"
        elif phase == "journal" and arcane is not None:
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
                if math.hypot(point[0] - player[0], point[1] - player[1]) > JOURNAL_CLICK_RADIUS:
                    if journal_approaches >= 2:
                        raise RuntimeError("journal approach limit reached")
                    view = path.observe(
                        frame, sequence=len(actions), generation=0, captured_at=acquired, viewport=(120, 180, 1720, 890), anchor=player
                    )
                    landing = journal_landing(path, view, point, player)
                    if landing is None:
                        raise RuntimeError("journal approach floor unverified")
                    aim = frame_to_desktop(capture, landing)
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
                        actions.append(dict(reason="Teleport near current Horazon journal", point=landing, acquired_at=acquired))
                        evidence.append(("journal-approach-{:02d}.png".format(journal_approaches), frame))
                        emit("SUMMONER_FINISH: Teleport near current Horazon journal")
                        stop.wait(reference_seconds)
                    continue
                reason, next_phase = "Click current Horazon journal", "dismiss"
        elif phase == "dismiss":
            ground = planner[0]
            prepare = getattr(ground, "prepare_frame", None)
            if prepare is not None:
                prepare(frame, acquired, reading, focused())
            player = ground.observe(frame, captured_at=acquired).point
            if player is not None:
                point, reason, next_phase = (player[0] - 45, player[1] + 30), "Click nearby floor immediately after journal", "portal"
        if not 0 <= clock() - acquired < 0.25:
            raise RuntimeError("canyon finish screen stale")
        if point is not None and click_world(point, acquired):
            phase = next_phase
            actions.append(dict(reason=reason, point=point, acquired_at=acquired))
            evidence.append(("journal-exit-{:02d}.png".format(len(actions)), frame))
            emit("SUMMONER_FINISH: " + reason)
        stop.wait(0.04)
