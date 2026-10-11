from __future__ import annotations

from dataclasses import dataclass
from concurrent.futures import ThreadPoolExecutor
import math
import json
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.encounter_vision import RegionObserver
from diablo2.common.encounters import SceneContext
from diablo2.common.survival import ObservationStamp


@dataclass(frozen=True)
class HoverCandidate:
    kind: str
    evidence_box: tuple[int, int, int, int]
    score: float


@dataclass(frozen=True)
class HostileTarget:
    kind: str
    evidence_box: tuple[int, int, int, int]
    title_score: float | None


class ArcaneTargetObserver:
    """상단의 현재 적 이름/붉은 체력 패널 후보. 대상 위치나 사망은 추측하지 않는다."""

    def __init__(self):
        owner = Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        self.titles = []
        for kind, filename, box in (
            ("ghoul-lord", "templates/encounters/ghoul-name.png", (27, 9, 138, 35)),
            ("hell-clan", "templates/encounters/hell-clan-name.png", (25, 10, 148, 36)),
        ):
            image = cv.imread(str(owner / filename))
            left, top, right, bottom = box
            self.titles.append((kind, cv.cvtColor(image[top:bottom, left:right], cv.COLOR_BGR2GRAY)))

    def observe(self, frame):
        if not isinstance(frame, np.ndarray) or frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("native reviewed target-panel layout required")
        patch = frame[55:180, 700:1250]
        b, g, r = (channel.astype(np.int16) for channel in cv.split(patch))
        red = ((r >= 35) & (g <= 25) & (b <= 35) & (r > g * 2) & (r > b * 1.5)).astype(np.uint8)
        _, _, stats, _ = cv.connectedComponentsWithStats(red)
        panels = [(x, y, w, h) for x, y, w, h, area in stats[1:] if 60 <= w <= 450 and 20 <= h <= 60 and area >= 500]
        if len(panels) != 1:
            return None
        x, y, w, h = panels[0]
        panel = patch[y : y + h, x : x + w]
        if np.count_nonzero(panel.min(axis=2) >= 150) < 20:
            return None
        gray = cv.cvtColor(patch, cv.COLOR_BGR2GRAY)
        kind, title_score = "hostile-target", None
        for name, title in self.titles:
            _, score, _, location = cv.minMaxLoc(cv.matchTemplate(gray, title, cv.TM_CCOEFF_NORMED))
            if score >= 0.9 and x <= location[0] and y <= location[1] and location[0] + title.shape[1] <= x + w:
                kind, title_score = name, float(score)
                break
        return HostileTarget(kind, tuple(int(value) for value in (700 + x, 55 + y, 700 + x + w, 55 + y + h)), title_score)


class HubStabilityTracker:
    """문양으로 확인한 중앙이 정지한 동안만 현재 불꽃 쌍을 짧게 추적한다."""

    def __init__(self, observer, maximum_age=2.0):
        self.observer, self.maximum_age = observer, maximum_age
        self.reset()

    def reset(self):
        self.point, self.reference, self.observed_at = None, None, None

    def update(self, frame, point, evidence, acquired_at, displacement):
        if point is not None:
            self.point, self.observed_at = point, acquired_at
            self.reference = evidence if isinstance(evidence, dict) and evidence.get("kind") == "blue_pair_and_floor_ornaments" else None
            return point, evidence
        if self.reference is None:
            return None, None
        if not 0 <= acquired_at - self.observed_at <= self.maximum_age or displacement is None or math.hypot(*displacement) >= 2:
            self.reset()
            return None, None
        current = self.observer.confirm_static_pair(frame, self.point)
        if current is None:
            return None, None
        return current, dict(
            kind="recent_floor_and_current_blue_pair",
            point=current,
            reference_observed_at=self.observed_at,
            reference_parts=self.reference["parts"],
            current_floor_parts=[],
        )


