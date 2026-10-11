from __future__ import annotations

import math

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.navigation import HEADINGS, PathChoice


def _angle(left, right):
    lengths = math.hypot(*left) * math.hypot(*right)
    if not lengths:
        return 0
    cosine = sum(a * b for a, b in zip(left, right)) / lengths
    return math.degrees(math.acos(max(-1, min(1, cosine))))


def floor_supports_aim(view, aim, clearance_pixels):
    x = int((aim[0] - view.viewport[0]) // view.cell_pixels)
    y = int((aim[1] - view.viewport[1]) // view.cell_pixels)
    clearance = cv.distanceTransform(np.pad(view.floor.astype(np.uint8), 1), cv.DIST_L2, 5)[1:-1, 1:-1]
    return 0 <= y < clearance.shape[0] and 0 <= x < clearance.shape[1] and clearance[y, x] * view.cell_pixels >= clearance_pixels


class ArcaneRouteFlow:
    """국소 우회 구간을 유지하다 현재 발판이 전체 진행 방향으로 다시 열리면 복귀한다."""

    def __init__(self, detour=None):
        self.detour = None if detour is None else self.axis(detour)
        self.forward_frames = 0
        self.observed_at = float("-inf")
        self.reason = "global_flow"

    @staticmethod
    def follows(direction, heading, degrees=40):
        return _angle((direction[0], direction[1] * 2), (heading[0], heading[1] * 2)) < degrees

    @staticmethod
    def axis(direction):
        # 발판 안쪽 조준점의 편차를 통로 자체가 기울어진 것으로 누적하지 않는다.
        return min(HEADINGS.values(), key=lambda heading: _angle((direction[0], direction[1] * 2), (heading[0], heading[1] * 2)))

    def choose(self, path, view, wing, global_choice, now, *, position=(0, 0), visited=(), excluded=()):
        if not 0 <= now - view.captured_at < 0.25:
            return PathChoice(None, "route_frame_stale")
        fresh = view.captured_at > self.observed_at
        if fresh:
            self.observed_at = view.captured_at
        heading = HEADINGS[wing]
        if self.detour == heading:
            self.detour = None
        direction = None if global_choice.aim is None else tuple(a - b for a, b in zip(global_choice.aim, view.anchor))
        # 화면 높이 축을 보정해 등각 지도의 전체 방향과 좌우 갈림을 구분한다.
        forward = direction is not None and self.follows(direction, heading, 20)
        minimum_forward = min(path.teleport["maximum_distance"], max(200, path.teleport.get("preferred_distance", 240) * 0.5))
        wide = direction is not None and math.hypot(*direction) >= minimum_forward
        if self.detour is None:
            if direction is None or self.axis(direction) != heading:
                direct = path.choose(view, wing, position=position, visited=visited, excluded=excluded)
                if direct.aim is not None:
                    vector = tuple(a - b for a, b in zip(direct.aim, view.anchor))
                    if math.hypot(*vector) >= minimum_forward and self.axis(vector) == heading:
                        self.reason = "global_flow"
                        return direct
            if direction is not None and not self.follows(direction, heading, 150):
                # 화면 밖으로 이어지는 진입 길이 반대쪽이면, 현재 보이는 직각 출구를 먼저 찾는다.
                dx, dy = heading
                exits = []
                for side in ((-dy * 2, dx / 2), (dy * 2, -dx / 2)):
                    candidate = path.choose(view, wing, position=position, visited=visited, heading=side, excluded=excluded)
                    if candidate.aim is None:
                        continue
                    vector = tuple(a - b for a, b in zip(candidate.aim, view.anchor))
                    if math.hypot(*vector) >= minimum_forward and self.follows(vector, side):
                        goal = sum(d * h for d, h in zip(vector, heading)) / math.hypot(*heading)
                        exits.append((goal, candidate, side))
                if exits:
                    _, candidate, self.detour = max(exits, key=lambda item: item[0])
                    self.reason = "observed_corner_turn"
                    return PathChoice(candidate.aim, self.reason, candidate.path)
                self.reason = "opposite_exit_without_observed_corner"
                return global_choice
            if wide and self.axis(direction) != heading:
                self.detour = self.axis(direction)
                self.reason = "bend_entered"
            else:
                self.reason = "global_flow"
            return global_choice
        if fresh:
            self.forward_frames = self.forward_frames + 1 if forward and wide else 0
        local = path.choose(view, wing, position=position, visited=visited, heading=self.detour, excluded=excluded)
        advance = 0
        if local.aim is not None:
            local_direction = tuple(a - b for a, b in zip(local.aim, view.anchor))
            if self.follows(local_direction, self.detour):
                advance = sum(d * h for d, h in zip(local_direction, self.detour)) / math.hypot(*self.detour)
        # 큰 발판 안쪽의 북쪽 후보만 보고 꺾임을 끝내지 않는다. 옆길 끝도 함께 확인한다.
        if self.forward_frames >= 2 and advance < minimum_forward:
            self.detour, self.forward_frames, self.reason = None, 0, "forward_reopened"
            return global_choice
        if local.aim is not None:
            if advance >= path.teleport["clearance_pixels"] * 2:
                self.reason = "follow_observed_bend"
                return local
        # 옆길 끝에서는 들어온 길의 역방향 대신 등각 지도의 두 직각 출구를 관찰한다.
        # 전체 목표와 반대 투영인 국소 8시 꺾임도 현재 발판 근거가 있으면 허용한다.
        exits, probes = [], []
        dx, dy = self.detour
        for next_heading in ((-dy * 2, dx / 2), (dy * 2, -dx / 2)):
            if forward and self.forward_frames < 2 and self.follows(next_heading, heading, 20):
                continue
            candidate = path.choose(view, wing, position=position, visited=visited, heading=next_heading, excluded=excluded)
            if candidate.aim is None:
                continue
            vector = tuple(a - b for a, b in zip(candidate.aim, view.anchor))
            along = sum(d * h for d, h in zip(vector, next_heading)) / math.hypot(*next_heading)
            if not self.follows(vector, next_heading):
                continue
            goal = sum(d * h for d, h in zip(vector, heading)) / math.hypot(*heading)
            if along >= minimum_forward:
                exits.append((goal, candidate, next_heading))
            elif along >= path.teleport["clearance_pixels"] * 2:
                probes.append((goal, candidate, next_heading))
        # 작은 계단 꼭짓점에서는 넓은 다음 발판이 아직 화면/HUD 뒤에 있을 수 있다.
        # 넓은 출구가 없을 때만 현재 내부 여유를 통과한 짧은 직각 발판을 재관찰 지점으로 쓴다.
        probing = not exits and bool(probes)
        if probing:
            exits = probes
        if exits:
            _, candidate, next_heading = max(exits, key=lambda item: item[0])
            self.detour, self.forward_frames, self.reason = next_heading, 0, "observed_corner_probe" if probing else "observed_corner_turn"
            return PathChoice(candidate.aim, self.reason, candidate.path)
        # 반대쪽으로 왕복하기보다 현재 꺾임의 이어지는 발판을 새 화면에서 다시 찾는다.
        self.reason = "bend_continuation_unobserved"
        return PathChoice(None, self.reason)


class ContinuousSteering:
    """전체 진행 방향과 독립적으로 현재 발판의 급회전만 빠르게 조향한다."""

    def __init__(self, reference_seconds, clearance_pixels, *, aim=None):
        self.reference_seconds = reference_seconds
        self.clearance_pixels = clearance_pixels
        self.aim = aim
        self.steered_at = float("-inf")
        self.observed_at = float("-inf")
        self.pending = None

    def update(self, view, choice, now):
        if choice.aim is None or view.captured_at <= self.observed_at:
            return False, "no_new_path"
        self.observed_at = view.captured_at
        direction = tuple(a - b for a, b in zip(choice.aim, view.anchor))
        reason = "cast_reference"
        changed = self.aim is None
        if self.aim is not None:
            supported = floor_supports_aim(view, self.aim, self.clearance_pixels)
            previous = tuple(a - b for a, b in zip(self.aim, view.anchor))
            turning = _angle(previous, direction) >= 35
            # 기존 조준이 공허/발판 가장자리로 바뀌면 시전 참고 시간보다 새 화면을 우선한다.
            if not supported:
                changed, reason = True, "previous_floor_closed"
            elif turning and self.pending is not None and _angle(self.pending, direction) < 20:
                changed, reason = True, "bend_confirmed_twice"
            elif now - self.steered_at >= self.reference_seconds:
                changed = True
            self.pending = direction if turning else None
        if changed:
            self.aim, self.steered_at, self.pending = choice.aim, now, None
        return changed, reason if changed else "hold_current_aim"
