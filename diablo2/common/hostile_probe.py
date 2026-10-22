import math

import cv2 as cv
import numpy as np


def nearby_actor_proposals(frame, player):
    """현재 근접 녹색 효과는 호버 후보만 제공한다. 적이나 안전의 판정이 아니다."""
    if player is None:
        return ()
    px, py = player
    x0, y0 = max(24, int(px - 300)), max(55, int(py - 180))
    x1, y1 = min(1900, int(px + 300)), min(900, int(py + 180))
    patch = frame[y0:y1, x0:x1]
    if not patch.size:
        return ()
    mask = cv.inRange(cv.cvtColor(patch, cv.COLOR_BGR2HSV), np.array((35, 120, 80)), np.array((90, 255, 255)))
    mask = cv.morphologyEx(mask, cv.MORPH_CLOSE, np.ones((5, 5), np.uint8))
    _, _, stats, centers = cv.connectedComponentsWithStats(mask)
    points = []
    for (_, _, width, height, area), center in zip(stats[1:], centers[1:]):
        x, y = center + (x0, y0)
        if (
            20 <= width <= 180
            and 5 <= height <= 100
            and area >= 40
            and math.hypot((x - px) / 300, (y - py) / 180) <= 1
            and math.hypot(x - px, y - py) >= 64
        ):
            points.append((int(round(x)), int(round(y - 80))))
    return tuple(sorted(points, key=lambda point: math.hypot(point[0] - px, point[1] + 80 - py))[:8])


class NearbyHostileProbe:
    """외형 미등록 적은 근처를 호버한 뒤 현재 적 체력 패널의 양성 확인으로만 승인한다."""

    OFFSETS = ((-80, -130), (80, -130), (0, -80), (-160, -80), (160, -80), (-240, -40), (240, -40), (-120, 0), (120, 0), (0, -160))
    PROPOSAL_TOLERANCE = 64

    def __init__(self, target_observer, backend, origin, *, clock, to_desktop=None):
        self.observer, self.backend, self.origin, self.clock = target_observer, backend, origin, clock
        self.to_desktop = to_desktop or (lambda point: tuple(int(a + b) for a, b in zip(self.origin, point)))
        self.pending, self.index = None, 0
        self.pending_ground = None
        self.next_proposal_at = float("-inf")

    def reset(self):
        self.pending = None
        self.pending_ground = None

    def observe(self, frame, player, acquired, *, points=None):
        now = self.clock()
        if player is None or not 0 <= now - acquired < 0.25:
            self.reset()
            return None
        if self.pending is not None:
            point, requested_at = self.pending
            absolute = self.to_desktop(point)
            # 실제 커서와 새 화면이 같은 근접 지점을 지지해야 한다. 이전 패널을 저장하지 않는다.
            if (
                0.05 <= acquired - requested_at <= 0.5
                and self.pending_ground is not None
                and math.hypot((self.pending_ground[0] - player[0]) / 300, (self.pending_ground[1] - player[1]) / 180) <= 1
                and (
                    points is None
                    or any(
                        math.hypot((x - player[0]) / 300, (y + 80 - player[1]) / 180) <= 1
                        and math.hypot(x - self.pending_ground[0], y + 80 - self.pending_ground[1]) <= self.PROPOSAL_TOLERANCE
                        for x, y in points
                    )
                )
                and self.backend.pointer_matches(absolute)
            ):
                target = self.observer.observe(frame)
                if target is not None and 0 <= self.clock() - acquired < 0.25:
                    self.pending = (point, acquired)
                    return target
            elif 0 <= acquired - requested_at < 0.05:
                return None
            self.reset()
        if points is not None:
            if not points or now < self.next_proposal_at:
                return None
            point = points[self.index % len(points)]
            # 후보는 바닥 효과에서 몸통 쪽으로 80px 올린 호버점이다. 근접 거리는 원래 바닥으로 잰다.
            ground = (point[0], point[1] + 80)
            self.next_proposal_at = now + 0.5
        else:
            dx, dy = self.OFFSETS[self.index % len(self.OFFSETS)]
            point = (int(player[0] + dx), int(player[1] + dy))
            ground = point
        self.index += 1
        if math.hypot((ground[0] - player[0]) / 300, (ground[1] - player[1]) / 180) > 1:
            return None
        if not 24 <= point[0] < frame.shape[1] - 24 or not 55 <= point[1] < 900:
            return None
        if not 0 <= self.clock() - acquired < 0.25:
            return None
        self.backend.move(*self.to_desktop(point))
        self.pending = (point, self.clock())
        self.pending_ground = ground
        return None