class ArcaneHubObserver:
    """두 불꽃과 별개의 바닥 문양을 같은 native 위치에서 확인한다."""

    def __init__(self, manifest=None):
        path = (
            Path(manifest)
            if manifest is not None
            else (Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/hub-patterns.json")
        )
        self.rules = json.loads(path.read_text(encoding="utf-8"))
        if self.rules["schema"] != 1 or self.rules["frame_size"] != [1922, 1140] or self.rules["minimum_parts"] < 2:
            raise ValueError("reviewed native multi-part waypoint required")
        self.parts = []
        owner = path.parent.resolve()
        for rule in self.rules["parts"]:
            image_path = (path.parent / rule["path"]).resolve()
            if image_path.parent not in (owner, owner / "templates/hub") or not 0.8 <= rule["threshold"] <= 1:
                raise ValueError("waypoint part outside reviewed owner or score")
            image = cv.imread(str(image_path), cv.IMREAD_GRAYSCALE)
            if image is None or min(image.shape) < 20 or float(image.std()) < 5:
                raise ValueError("textured waypoint ornament required")
            self.parts.append((rule, image))
        self.last_evidence = None
        self.last_frame_id, self.last_pair_points = None, ()

    def observe(self, frame):
        if frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("native reviewed waypoint frame required")
        self.last_evidence = None
        self.last_frame_id, self.last_pair_points = id(frame), []
        pair = self.rules["blue_pair"]
        viewport = frame[55:900, 24:1900]
        mask = cv.inRange(cv.cvtColor(viewport, cv.COLOR_BGR2HSV), np.array(pair["hsv_lower"]), np.array(pair["hsv_upper"]))
        mask = cv.morphologyEx(mask, cv.MORPH_CLOSE, np.ones((3, 3), np.uint8))
        _, _, stats, centers = cv.connectedComponentsWithStats(mask)
        flames = [
            center + (24, 55)
            for (_, _, w, h, area), center in zip(stats[1:], centers[1:])
            if area >= pair["minimum_area"] and pair["width"][0] <= w <= pair["width"][1] and pair["height"][0] <= h <= pair["height"][1]
        ]
        gray = cv.cvtColor(frame, cv.COLOR_BGR2GRAY)
        found = []
        for index, left in enumerate(flames if len(flames) <= 24 else ()):
            for right in flames[index + 1 :]:
                if (
                    not pair["separation"][0] <= abs(left[0] - right[0]) <= pair["separation"][1]
                    or abs(left[1] - right[1]) > pair["vertical_difference"]
                ):
                    continue
                candidate = (left + right) / 2 + pair["ground_offset"]
                self.last_pair_points.append(tuple(float(value) for value in candidate))
                matched = self._match_floor(gray, candidate)
                if matched is not None:
                    found.append(matched)
        blue_evidence = "current_components"
        if not found:
            found = self._floor_anchored_braziers(gray, mask)
            blue_evidence = "floor_anchored_current_pixels"
        if len(found) != 1:
            return None
        point, matches = found[0]
        self.last_evidence = dict(
            kind="blue_pair_and_floor_ornaments",
            point=point,
            parts=[dict(id=item[2], score=item[1]) for item in matches],
            blue_evidence=blue_evidence,
        )
        return point

    def _match_floor(self, gray, candidate):
        matches, tolerance = [], self.rules["location_tolerance"]
        for rule, template in self.parts:
            x, y = (int(round(value)) for value in np.array(candidate) + rule["offset"])
            h, w = template.shape
            x0, y0 = max(24, x - tolerance), max(55, y - tolerance)
            patch = gray[y0 : min(900, y + h + tolerance + 1), x0 : min(1900, x + w + tolerance + 1)]
            if patch.shape[0] < h or patch.shape[1] < w:
                continue
            _, score, _, (dx, dy) = cv.minMaxLoc(cv.matchTemplate(patch, template, cv.TM_CCOEFF_NORMED))
            if score >= rule["threshold"]:
                point = (x0 + dx - rule["offset"][0], y0 + dy - rule["offset"][1])
                matches.append((point, float(score), rule["id"]))
        if len(matches) < self.rules["minimum_parts"]:
            return None
        center = tuple(int(round(v)) for v in np.median([item[0] for item in matches], axis=0))
        if all(math.hypot(*(a - b for a, b in zip(item[0], center))) <= tolerance for item in matches):
            return center, matches
        return None

    def _floor_anchored_braziers(self, gray, blue_mask):
        # 캐릭터 효과와 불꽃이 합쳐져도 현재 문양 두 개로 위치를 먼저 찾는다.
        # 축소 비교는 후보만 만들며 문양 native 점수와 양쪽 현재 청색 픽셀을 모두 요구한다.
        seed_id = self.rules.get("floor_seed_id")
        seed = next(((rule, image) for rule, image in self.parts if rule["id"] == seed_id), None)
        if seed is None:
            return []
        rule, image = seed
        coarse = cv.matchTemplate(
            cv.resize(gray[55:900, 24:1900], None, fx=0.5, fy=0.5, interpolation=cv.INTER_AREA),
            cv.resize(image, None, fx=0.5, fy=0.5, interpolation=cv.INTER_AREA),
            cv.TM_CCOEFF_NORMED,
        )
        found, pair = [], self.rules["blue_pair"]
        radius = pair["local_radius"]
        for _ in range(4):
            _, score, _, (x, y) = cv.minMaxLoc(coarse)
            if score < rule["threshold"]:
                break
            coarse[
                max(0, y - image.shape[0] // 2) : y + image.shape[0] // 2 + 1, max(0, x - image.shape[1] // 2) : x + image.shape[1] // 2 + 1
            ] = -1
            matched = self._match_floor(gray, (24 + x * 2 - rule["offset"][0], 55 + y * 2 - rule["offset"][1]))
            if matched is None or not any(item[2] == seed_id for item in matched[1]):
                continue
            center, _ = matched
            separation = sum(pair["separation"]) / 2
            counts = []
            for dx in (-separation / 2, separation / 2):
                bx, by = int(round(center[0] + dx - 24)), int(round(center[1] - pair["ground_offset"][1] - 55))
                patch = blue_mask[max(0, by - radius) : by + radius + 1, max(0, bx - radius) : bx + radius + 1]
                counts.append(cv.countNonZero(patch))
            if all(count >= pair["minimum_area"] for count in counts):
                found.append(matched)
                self.last_pair_points.append(tuple(float(value) for value in center))
        return found

    def confirm_static_pair(self, frame, reference):
        if id(frame) != self.last_frame_id:
            raise ValueError("same current hub observation required")
        candidates = [
            point
            for point in self.last_pair_points
            if math.hypot(*(a - b for a, b in zip(point, reference))) <= self.rules["location_tolerance"]
        ]
        return tuple(int(round(value)) for value in candidates[0]) if len(candidates) == 1 else None

    def current_pair_near(self, frame, player, radius=100):
        """현재 근처 불꽃 쌍은 문양 재획득 대기의 근거만 제공한다. 도착 근거가 아니다."""
        if id(frame) != self.last_frame_id:
            raise ValueError("same current hub observation required")
        candidates = [point for point in self.last_pair_points if math.hypot(*(a - b for a, b in zip(point, player))) <= radius]
        return tuple(int(round(value)) for value in candidates[0]) if len(candidates) == 1 else None


class ArcaneHoverObserver:
    """기존 호버 외형의 긍정 후보만 읽는다. 박스는 외형 근거이며 실제 물체 경계가 아니다."""

    def __init__(self, templates=None):
        if templates is None:
            root = Path(__file__).resolve().parents[3] / "assets"
            templates = [
                (
                    "chest",
                    cv.imread(str(root / "regions/arcane-sanctuary/remastered-ko-1922x1140/templates/hover/chest-name-ko.png")),
                ),
                (
                    "coffin",
                    cv.imread(str(root / "regions/arcane-sanctuary/remastered-ko-1922x1140/templates/hover/small-coffin-hover-ko.png")),
                ),
                ("shrine", cv.imread(str(root / "ui/world-labels/shrine-name-ko.png"))),
                (
                    "teleporter",
                    cv.imread(str(root / "regions/arcane-sanctuary/remastered-ko-1922x1140/templates/hover/teleporter-hover-ko.png")),
                ),
            ]
        self.templates = []
        for kind, template in templates:
            if kind not in ("chest", "coffin", "shrine", "teleporter"):
                raise ValueError("Arcane hover excludes town NPCs")
            if not isinstance(template, np.ndarray) or template.dtype != np.uint8 or template.ndim != 3 or template.shape[2] != 3:
                raise ValueError("native BGR hover reference required")
            self.templates.append((kind, cv.cvtColor(template, cv.COLOR_BGR2GRAY)))
        if not self.templates:
            raise ValueError("positive hover references required")

    def observe(self, frame, aim):
        if not isinstance(frame, np.ndarray) or frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("reviewed native Arcane frame required")
        x, y = aim
        if not 24 <= x < 1900 or not 55 <= y < 900:
            raise ValueError("hover aim must be inside world viewport")
        x0, y0, x1, y1 = max(24, x - 240), max(55, y - 350), min(1900, x + 240), min(900, y + 350)
        patch = cv.cvtColor(frame[y0:y1, x0:x1], cv.COLOR_BGR2GRAY)
        candidates = []
        for kind, template in self.templates:
            height, width = template.shape
            if height > patch.shape[0] or width > patch.shape[1]:
                continue
            _, score, _, (left, top) = cv.minMaxLoc(cv.matchTemplate(patch, template, cv.TM_CCOEFF_NORMED))
            if score >= 0.82:
                candidates.append(HoverCandidate(kind, (x0 + left, y0 + top, x0 + left + width, y0 + top + height), score))
        return tuple(candidates)


class ArcaneHazardObserver:
    """검토한 적 외형의 긍정 후보로 짧은 이동 시험을 멈춘다. 미검출은 clear 증거가 아니다."""

    def __init__(self):
        owner = Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/manifest.json"
        self.region = RegionObserver(owner)
        self.region.templates = [
            (rule, template) for rule, template in self.region.templates if rule["category"] == "monster" and rule["appearance"] == "alive"
        ]
        self.sequence = 0

    def observe(self, frame, ground, captured_at, now, *, aim=None):
        self.sequence += 1
        points = (ground,) if aim is None else (ground, aim)
        boxes = [(max(24, px - 240), max(55, py - 360), min(frame.shape[1] - 24, px + 240), min(900, py + 120)) for px, py in points]
        # 합친 큰 사각형보다 각 관심 영역을 병렬 비교하는 편이 현재 BGR 판독에서 빠르다.
        # 겹침의 중복 외형은 개체 수로 해석하지 않으며 같은 영역만 한 번으로 줄인다.
        boxes = list(dict.fromkeys(boxes))

        def scan(viewport):
            context = SceneContext(
                ObservationStamp("flash", "supervised-north", self.sequence, captured_at),
                self.region.region_id,
                self.sequence,
                self.region.frame_size,
                viewport,
            )
            x0, y0, x1, y1 = viewport
            return self.region.scan(frame[y0:y1, x0:x1], context, now=now, max_age=0.25, limit_per_template=1, workers=2)

        # 현재/착지 주변과 독립 템플릿만 병렬 처리한다. 반환 전에 모두 정리한다.
        with ThreadPoolExecutor(max_workers=len(boxes)) as executor:
            candidates = tuple(candidate for group in executor.map(scan, boxes) for candidate in group)
        return tuple(candidate for candidate in candidates if candidate.category == "monster" and candidate.appearance == "alive")


def cyan_ground_mask(frame):
    b, g, r = (channel.astype(np.int16) for channel in cv.split(frame))
    # 밝은 발판과 섞인 표식도 상대 채널 차이로 읽는다. 회색 바닥은 청색 우세가 없다.
    return ((b > 120) & (g > 90) & (b - r > 45) & (g - r > 25)).astype(np.uint8)


@dataclass(frozen=True)
class GroundReading:
    point: tuple[int, int] | None
    reason: str
    score: float | None = None
    calibration: dict | None = None


class FlashGroundObserver:
    """현재 발밑 표식과 검토한 ES/캐릭터 외형을 함께 찾는다. 지형/안전 증거는 별도다."""

    def __init__(
        self,
        marker,
        *,
        search_box,
        anchor_offset,
        minimum_score=0.88,
        variants=(),
        body_templates=(),
        body_box=None,
        body_minimum_score=0.82,
        tracking=None,
    ):
        if not isinstance(marker, np.ndarray) or marker.dtype != np.uint8 or marker.ndim != 3 or marker.shape[2] != 3:
            raise ValueError("reviewed native BGR ground-marker crop required")
        self.marker = cyan_ground_mask(marker)
        if np.count_nonzero(self.marker) < 20:
            raise ValueError("ground marker must contain positive cyan shape evidence")
        self.markers = [self.marker]
        for variant in variants:
            if not isinstance(variant, np.ndarray) or variant.dtype != np.uint8 or variant.shape != marker.shape:
                raise ValueError("reviewed ground variants must match the marker dimensions")
            mask = cyan_ground_mask(variant)
            if np.count_nonzero(mask) < 20:
                raise ValueError("ground variant must contain positive cyan shape evidence")
            self.markers.append(mask)
        if (
            not isinstance(search_box, tuple)
            or len(search_box) != 4
            or any(type(value) is not int for value in search_box)
            or not 0 <= search_box[0] < search_box[2] <= 1922
            or not 0 <= search_box[1] < search_box[3] <= 900
            or search_box[2] - search_box[0] < marker.shape[1]
            or search_box[3] - search_box[1] < marker.shape[0]
        ):
            raise ValueError("native world search rectangle required")
        if not isinstance(anchor_offset, tuple) or len(anchor_offset) != 2 or any(type(value) is not int for value in anchor_offset):
            raise ValueError("reviewed marker-relative ground anchor required")
        if not 0 <= anchor_offset[0] < marker.shape[1] or not 0 <= anchor_offset[1] < marker.shape[0]:
            raise ValueError("anchor must lie inside marker crop")
        if type(minimum_score) not in (int, float) or not 0 < minimum_score <= 1:
            raise ValueError("positive finite marker score required")
        self.search_box, self.anchor_offset, self.minimum_score = search_box, anchor_offset, minimum_score
        self.body_templates, self.body_box, self.body_minimum_score = tuple(body_templates), body_box, body_minimum_score
        self.tracking, self._track = tracking, None
        if tracking is not None:
            if set(tracking) != {"maximum_seconds", "maximum_gap_seconds", "maximum_displacement"} or any(
                type(value) not in (int, float) or not math.isfinite(value) or value <= 0 for value in tracking.values()
            ):
                raise ValueError("Explicit bounded ground identity tracking required")
        if self.body_templates:
            if body_box is None or len(body_box) != 4 or any(type(value) is not int for value in body_box):
                raise ValueError("Reviewed marker-relative body box required")
            left, top, right, bottom = body_box
            if not left < right or not top < bottom < 0 or not 0 < body_minimum_score <= 1:
                raise ValueError("Reviewed body geometry/score required")
            if any(
                not isinstance(body, np.ndarray) or body.dtype != np.uint8 or body.shape != (bottom - top, right - left, 3)
                for body in self.body_templates
            ):
                raise ValueError("Native reviewed body crops required")

    def _body_at(self, frame, point):
        if not self.body_templates:
            return False
        x, y = point
        left, top, right, bottom = self.body_box
        if not 0 <= x + left < x + right <= frame.shape[1] or not 0 <= y + top < y + bottom <= frame.shape[0]:
            return False
        patch = frame[y + top : y + bottom, x + left : x + right]
        return any(cv.matchTemplate(patch, body, cv.TM_CCOEFF_NORMED)[0, 0] >= self.body_minimum_score for body in self.body_templates)

    def _body_ground_candidates(self, frame):
        if not self.body_templates:
            return []
        l, t, r, b = self.search_box
        left, top, right, bottom = self.body_box
        found = []
        shield_points = []
        # 자동지도 표식이 없어도 현재 몸 외형과 ES가 같은 위치를 지지해야 한다.
        # 과거 좌표나 고정 화면 중심을 발밑으로 대입하지 않는다.
        shield_top = max(0, t - 180)
        shield_patch = frame[shield_top : b - 125, l - 25 : r + 25]
        blue, green, red = (channel.astype(np.int16) for channel in cv.split(shield_patch))
        gold = ((red > 65) & (green > 55) & (red > green * 1.04) & (green > blue * 1.2) & (red - blue > 25)).astype(np.uint8)
        gold = cv.morphologyEx(gold, cv.MORPH_OPEN, np.ones((3, 3), np.uint8))
        _, _, stats, centers = cv.connectedComponentsWithStats(gold)
        patches = []
        for (_, _, width, height, area), center in zip(stats[1:], centers[1:]):
            if not (15 <= width <= 40 and 15 <= height <= 40 and 0.7 <= width / height <= 1.4 and 120 <= area <= 1000):
                continue
            px, py = round(l - 25 + center[0]), round(shield_top + center[1] + 150)
            if not l <= px < r or not t <= py < b:
                continue
            shield_points.append((int(px), int(py)))
            x0, y0 = max(0, px + left - 12), max(0, py + top - 12)
            x1, y1 = min(frame.shape[1], px + right + 12), min(frame.shape[0], py + bottom + 12)
            patches.append((x0, y0, frame[y0:y1, x0:x1]))
        if len(patches) > 8:
            return []
        # 자세/오라의 세부 변화는 흐린 몸 외형으로 보완하되 ES와 유일 후보 조건은 유지한다.
        for blur in (None, 15):
            for x0, y0, patch in patches:
                current = patch if blur is None else cv.GaussianBlur(patch, (blur, blur), 0)
                for body in self.body_templates:
                    if patch.shape[0] < body.shape[0] or patch.shape[1] < body.shape[1]:
                        continue
                    template = body if blur is None else cv.GaussianBlur(body, (blur, blur), 0)
                    scores = cv.matchTemplate(current, template, cv.TM_CCOEFF_NORMED)
                    for _ in range(8):
                        _, score, _, (x, y) = cv.minMaxLoc(scores)
                        if score < self.body_minimum_score:
                            break
                        point = (x0 + x - left, y0 + y - top)
                        if l <= point[0] < r and t <= point[1] < b and self._energy_shield_above(frame, point):
                            found.append((point, score, "flash_body_and_es" if blur is None else "flash_body_shape_and_es"))
                        scores[max(0, y - 12) : y + 13, max(0, x - 12) : x + 13] = 0
            if found:
                break
        if not found:
            for point in shield_points:
                x, y = point
                if not self._energy_shield_above(frame, point):
                    continue
                body = frame[y + top : y + bottom, x + left : x + right].astype(np.int16)
                sides = np.concatenate(
                    (frame[y + top : y + bottom, x + left - 24 : x + left], frame[y + top : y + bottom, x + right : x + right + 24]),
                    axis=1,
                )
                if sides.shape[0] != body.shape[0] or sides.shape[1] != 48 or body.shape[:2] != (bottom - top, right - left):
                    continue
                background = np.median(sides, axis=1)[:, None, :]
                contrast = np.linalg.norm(body - background, axis=2)
                colored = (body.max(axis=2) - body.min(axis=2) > 12) & (body.max(axis=2) < 170)
                silhouette = colored & (contrast > 30)
                # ES 바로 아래에 주변과 구별되는 세로 몸 외형이 현재 보여야 한다.
                # 균일한 공허/바닥 색이나 ES만 있는 화면은 발 위치가 아니다.
                if np.count_nonzero(silhouette) >= 120 and np.count_nonzero(silhouette.sum(axis=1) >= 4) >= 32:
                    found.append((point, 0.0, "flash_es_and_body_shape"))
        if not found:
            return []
        best = max(found, key=lambda item: item[1])
        return [best] if all(math.hypot(p[0] - best[0][0], p[1] - best[0][1]) <= 12 for p, _, _ in found) else found

    @staticmethod
    def _energy_shield_above(frame, point):
        x, y = point
        patch = frame[max(0, y - 180) : max(0, y - 125), max(0, x - 25) : min(frame.shape[1], x + 26)]
        if patch.size == 0:
            return False
        b, g, r = (channel.astype(np.int16) for channel in cv.split(patch))
        gold = ((r > 65) & (g > 55) & (r > g * 1.04) & (g > b * 1.2) & (r - b > 25)).astype(np.uint8)
        # 현재 보정의 구체 크기/색 후보다. 다른 효과/다른 캐릭터에 확대하지 않는다.
        # 얇게 붙은 효과 꼬리만 제거한 후보도 검사하고 기존 구체 후보는 보존한다.
        for mask in (gold, cv.morphologyEx(gold, cv.MORPH_OPEN, np.ones((3, 3), np.uint8))):
            _, _, stats, centers = cv.connectedComponentsWithStats(mask)
            if any(
                15 <= w <= 40
                and 15 <= h <= 40
                and 0.7 <= w / h <= 1.4
                and 120 <= area <= 1000
                and area / (w * h) >= 0.3
                and left >= 2
                and top >= 2
                and left + w <= patch.shape[1] - 2
                and top + h <= patch.shape[0] - 2
                and abs(center[0] - 25) <= 8
                and 25 <= center[1] <= 42
                for (left, top, w, h, area), center in zip(stats[1:], centers[1:])
            ):
                return True
        return False

    def observe(self, frame, *, captured_at=None):
        if not isinstance(frame, np.ndarray) or frame.dtype != np.uint8 or frame.ndim != 3 or frame.shape[2] != 3:
            raise ValueError("native uint8 BGR frame required")
        if frame.shape[:2] != (1140, 1922):
            self._track = None
            return GroundReading(None, "layout_unverified")
        if captured_at is not None and (type(captured_at) not in (int, float) or not math.isfinite(captured_at) or captured_at < 0):
            raise ValueError("Finite ground capture time required")
        if self._track is not None and captured_at is not None and captured_at < self._track["at"]:
            self._track = None
            return GroundReading(None, "ground_time_reversed")
        l, t, r, b = self.search_box
        mask = cyan_ground_mask(frame[t:b, l:r])
        scores = np.maximum.reduce([cv.matchTemplate(mask, marker, cv.TM_CCORR_NORMED) for marker in self.markers])
        scores = np.nan_to_num(scores, nan=0.0, posinf=0.0, neginf=0.0)
        candidates, markers = [], []
        height, width = self.marker.shape
        for _ in range(32):
            _, score, _, location = cv.minMaxLoc(scores)
            if score < self.minimum_score:
                break
            x, y = location
            point = (l + x + self.anchor_offset[0], t + y + self.anchor_offset[1])
            markers.append((point, score))
            shield = self._energy_shield_above(frame, point)
            if shield or self._body_at(frame, point):
                candidates.append((point, score, "flash_marker_and_es" if shield else "flash_marker_and_body"))
            scores[max(0, y - height // 2) : y + height // 2 + 1, max(0, x - width // 2) : x + width // 2 + 1] = 0
        if cv.minMaxLoc(scores)[1] >= self.minimum_score:
            self._track = None
            return GroundReading(None, "ground_search_overflow")
        if not candidates:
            candidates = self._body_ground_candidates(frame)
        if len(candidates) == 1:
            point, score, reason = candidates[0]
            self._track = None if captured_at is None else dict(point=point, at=captured_at, strong_at=captured_at)
            return GroundReading(point, reason, None if reason == "flash_es_and_body_shape" else score)
        if not candidates and self.tracking is not None and self._track is not None and captured_at is not None:
            previous = self._track
            current = [
                (point, score)
                for point, score in markers
                if math.hypot(point[0] - previous["point"][0], point[1] - previous["point"][1]) <= self.tracking["maximum_displacement"]
            ]
            if (
                0 <= captured_at - previous["at"] <= self.tracking["maximum_gap_seconds"]
                and 0 <= captured_at - previous["strong_at"] <= self.tracking["maximum_seconds"]
                and len(current) == 1
            ):
                # 현재 프레임의 표식 외형을 직전 신원과 연결한다. 좌표 대체나 시한 갱신은 하지 않는다.
                point, score = current[0]
                self._track = dict(point=point, at=captured_at, strong_at=previous["strong_at"])
                return GroundReading(point, "flash_marker_tracked", score)
        self._track = None
        return GroundReading(None, "ground_ambiguous" if candidates else "ground_unverified")
