from __future__ import annotations

import heapq
import math
from dataclasses import dataclass

import cv2 as cv
import numpy as np

from diablo2.common.color_palette import palette_distance_map_bgr
from diablo2.common.encounter_vision import _palette
from diablo2.common.survival import _integer, _number
from diablo2.common.scene_motion import BackgroundMotionObserver, CameraMotion
from diablo2.runs.summoner.routes.common.arcane_palette import (
    ARCANE_FLOOR_GRAY_PALETTE_BGR,
    ARCANE_FLOOR_MAX_DISTANCE,
    ARCANE_STAR_VOID_MAX_DISTANCE,
    ARCANE_STAR_VOID_PALETTE_BGR,
)


HEADINGS = {"north": (1, -0.5), "east": (1, 0.5), "south": (-1, 0.5), "west": (-1, -0.5)}


def _distance(left, right):
    return math.hypot(left[0] - right[0], left[1] - right[1])


@dataclass(frozen=True)
class PathSettings:
    cell_pixels: int
    floor_fraction: float
    seed_radius: float
    maximum_step: float
    turn_degrees: float
    revisit_radius: float
    minimum_component_cells: int

    def __post_init__(self):
        _integer(self.cell_pixels, "cell_pixels", 2, 64)
        _integer(self.minimum_component_cells, "minimum_component_cells", 2)
        for name in ("floor_fraction", "seed_radius", "maximum_step", "turn_degrees", "revisit_radius"):
            if _number(getattr(self, name), name) == 0:
                raise ValueError(f"{name} must be positive")
        if self.floor_fraction > 1 or self.turn_degrees >= 180:
            raise ValueError("fraction/turn outside supported range")


@dataclass(frozen=True)
class PathView:
    sequence: int
    generation: int
    captured_at: float
    frame_size: tuple[int, int]
    viewport: tuple[int, int, int, int]
    anchor: tuple[float, float]
    cell_pixels: int
    floor: np.ndarray
    excluded_pixels: np.ndarray | None = None

    def point(self, cell):
        x, y = cell
        return (self.viewport[0] + (x + 0.5) * self.cell_pixels, self.viewport[1] + (y + 0.5) * self.cell_pixels)


@dataclass(frozen=True)
class PathChoice:
    aim: tuple[int, int] | None
    reason: str
    path: tuple[tuple[float, float], ...] = ()


