from __future__ import annotations

import math
from dataclasses import dataclass

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.navigation import HEADINGS, PathChoice


def _distance(left, right):
    return math.hypot(left[0] - right[0], left[1] - right[1])


def _axis(vector):
    length = math.hypot(vector[0], vector[1] * 2)
    if not length:
        return None
    return max(HEADINGS, key=lambda name: (vector[0] * HEADINGS[name][0] + vector[1] * 4 * HEADINGS[name][1]))


OPPOSITE = dict(north="south", south="north", east="west", west="east")


@dataclass
class PlaceReference:
    features: object
    anchor: tuple
    position: tuple
    chart: int


class ArcaneRouteGraph:
    """관측한 착지 구역·진입 방향과 지형 특징을 기억한다. 색상만으로 물리 연결을 확정하지 않는다."""

    def __init__(self, *, maximum_references=12, maximum_places=96, place_radius=120, reference_spacing=300):
        if not 2 <= maximum_references <= 32 or not 4 <= maximum_places <= 256 or min(place_radius, reference_spacing) <= 0:
            raise ValueError("bounded route memory geometry required")
        self.maximum_references, self.maximum_places = maximum_references, maximum_places
        self.place_radius, self.reference_spacing = place_radius, reference_spacing
        self.references, self.places, self.edges, self.blocked = [], [], set(), []
        self.position, self.localized, self.current_place = (0.0, 0.0), False, None
        self.started, self.last_seen, self.reference_cursor = False, float("-inf"), 0
        self.entry, self.kind, self.reason = None, "unobserved", "unobserved"
        self.ports, self.relocalizations = {}, 0
        self.traversed, self.pending_exit = {}, None
        self.exit_intent = None
        self.pending_retreat = None
        self.retreated_branches = set()
        self.failed_exits = {}
        self.chart, self.place_charts, self.missing_since = 0, [], None
        self.matcher = cv.BFMatcher(cv.NORM_HAMMING)

    def _relocate(self, features, anchor):
        if features is None or features.descriptors is None or len(features.descriptors) < 24:
            return None
        # 최근 두 장과 순환하는 두 장만 비교한다. 원본 영상/무제한 검색 큐는 없다.
        count = len(self.references)
        indices = list(range(max(0, count - 2), count))
        if count:
            indices += [(self.reference_cursor + offset) % count for offset in range(min(2, count))]
            self.reference_cursor = (self.reference_cursor + 2) % count
        matches_found = []
        for index in dict.fromkeys(indices):
            reference = self.references[index]
            source = reference.features
            if source.frame_size != features.frame_size or source.descriptors is None:
                continue
            pairs = self.matcher.knnMatch(source.descriptors, features.descriptors, k=2)
            pairs = [pair[0] for pair in pairs if len(pair) == 2 and pair[0].distance < 0.7 * pair[1].distance]
            if len(pairs) < 24:
                continue
            left = np.float32([source.points[pair.queryIdx] for pair in pairs])
            right = np.float32([features.points[pair.trainIdx] for pair in pairs])
            transform, mask = cv.estimateAffinePartial2D(left, right, method=cv.RANSAC, ransacReprojThreshold=3)
            if transform is None or mask is None:
                continue
            mask = mask.ravel().astype(bool)
            inliers = int(mask.sum())
            if inliers < 24 or inliers / len(pairs) < 0.7 or np.max(np.abs(transform[:, :2] - np.eye(2))) > 0.03:
                continue
            error = np.linalg.norm(left @ transform[:, :2].T + transform[:, 2] - right, axis=1)
            if np.median(error[mask]) > 3:
                continue
            position = tuple(
                float(world + current - old - translation)
                for world, current, old, translation in zip(reference.position, anchor, reference.anchor, transform[:, 2])
            )
            matches_found.append((inliers, position, reference.chart))
        if not matches_found:
            return None
        best = max(matches_found, key=lambda item: item[0])
        # 반복 타일이 서로 다른 장소를 동시에 지지하면 위치를 확정하지 않는다.
        if any(chart == best[2] and _distance(position, best[1]) > self.place_radius for _, position, chart in matches_found):
            return None
        return best[2], best[1]

    def observe(self, features, anchor, displacement, captured_at):
        if captured_at <= self.last_seen:
            return False
        self.last_seen = captured_at
        previous, old_place, old_chart = self.position, self.current_place, self.chart
        relocated, new_chart = False, False
        if not self.started:
            self.started, self.localized = True, True
        elif displacement is not None:
            self.position = tuple(float(value + delta) for value, delta in zip(self.position, displacement))
            self.localized = True
        else:
            found = self._relocate(features, anchor)
            self.localized = found is not None
            if found is not None:
                self.chart, self.position = found
                relocated = True
                self.relocalizations += 1
        if not self.localized:
            self.missing_since = captured_at if self.missing_since is None else self.missing_since
            if captured_at - self.missing_since < 0.64:
                self.reason = "place_unlocalized_memory_retained"
                return False
            # 겹침이 없는 이동은 별도 국소 좌표계로 남긴다. 이전 장소/특징은 폐기하지 않는다.
            self.chart = max(self.place_charts, default=-1) + 1
            self.position, self.localized, new_chart, self.entry = (0.0, 0.0), True, True, None
        self.missing_since = None
        distances = [
            _distance(self.position, point) if chart == self.chart else float("inf") for point, chart in zip(self.places, self.place_charts)
        ]
        if distances and min(distances) <= self.place_radius:
            self.current_place = int(np.argmin(distances))
        elif len(self.places) < self.maximum_places:
            self.current_place = len(self.places)
            self.places.append(self.position)
            self.place_charts.append(self.chart)
        else:
            self.current_place = None
        moved = displacement is not None and old_chart == self.chart and _distance(previous, self.position) >= 64
        if moved:
            direction = _axis(tuple(new - old for new, old in zip(self.position, previous)))
            if self.pending_retreat == (old_chart, old_place, direction):
                if self.current_place is not None and self.current_place != old_place:
                    self.retreated_branches.add((self.chart, self.current_place, OPPOSITE[direction]))
            self.pending_retreat = None
            self.entry = OPPOSITE[direction]
            self.pending_exit = None
            if old_place is not None and self.current_place is not None and old_place != self.current_place:
                self.edges.add((old_place, self.current_place))
                key = (old_chart, old_place, direction)
                self.traversed[key] = self.traversed.get(key, 0) + 1
        elif relocated:
            # 같은 장소의 잠깐 가림은 확인된 진입로를 지우지 않는다.
            # 다른 장소/좌표계의 재획득으로 새 진입 방향을 만들지는 않는다.
            if old_chart != self.chart or old_place != self.current_place:
                self.entry = None
                self.pending_exit = None
                self.pending_retreat = None
        if features is not None and features.descriptors is not None:
            nearby = [item for item in self.references if item.chart == self.chart]
            if not nearby or min(_distance(self.position, item.position) for item in nearby) >= self.reference_spacing:
                self.references.append(PlaceReference(features, tuple(anchor), self.position, self.chart))
                self.references = self.references[-self.maximum_references :]
        self.reason = "new_local_chart" if new_chart else ("place_relocalized" if relocated else "place_observed")
        return relocated or new_chart

    def fail_landing(self, aim, anchor):
        self.pending_exit = None
        self.exit_intent = None
        self.pending_retreat = None
        if self.localized and aim is not None:
            direction = _axis(tuple(target - player for target, player in zip(aim, anchor)))
            if self.current_place is not None and direction is not None:
                key = (self.chart, self.current_place, direction)
                self.failed_exits[key] = self.failed_exits.get(key, 0) + 1
            self.blocked.append((self.chart, tuple(world + target - player for world, target, player in zip(self.position, aim, anchor))))
            self.blocked = self.blocked[-48:]

    def choose(self, path, view, wing, now, *, excluded=(), landings=(), hub=None):
        if not 0 <= now - view.captured_at < 0.25:
            return PathChoice(None, "graph_frame_stale")
        if hub is not None and _distance(hub, view.anchor) <= 400:
            # 실물 중앙에서는 방문 회피보다 지정 날개가 우선이다. 순환해 돌아와도 다른 날개를 탐색하지 않는다.
            choice = path.choose(view, wing, heading=HEADINGS[wing], excluded=excluded)
            self.entry, self.pending_exit = None, None
            self.exit_intent = None
            self.pending_retreat = None
            self.kind = "central_waypoint_departure"
            self.reason = "current_hub_departure_" + wing
            self.ports = {} if choice.aim is None else {wing: choice.aim}
            if (
                choice.aim is None
                or _axis(tuple(a - b for a, b in zip(choice.aim, view.anchor))) != wing
                or _distance(choice.aim, view.anchor) < path.teleport["clearance_pixels"] * 2
            ):
                return PathChoice(None, "hub_wing_exit_unobserved")
            return PathChoice(choice.aim, self.reason, choice.path)
        exclusions = list(excluded)
        if self.localized:
            for chart, point in self.blocked:
                if chart != self.chart:
                    continue
                x, y = (target - world + player for target, world, player in zip(point, self.position, view.anchor))
                exclusions.append((x - 64, y - 64, x + 64, y + 64))
        ports = {}
        for name, heading in HEADINGS.items():
            # 방문 회피는 아래 실제 출구 사용 횟수가 소유한다.
            # 같은 발판의 안쪽 후보까지 중복으로 제외하지 않는다.
            choice = path.choose(view, wing, position=self.position, heading=heading, excluded=tuple(exclusions))
            if choice.aim is None:
                continue
            vector = tuple(target - player for target, player in zip(choice.aim, view.anchor))
            if _axis(vector) != name or math.hypot(*vector) < path.teleport["clearance_pixels"] * 2:
                continue
            ports[name] = choice
        for landing in landings:
            vector = tuple(target - player for target, player in zip(landing.point, view.anchor))
            distance, name = math.hypot(*vector), _axis(vector)
            if name is None or not 64 <= distance <= path.teleport["maximum_distance"]:
                continue
            if any(left <= landing.point[0] < right and top <= landing.point[1] < bottom for left, top, right, bottom in exclusions):
                continue
            before = ports.get(name)
            if before is None or distance > _distance(before.aim, view.anchor):
                ports[name] = PathChoice(landing.point, landing.kind, (landing.point,))
        names = set(ports)
        if len(names) >= 3:
            self.kind = "branch_candidate"
        elif len(names) == 2:
            self.kind = "straight_candidate" if OPPOSITE[next(iter(names))] in names else "corner_candidate"
        else:
            self.kind = "single_landing_candidate" if names else "unobserved"
        self.ports = {name: choice.aim for name, choice in ports.items()}
        retreated = {name for name in ports if (self.chart, self.current_place, name) in self.retreated_branches}
        for name in retreated:
            ports.pop(name)
        if retreated and not ports:
            self.reason = "graph_retreat_branch_only"
            return PathChoice(None, self.reason)
        # 진입 방향은 조준 요청이 아니라 실제 배경 이동으로 확인한 방향이다.
        # 잘못 들어온 뒤에는 목표 방향 자체가 진입로의 역방향일 수 있다.
        # 진입 방향 회피가 전체 목표로 복귀하는 현재 출구까지 금지해서는 안 된다.
        avoid_entry = self.entry if self.entry != wing else None
        if avoid_entry in ports and len(ports) > 1:
            ports.pop(avoid_entry)
        elif avoid_entry in ports:
            key = (self.chart, self.current_place, avoid_entry)
            # 막다른 꺾임은 현재 보이는 진입 발판으로 한 번 되돌아갈 수 있다.
            # 같은 장소에서 이미 실제로 사용했거나 실패한 진입로를 무한 재시도하지 않는다.
            if self.traversed.get(key, 0) or self.failed_exits.get(key, 0):
                self.reason = "graph_entry_retreat_exhausted"
                return PathChoice(None, self.reason)
            self.pending_retreat = key
        if not ports:
            self.reason = "graph_no_unvisited_landing"
            return PathChoice(None, self.reason)
        # 막힌 끝의 짧은 앞쪽 조각이 실제로 이어지는 넓은 옆길을 계속 선점하지 않게 한다.
        # 좁은 꺾임에 짧은 후보만 보일 때는 그대로 보존하고 실제 이동으로 다시 확인한다.
        minimum_wide = min(path.teleport["maximum_distance"], max(200, path.teleport.get("preferred_distance", 240) * 0.5))
        wide = {name: choice for name, choice in ports.items() if _distance(choice.aim, view.anchor) >= minimum_wide}
        if wide:
            ports = wide
        # 현재 넓은 전진 출구가 실패하지 않았으면 방문 횟수만으로 정반대 후진을 고르지 않는다.
        # 전진이 안 보이거나 실제 실패한 국소 꺾임에서는 반대 방향도 보존한다.
        forward_failed = self.failed_exits.get((self.chart, self.current_place, wing), 0) if self.localized else 0
        if wing in ports and not forward_failed:
            ports.pop(OPPOSITE[wing], None)
        # 지형 대응이 끊겨도 조향 의도를 실제 진입 방향으로 바꾸지 않는다.
        # 같은 넓은 출구가 현재 이어지면 새 국소 좌표계의 방문 수만으로 즉시 역회전하지 않는다.
        # 현재 목표 방향·막힌 출구·실패 착지는 이 보완보다 우선한다.
        if self.exit_intent in ports:
            intent_failed = self.failed_exits.get((self.chart, self.current_place, self.exit_intent), 0) if self.localized else 0
            reverse = OPPOSITE[self.exit_intent]
            if reverse != wing and not intent_failed:
                ports.pop(reverse, None)
        # 같은 발판에서 실제로 나갔던 출구를 계속 순환하지 않는다. 목표 방향은
        # 같은 사용 횟수끼리의 우선순위이며, 시전 중에는 관측된 출구 의도를 유지한다.
        if self.pending_exit not in ports:
            self.pending_exit = None
        if self.pending_exit is None and self.localized and self.current_place is not None:
            # 같은 발판의 실패한 방향 안에서 조준만 바꾸기보다 현재 보이는 다른 출구를 먼저 시험한다.
            counts = {
                name: (
                    self.failed_exits.get((self.chart, self.current_place, name), 0),
                    self.traversed.get((self.chart, self.current_place, name), 0),
                )
                for name in ports
            }
            minimum_count = min(counts.values())
            ports = {name: choice for name, choice in ports.items() if counts[name] == minimum_count}
        elif self.pending_exit is not None:
            ports = {self.pending_exit: ports[self.pending_exit]}
        hx, hy = HEADINGS[wing]

        def score(item):
            name, choice = item
            dx, dy = (target - player for target, player in zip(choice.aim, view.anchor))
            forward = dx * hx + dy * hy * 4
            return (forward, math.hypot(dx, dy))

        name, choice = max(ports.items(), key=score)
        self.pending_exit = name
        self.exit_intent = name
        self.reason = "graph_observed_exit_" + name
        return PathChoice(choice.aim, self.reason, choice.path)

    def summary(self):
        return dict(
            position=self.position,
            chart=self.chart,
            coordinate_scope="local_chart",
            localized=self.localized,
            place=self.current_place,
            places=len(self.places),
            edges=len(self.edges),
            entry=self.entry,
            kind=self.kind,
            ports=self.ports,
            reason=self.reason,
            relocalizations=self.relocalizations,
            references=len(self.references),
            traversed_exits=len(self.traversed),
            failed_exits=len(self.failed_exits),
            pending_exit=self.pending_exit,
            exit_intent=self.exit_intent,
            pending_retreat=self.pending_retreat,
            retreated_branches=len(self.retreated_branches),
            feature_bytes=sum(item.features.points.nbytes + item.features.descriptors.nbytes for item in self.references),
        )