class ArcanePathObserver:
    """팔레트 후보의 현재 연결을 탐색하며 텔레포트 가능/주변 안전을 확정하지 않는다."""

    def __init__(self, settings: PathSettings, *, terrain=None, teleport=None, automap_overlay=None):
        self.settings = settings
        self.automap_overlay = automap_overlay
        if automap_overlay is not None and (
            set(automap_overlay) != {"neutral_pixel_fraction"} or not 0 < automap_overlay["neutral_pixel_fraction"] <= 1
        ):
            raise ValueError("explicit reviewed automap neutral-pixel fraction required")
        self.teleport = None if teleport is None else dict(teleport)
        if self.teleport is not None:
            if (
                not {"maximum_distance", "clearance_pixels"}
                <= set(self.teleport)
                <= {"maximum_distance", "clearance_pixels", "preferred_distance"}
            ):
                raise ValueError("explicit teleport distance and floor clearance required")
            for name, value in self.teleport.items():
                if _number(value, name) == 0:
                    raise ValueError("positive teleport geometry required")
            if self.teleport["clearance_pixels"] >= self.teleport["maximum_distance"]:
                raise ValueError("teleport clearance must fit its distance bound")
            if self.teleport.get("preferred_distance", self.teleport["maximum_distance"]) > self.teleport["maximum_distance"]:
                raise ValueError("preferred teleport distance must fit its maximum")
        if terrain is None:
            terrain = {
                "floor_bgr": [list(color) for color in ARCANE_FLOOR_GRAY_PALETTE_BGR],
                "void_bgr": [list(color) for color in ARCANE_STAR_VOID_PALETTE_BGR],
                "floor_lab_distance": ARCANE_FLOOR_MAX_DISTANCE,
                "void_lab_distance": ARCANE_STAR_VOID_MAX_DISTANCE,
            }
        self.floor_palette = _palette(terrain["floor_bgr"])
        self.void_palette = _palette(terrain["void_bgr"])
        self.floor_distance = _number(terrain["floor_lab_distance"], "floor_lab_distance")
        self.void_distance = _number(terrain["void_lab_distance"], "void_lab_distance")
        if min(self.floor_distance, self.void_distance) == 0:
            raise ValueError("positive terrain distances required")

    def observe(self, frame, *, sequence, generation, captured_at, viewport, anchor):
        _integer(sequence, "sequence")
        _integer(generation, "generation")
        _number(captured_at, "captured_at")
        if not isinstance(frame, np.ndarray) or frame.dtype != np.uint8 or frame.ndim != 3 or frame.shape[2] != 3:
            raise ValueError("native uint8 BGR frame required")
        x0, y0, x1, y1 = viewport
        if not 0 <= x0 < x1 <= frame.shape[1] or not 0 <= y0 < y1 <= frame.shape[0]:
            raise ValueError("viewport outside frame")
        if not x0 <= anchor[0] < x1 or not y0 <= anchor[1] < y1:
            raise ValueError("anchor outside world viewport")
        cell = self.settings.cell_pixels
        rows, columns = (y1 - y0) // cell, (x1 - x0) // cell
        if min(rows, columns) < 3:
            raise ValueError("viewport too small for path grid")
        # 격자당 네 표본으로 색 계산량을 제한하고 실제 화면의 경계 좌표는 유지한다.
        patch = cv.resize(frame[y0 : y0 + rows * cell, x0 : x0 + columns * cell], (columns * 2, rows * 2), interpolation=cv.INTER_AREA)
        # 현재 지역 manifest/명시적 보정값을 사용하고 다른 그래픽의 적합성을 추정하지 않는다.
        floor_distance = palette_distance_map_bgr(patch, self.floor_palette)
        void_distance = palette_distance_map_bgr(patch, self.void_palette)
        floor = (floor_distance <= self.floor_distance) & ((void_distance > self.void_distance) | (floor_distance < void_distance))
        excluded = None
        if self.automap_overlay is not None:
            # 검토한 remastered 지도는 완전 무채색으로 그려진다. 원본 표본에서
            # 지도 선을 제외하며 유색 바닥 위에 겹친 선은 바닥 근거를 유지한다.
            blue, green, red = cv.split(frame)
            excluded = (blue == green) & (green == red) & (blue >= 30)
            neutral = cv.resize(
                excluded[y0 : y0 + rows * cell, x0 : x0 + columns * cell].astype(np.float32),
                (columns * 2, rows * 2),
                interpolation=cv.INTER_AREA,
            )
            floor &= neutral < self.automap_overlay["neutral_pixel_fraction"]
            excluded.setflags(write=False)
        grid = floor.reshape(rows, 2, columns, 2).mean(axis=(1, 3)) >= self.settings.floor_fraction
        # 작은 벽 장식/별 조각을 먼저 걷어내고 발판의 좁은 텍스처 틈만 잇는다.
        kernel = np.ones((3, 3), np.uint8)
        grid = cv.morphologyEx(grid.astype(np.uint8), cv.MORPH_OPEN, kernel)
        grid = cv.morphologyEx(grid, cv.MORPH_CLOSE, kernel).astype(bool)
        grid.setflags(write=False)
        return PathView(sequence, generation, captured_at, (frame.shape[1], frame.shape[0]), viewport, anchor, cell, grid, excluded)

    def from_mask(self, mask, sequence, generation, captured_at, frame_size, viewport, anchor):
        cell = self.settings.cell_pixels
        rows, columns = mask.shape[0] // cell, mask.shape[1] // cell
        grid = mask[: rows * cell, : columns * cell].reshape(rows, cell, columns, cell).mean(axis=(1, 3))
        grid = np.asarray(grid >= self.settings.floor_fraction, dtype=np.bool_)
        grid.setflags(write=False)
        return PathView(sequence, generation, captured_at, frame_size, viewport, anchor, cell, grid)

    def choose(
        self,
        view: PathView,
        wing,
        *,
        position=(0.0, 0.0),
        visited=(),
        destination=None,
        excluded=(),
        allow_detour=False,
        heading=None,
        prefer_frontier=False,
    ):
        if wing not in HEADINGS:
            raise ValueError("known Arcane wing required")
        floor = view.floor.copy()
        for x0, y0, x1, y1 in excluded:
            xs = view.viewport[0] + (np.arange(floor.shape[1]) + 0.5) * view.cell_pixels
            ys = view.viewport[1] + (np.arange(floor.shape[0]) + 0.5) * view.cell_pixels
            floor[np.ix_((ys >= y0) & (ys < y1), (xs >= x0) & (xs < x1))] = False
        # 발밑 효과/캐릭터의 작은 회색 조각 대신 실제로 이어진 발판 후보에서 시작한다.
        count, labels, statistics, _ = cv.connectedComponentsWithStats(floor.astype(np.uint8), connectivity=4)
        admitted = [label for label in range(1, count) if statistics[label, cv.CC_STAT_AREA] >= self.settings.minimum_component_cells]
        floor &= np.isin(labels, admitted)
        if self.teleport is not None:
            return self._choose_teleport(view, floor, wing, position, visited, destination, allow_detour, heading, prefer_frontier)
        candidates = [(_distance(view.point((x, y)), view.anchor), (int(x), int(y))) for y, x in np.argwhere(floor)]
        if not candidates:
            return PathChoice(None, "floor_unobserved")
        seeds = [(distance / view.cell_pixels, point) for distance, point in candidates if distance <= self.settings.seed_radius]
        if not seeds:
            return PathChoice(None, "player_floor_unobserved")
        # 캐릭터/시체가 발밑을 가리므로 가장 가까운 한 조각에 묶이지 않는다.
        # 초기 공백은 명시적 seed 반경 안에서만 건너뛰고 이후는 관찰된 연결을 따른다.
        costs, parents, queue = {point: cost for cost, point in seeds}, {}, list(seeds)
        heapq.heapify(queue)
        height, width = floor.shape
        while queue:
            cost, current = heapq.heappop(queue)
            if cost != costs[current]:
                continue
            x, y = current
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)):
                nx, ny = x + dx, y + dy
                if not 0 <= nx < width or not 0 <= ny < height or not floor[ny, nx]:
                    continue
                if dx and dy and (not floor[y, nx] or not floor[ny, x]):
                    continue
                next_cost = cost + math.hypot(dx, dy)
                if next_cost < costs.get((nx, ny), float("inf")):
                    costs[(nx, ny)], parents[(nx, ny)] = next_cost, current
                    heapq.heappush(queue, (next_cost, (nx, ny)))
        hx, hy = HEADINGS[wing]
        heading_length = math.hypot(hx, hy)
        hx, hy = hx / heading_length, hy / heading_length

        def score(point):
            px, py = view.point(point)
            dx, dy = px - view.anchor[0], py - view.anchor[1]
            wx, wy = position[0] + dx, position[1] + dy
            if destination is not None:
                return -math.hypot(wx - destination[0], wy - destination[1]) - 0.05 * costs[point] * view.cell_pixels
            advance = dx * hx + dy * hy
            sideways = abs(dx * hy - dy * hx)
            repeated = any(math.hypot(wx - previous[0], wy - previous[1]) < self.settings.revisit_radius for previous in visited)
            return advance - 0.12 * sideways - (self.settings.maximum_step if repeated else 0)

        target = max(costs, key=score)
        tx, ty = view.point(target)
        if destination is None and (tx - view.anchor[0]) * hx + (ty - view.anchor[1]) * hy <= view.cell_pixels:
            return PathChoice(None, "no_observed_progress")
        cells = [target]
        while cells[-1] in parents:
            cells.append(parents[cells[-1]])
        cells.reverse()

        def visible(left, right):
            count = 2 * max(abs(left[0] - right[0]), abs(left[1] - right[1])) + 1
            xs = np.rint(np.linspace(left[0], right[0], count)).astype(int)
            ys = np.rint(np.linspace(left[1], right[1], count)).astype(int)
            return bool(floor[ys, xs].all())

        # 격자 경로의 계단 모양을 실제 굴곡으로 오인하지 않도록 직시 가능한 구간을 합친다.
        simplified, cursor = [cells[0]], 0
        while cursor < len(cells) - 1:
            following = next(index for index in range(len(cells) - 1, cursor, -1) if visible(cells[cursor], cells[index]))
            simplified.append(cells[following])
            cursor = following
        path = tuple(view.point(point) for point in simplified)
        travelled, aim, reason = _distance(view.anchor, path[0]), path[0], "connected_floor"
        if travelled >= self.settings.maximum_step:
            return PathChoice(None, "step_too_short", path)
        for index in range(1, len(path)):
            step = _distance(path[index], path[index - 1])
            remaining = self.settings.maximum_step - travelled
            if step > remaining:
                aim = tuple(left + (right - left) * remaining / step for left, right in zip(path[index - 1], path[index]))
                break
            travelled, aim = travelled + step, path[index]
            if index + 1 < len(path):
                before = np.asarray(path[index]) - path[index - 1]
                after = np.asarray(path[index + 1]) - path[index]
                cosine = float(before @ after / (np.linalg.norm(before) * np.linalg.norm(after)))
                if math.degrees(math.acos(max(-1.0, min(1.0, cosine)))) >= self.settings.turn_degrees:
                    reason = "observed_bend"
                    break
        if aim is None:
            return PathChoice(None, "step_too_short", path)
        return PathChoice(tuple(int(round(value)) for value in aim), reason, path)

    def _choose_teleport(self, view, floor, wing, position, visited, destination, allow_detour=False, heading=None, prefer_frontier=False):
        # 텔레포트는 공백을 건널 수 있다. 연결 성분의 벽면 꼬리/가장자리 대신
        # 현재 관찰된 발판 안쪽을 직접 조준하며 중간 점을 공백 위로 보간하지 않는다.
        padded = np.pad(floor.astype(np.uint8), 1, mode="constant")
        clearance = cv.distanceTransform(padded, cv.DIST_L2, 5)[1:-1, 1:-1] * view.cell_pixels
        hx, hy = HEADINGS[wing] if heading is None else heading
        length = math.hypot(hx, hy)
        if not math.isfinite(length) or length == 0:
            raise ValueError("finite nonzero steering direction required")
        hx, hy = hx / length, hy / length
        frontier_label = None
        if prefer_frontier and destination is None:
            _, labels = cv.connectedComponents(floor.astype(np.uint8), connectivity=4)
            seeds = [(_distance(view.point((int(x), int(y))), view.anchor), int(labels[y, x])) for y, x in np.argwhere(floor)]
            if seeds and min(seeds)[0] <= self.settings.seed_radius:
                nearest_label = min(seeds)[1]
                exits = []
                edge_cells = math.ceil(self.teleport["clearance_pixels"] / view.cell_pixels) + 1
                for y, x in np.argwhere(labels == nearest_label):
                    if min(x, y, floor.shape[1] - 1 - x, floor.shape[0] - 1 - y) > edge_cells:
                        continue
                    # 지도 선/벽 꼬리보다 실제 착지 폭이 보이는 출구를 우선한다.
                    if clearance[y, x] < self.teleport["clearance_pixels"]:
                        continue
                    px, py = view.point((int(x), int(y)))
                    dx, dy = px - view.anchor[0], py - view.anchor[1]
                    distance = math.hypot(dx, dy)
                    alignment = (dx * hx + dy * hy) / distance if distance else -1
                    if distance >= 200 and alignment > math.cos(math.radians(150)):
                        exits.append((alignment, distance, dx, dy))
                if exits:
                    _, distance, dx, dy = max(exits)
                    hx, hy = dx / distance, dy / distance
                    frontier_label = nearest_label
        safe = clearance >= self.teleport["clearance_pixels"]
        if frontier_label is not None:
            safe &= labels == frontier_label
        ys, xs = np.nonzero(safe)
        points = np.column_stack((view.viewport[0] + (xs + 0.5) * view.cell_pixels, view.viewport[1] + (ys + 0.5) * view.cell_pixels))
        vectors = points - view.anchor
        distances = np.linalg.norm(vectors, axis=1)
        eligible = (distances > view.cell_pixels) & (distances <= self.teleport["maximum_distance"])
        world = vectors + position
        if destination is None:
            # 후보별 Python 반복 대신 현재 화면의 작은 배열로 방문 지점을 비교한다.
            for previous in visited:
                eligible &= np.sum((world - previous) ** 2, axis=1) >= self.settings.revisit_radius**2
            advance = vectors[:, 0] * hx + vectors[:, 1] * hy
            sideways = np.abs(vectors[:, 0] * hy - vectors[:, 1] * hx)
            accepted = eligible & (advance > view.cell_pixels)
            scores = advance - (0.75 if heading is not None else 0.12) * sideways
            detours = (
                eligible & (advance <= view.cell_pixels) & (advance > distances * math.cos(math.radians(150)))
                if allow_detour
                else np.zeros(len(points), bool)
            )
        else:
            scores = -np.linalg.norm(world - destination, axis=1) - 0.05 * distances
            accepted, detours = eligible, np.zeros(len(points), bool)
        detouring = not accepted.any() and detours.any()
        if detouring:
            # 먼 반대쪽 모서리보다 충분히 넓게 이동하면서 전체 흐름을 잇는 계단을 고른다.
            minimum = min(self.teleport["maximum_distance"], max(200, self.teleport.get("preferred_distance", 240) * 0.5))
            accepted = detours & (distances >= minimum)
            if not accepted.any():
                accepted = detours
            scores = advance - 0.12 * sideways
        if not accepted.any():
            return PathChoice(None, "teleport_floor_unobserved")
        # 보이는 가까운 발판을 우선해 다음 굴곡을 자주 다시 본다.
        # 가까운 미방문 후보가 없을 때만 계단/공백 너머의 먼 발판을 검토한다.
        preferred = self.teleport.get("preferred_distance", self.teleport["maximum_distance"])
        nearby = accepted & (distances <= preferred)
        if nearby.any():
            index = int(np.argmax(np.where(nearby, scores, -np.inf)))
        else:
            # 공백을 건너야 할 때는 가장 가까운 미방문 내부에 내려 다음 화면을 다시 본다.
            indices = np.flatnonzero(accepted)
            index = int(indices[np.lexsort((-scores[indices], distances[indices]))[0]])
        point = tuple(float(value) for value in points[index])
        reason = (
            "observed_floor_frontier"
            if frontier_label is not None
            else ("observed_teleport_detour" if detouring else "observed_teleport_floor")
        )
        return PathChoice(tuple(int(round(value)) for value in point), reason, (point,))
