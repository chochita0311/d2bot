from __future__ import annotations

import itertools
import json
import math
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, replace
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.label_occlusion import label_occlusion_mask, visible_template_peak


@dataclass(frozen=True)
class TerminalReading:
    kind: str
    point: tuple
    evidence_boxes: tuple
    scores: tuple
    part_ids: tuple = ()
    provenance: str = "current_three_parts"
    current_parts: tuple = ()
    reference_observed_at: float = None
    visible_fractions: tuple = ()
    minimum_separation: float = 84
    minimum_area: float = 8000


class TerminalStabilityTracker:
    """정지한 배치는 도착에, 이동 후 현재 부위 대응은 접근에만 쓴다. 원본 시한은 연장하지 않는다."""

    def __init__(self, observer, maximum_age=2.0):
        self.observer, self.maximum_age = observer, maximum_age
        self.reference, self.observed_at = None, None
        self.approach, self.approach_only = None, False
        self.landmark_relocalized = False
        self.previous_current_full, self.approach_displacement = None, None
        self.previous_current_frame = None
        self.approach_static_pairs = 0

    def reset(self):
        self.reference, self.observed_at = None, None
        self.approach, self.approach_only = None, False
        self.landmark_relocalized = False
        self.previous_current_full, self.approach_displacement = None, None
        self.previous_current_frame = None
        self.approach_static_pairs = 0

    def require_relocalization(self):
        # 화면 표현이 바뀌면 과거 상자의 국소 좌표 대신 현재 두 부위의 공통 이동부터 확인한다.
        self.approach = None
        self.landmark_relocalized = True
        self.previous_current_full, self.approach_displacement = None, None
        self.previous_current_frame = None
        self.approach_static_pairs = 0

    def update(self, frame, reading, acquired_at, displacement):
        self.approach = None
        self.approach_displacement = None
        previous, previous_frame = self.previous_current_full, self.previous_current_frame
        self.previous_current_full = (
            (reading, acquired_at)
            if reading is not None
            and reading.provenance == "current_three_parts"
            and len(reading.part_ids) == len(reading.evidence_boxes) == 3
            and reading.current_parts == (True, True, True)
            else None
        )
        self.previous_current_frame = frame.copy() if self.previous_current_full is not None else None
        if previous is not None and self.previous_current_full is not None:
            before, before_at = previous
            if 0 < acquired_at - before_at < 0.25 and before.kind == reading.kind:
                if before.part_ids != reading.part_ids:
                    # 외형 ID가 바뀌어도 현재 세 외형이 직전 실제 화면에서 모두 확인돼야 같은 정지 대응이다.
                    before = self.observer.confirm_static_parts(previous_frame, reading)
                    if before is not None and (
                        len(before.visible_fractions) != 3 or any(value != 1.0 for value in before.visible_fractions)
                    ):
                        before = None
                if before is not None and all(before.current_parts):
                    shifts = [
                        (current[0] - old[0], current[1] - old[1]) for old, current in zip(before.evidence_boxes, reading.evidence_boxes)
                    ]
                    if all(math.hypot(*shift) < 2 for shift in shifts):
                        # 접근 준비만 보조한다. 실제 도착에는 별도 배경 정지가 계속 필요하다.
                        self.approach_displacement = tuple(sum(shift[axis] for shift in shifts) / 3 for axis in (0, 1))
        self.approach_static_pairs = self.approach_static_pairs + 1 if self.approach_displacement is not None else 0
        if reading is not None:
            self.reference = reading if len(reading.part_ids) == 3 else None
            self.observed_at = acquired_at
            self.approach_only = False
            self.landmark_relocalized = False
            return replace(reading, reference_observed_at=acquired_at)
        if self.reference is None:
            return None
        if not 0 <= acquired_at - self.observed_at <= self.maximum_age:
            self.reset()
            return None
        if displacement is None or self.landmark_relocalized:
            return self._relocalize(frame, displacement)
        if self.approach_only or math.hypot(*displacement) >= 2:
            previous, was_approach = self.reference, self.approach_only
            reading = self.observer.confirm_approach_parts(frame, self.reference, displacement)
            if reading is None:
                return self._relocalize(frame, displacement)
            self.reference = reading
            self.approach = replace(reading, reference_observed_at=self.observed_at)
            self.approach_only = True
            if was_approach and self._same_static_parts(previous, reading, displacement):
                return replace(self.approach, provenance="current_motion_settled_two_parts")
            return None
        reading = self.observer.confirm_static_parts(frame, self.reference)
        return None if reading is None else replace(reading, reference_observed_at=self.observed_at)

    def _relocalize(self, frame, displacement):
        # 재위치 자체는 접근 근거다. 같은 현재 두 부위와 별도 배경 정지가 이어져야 부분 도착에 쓴다.
        previous, was_relocalized = self.reference, self.landmark_relocalized
        self.landmark_relocalized, self.approach_only = True, True
        reading = self.observer.confirm_relocalized_parts(frame, self.reference)
        if reading is not None:
            self.reference = reading
            self.approach = replace(reading, reference_observed_at=self.observed_at)
            if was_relocalized and self._same_static_parts(previous, reading, displacement):
                return replace(self.approach, provenance="current_relocalized_two_parts")
        return None

    @staticmethod
    def _same_static_parts(previous, reading, displacement):
        return (
            displacement is not None
            and math.hypot(*displacement) < 2
            and sum(reading.current_parts) >= 2
            and previous.current_parts == reading.current_parts
            and all(
                math.hypot(current[0] - before[0], current[1] - before[1]) < 2
                for before, current, present in zip(previous.evidence_boxes, reading.evidence_boxes, reading.current_parts)
                if present
            )
        )


# 아래20px는 HUD 생명/마나 검색 띠 사이만 관찰한다. 입력 가능한 월드 범위는 바꾸지 않는다.
TERMINAL_OBSERVATION_BOTTOM = 920
TERMINAL_LOWER_STRIP = (650, 1290)


def _landmark_box_visible(box):
    left, top, right, bottom = box
    return (
        24 <= left < right <= 1900
        and 55 <= top < bottom <= TERMINAL_OBSERVATION_BOTTOM
        and (bottom <= 900 or TERMINAL_LOWER_STRIP[0] <= left < right <= TERMINAL_LOWER_STRIP[1])
    )


def _terminal_image(frame, encoding):
    if encoding == "gray":
        return cv.cvtColor(frame, cv.COLOR_BGR2GRAY)
    if encoding == "soft_gold":
        blue, green, red = cv.split(frame)
        gold = cv.min(cv.subtract(red, green), cv.subtract(green, blue))
        return cv.GaussianBlur(cv.convertScaleAbs(gold, alpha=5), (3, 3), 0.8)
    raise ValueError("reviewed terminal encoding required")


class ArcaneTerminalObserver:
    """검토한 세 근거의 종착 배치를 찾는다. 적 미검출이나 일반 바닥을 빈 끝으로 쓰지 않는다."""

    def __init__(self, manifest=None):
        manifest = (
            Path(manifest)
            if manifest is not None
            else (Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/terminal-patterns.json")
        )
        data = json.loads(manifest.read_text(encoding="utf-8"))
        if data["schema"] != 1 or data["frame_size"] != [1922, 1140] or len(data["parts"]) != 3:
            raise ValueError("reviewed three-part terminal layout required")
        self.parts = self._load_parts(manifest, data["parts"])
        self.alternate_layouts = []
        for layout in data.get("alternate_layouts", []):
            if layout["kind"] != "nonboss_chest_platform" or len(layout["parts"]) != 3:
                raise ValueError("three independent reviewed nonboss platform parts required")
            self.alternate_layouts.append((layout["kind"], self._load_parts(manifest, layout["parts"])))
        self.native_parts = {
            rule["path"]: (rule, native)
            for parts in [self.parts] + [parts for _, parts in self.alternate_layouts]
            for rule, native, _ in parts
        }
        self.chest_parts = []
        self.chest_detail_parts = []
        self.previous_full = None
        for path in data.get("chest_parts", []):
            if path not in self.native_parts:
                raise ValueError("reviewed chest appearance required")
            rule, native = self.native_parts[path]
            if rule.get("encoding", "gray") != "gray":
                raise ValueError("floor encoding cannot enter chest composition")
            self.chest_parts.append((rule, native, cv.resize(native, None, fx=0.5, fy=0.5)))
        for path in data.get("chest_detail_parts", []):
            if path not in self.native_parts or path in data.get("chest_parts", []):
                raise ValueError("separate reviewed chest detail required")
            rule, native = self.native_parts[path]
            if rule.get("encoding", "gray") != "gray":
                raise ValueError("floor encoding cannot enter chest composition")
            minimum = data.get("chest_detail_minimum_scores", {}).get(path, 0.92)
            if not 0.91 <= minimum <= 1 or rule["threshold"] < minimum:
                raise ValueError("chest detail native score below reviewed minimum")
            self.chest_detail_parts.append((rule, native, cv.resize(native, None, fx=0.5, fy=0.5)))

    @staticmethod
    def _load_parts(manifest, rules):
        parts = []
        owner = manifest.parent.resolve()
        for rule in rules:
            path = (manifest.parent / rule["path"]).resolve()
            if path.parent not in (owner, owner / "templates/terminal") or not 0.9 <= rule["threshold"] <= 1:
                raise ValueError("terminal part outside reviewed owner or score")
            encoding = rule.get("encoding", "gray")
            if encoding not in ("gray", "soft_gold"):
                raise ValueError("reviewed terminal encoding required")
            image = cv.imread(str(path), cv.IMREAD_GRAYSCALE if encoding == "gray" else cv.IMREAD_COLOR)
            if image is not None and encoding == "soft_gold":
                image = _terminal_image(image, encoding)
            if image is None or min(image.shape) < 20 or float(image.std()) < 5:
                raise ValueError("positive textured terminal part required")
            parts.append((rule, image, cv.resize(image, None, fx=0.5, fy=0.5)))
        return parts

    def _images(self, frame, part_ids=None):
        names = self.native_parts if part_ids is None else part_ids
        encodings = {"gray"} | {self.native_parts[name][0].get("encoding", "gray") for name in names}
        return {encoding: _terminal_image(frame, encoding) for encoding in encodings}

    def observe(self, frame):
        if frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("native reviewed terminal frame required")
        # 직전 배치의 세 부위가 지금도 모두 확인되면 같은 위치를 전역 재탐색하지 않는다.
        # 한 부위라도 사라지면 즉시 전체 탐색으로 돌아가며 과거 점수/시한을 재사용하지 않는다.
        if self.previous_full is not None:
            current = self.confirm_static_parts(frame, self.previous_full)
            if current is not None and all(current.current_parts):
                current = self._layout(
                    [
                        [(box, score, fraction)]
                        for box, score, fraction in zip(current.evidence_boxes, current.scores, current.visible_fractions)
                    ],
                    current.kind,
                    [(self.native_parts[name][0], None, None) for name in current.part_ids],
                    current.minimum_separation,
                    current.minimum_area,
                )
                if current is not None:
                    self.previous_full = current
                    return current
        self.previous_full = self._observe_full(frame)
        return self.previous_full

    def _observe_full(self, frame):
        images = self._images(frame[55:TERMINAL_OBSERVATION_BOTTOM, 24:1900])
        views = {encoding: (image, cv.resize(image, None, fx=0.5, fy=0.5)) for encoding, image in images.items()}
        gray, small = views["gray"]
        occluded = label_occlusion_mask(frame)[55:TERMINAL_OBSERVATION_BOTTOM, 24:1900]
        # 같은 프레임의 공용 외형만 재사용하고 다음 관찰에는 검출 결과를 넘기지 않는다.
        matches = {}
        groups = self._match_groups(gray, small, self.parts, matches, occluded, views)
        reading = None if groups is None else self._layout(groups, "nonboss_chest_trio", self.parts)
        if reading is not None:
            return reading
        # 양성의 첫 배치는 짧게 끝낸다. 나머지는 같은 현재 화면의 독립 외형만 병렬 비교한다.
        # 레이아웃 순서·점수·가림·물체 간격 판정과 프레임 사이 캐시 정책은 바꾸지 않는다.
        remaining = list(
            {
                (part[0]["path"], part[0]["threshold"]): part
                for parts in [self.parts] + [parts for _, parts in self.alternate_layouts]
                for part in parts
                if (part[0]["path"], part[0]["threshold"]) not in matches
            }.values()
        )
        with ThreadPoolExecutor(max_workers=3) as pool:
            results = pool.map(lambda part: self._match_part(*views[part[0].get("encoding", "gray")], part, occluded), remaining)
            for part, found in zip(remaining, results):
                matches[(part[0]["path"], part[0]["threshold"])] = found
        for kind, parts in self.alternate_layouts:
            groups = self._match_groups(gray, small, parts, matches, occluded, views)
            reading = None if groups is None else self._layout(groups, kind, parts)
            if reading is not None:
                return reading
        # 몸통 조합을 먼저 쓰고, 부분 외형은 더 넓은 물체 간격으로 중복 계수를 막는다.
        reading = self._composed_chests(gray, small, self.chest_parts, matches, occluded)
        if reading is not None or not self.chest_detail_parts:
            return reading
        return self._composed_chests(gray, small, self.chest_parts + self.chest_detail_parts, matches, occluded, 120, 16000)

    def _composed_chests(self, gray, small, parts, matches, occluded, minimum_separation=84, minimum_area=8000):
        candidates = []
        for part in parts:
            groups = self._match_groups(gray, small, (part,), matches, occluded)
            if groups is not None:
                for match in groups[0]:
                    if match[2] == 1.0:
                        candidates.append((match, part))
        readings = []
        for trio in itertools.combinations(candidates, 3):
            reading = self._layout(
                [[match] for match, _ in trio], "nonboss_chest_platform", [part for _, part in trio], minimum_separation, minimum_area
            )
            if reading is not None:
                readings.append(reading)
        return max(readings, key=lambda item: sum(item.scores)) if readings else None

    def confirm_static_parts(self, frame, reference):
        if frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("native reviewed terminal frame required")
        images = self._images(frame, reference.part_ids)
        scores, current, fractions, boxes = [], [], [], []
        occluded = label_occlusion_mask(frame)
        for name, box in zip(reference.part_ids, reference.evidence_boxes):
            rule, template = self.native_parts[name]
            gray = images[rule.get("encoding", "gray")]
            x0, y0, x1, y1 = box
            left, top = max(24, x0 - 3), max(55, y0 - 3)
            patch = gray[top : min(TERMINAL_OBSERVATION_BOTTOM, y1 + 3), left : min(1900, x1 + 3)]
            score, location = -1, (0, 0)
            if min(patch.shape) > 0 and all(a >= b for a, b in zip(patch.shape, template.shape)):
                _, score, _, location = cv.minMaxLoc(cv.matchTemplate(patch, template, cv.TM_CCOEFF_NORMED))
            fraction = 1.0
            if score < rule["threshold"] and all(a >= b for a, b in zip(patch.shape, template.shape)):
                mask = occluded[top : top + patch.shape[0], left : left + patch.shape[1]]
                if np.any(mask):
                    score, location, fraction = visible_template_peak(patch, template, mask == 0)
            x, y = left + location[0], top + location[1]
            matched = score >= rule["threshold"] and _landmark_box_visible((x, y, x + template.shape[1], y + template.shape[0]))
            current.append(matched)
            scores.append(score if matched else None)
            fractions.append(fraction if matched else None)
            boxes.append((x, y, x + template.shape[1], y + template.shape[0]) if matched else box)
        if len(current) != 3 or sum(current) < 2:
            return None
        return replace(
            reference,
            evidence_boxes=tuple(boxes),
            scores=tuple(scores),
            current_parts=tuple(current),
            visible_fractions=tuple(fractions),
            provenance="current_static_two_parts",
        )

    def confirm_approach_parts(self, frame, reference, displacement):
        """검증된 배경 변위의 국소 탐색에서 현재 부위를 재확인한다. 이 결과는 도착을 승인하지 않는다."""
        if frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("native reviewed terminal frame required")
        images = self._images(frame, reference.part_ids)
        occluded = label_occlusion_mask(frame)
        shift = tuple(-value for value in displacement)
        boxes, scores, current, fractions, corrections = [], [], [], [], []
        for name, box in zip(reference.part_ids, reference.evidence_boxes):
            rule, template = self.native_parts[name]
            gray = images[rule.get("encoding", "gray")]
            predicted = tuple(int(round(value + shift[axis % 2])) for axis, value in enumerate(box))
            x0, y0, x1, y1 = predicted
            left, top = max(24, x0 - 8), max(55, y0 - 8)
            patch = gray[top : max(55, min(TERMINAL_OBSERVATION_BOTTOM, y1 + 8)), left : max(24, min(1900, x1 + 8))]
            score, fraction, location = -1, 0, (0, 0)
            if min(patch.shape) > 0 and all(a >= b for a, b in zip(patch.shape, template.shape)):
                _, score, _, location = cv.minMaxLoc(cv.matchTemplate(patch, template, cv.TM_CCOEFF_NORMED))
                fraction = 1.0
                mask = occluded[top : top + patch.shape[0], left : left + patch.shape[1]]
                if score < rule["threshold"] and np.any(mask):
                    score, location, fraction = visible_template_peak(patch, template, mask == 0)
            x, y = left + location[0], top + location[1]
            matched = score >= rule["threshold"] and _landmark_box_visible((x, y, x + template.shape[1], y + template.shape[0]))
            current.append(matched)
            scores.append(float(score) if matched else None)
            fractions.append(float(fraction) if matched else None)
            if matched:
                corrections.append((x - x0, y - y0))
                boxes.append((x, y, x + template.shape[1], y + template.shape[0]))
            else:
                boxes.append(predicted)
        if len(current) != 3 or not corrections:
            return None
        if any(math.hypot(a[0] - b[0], a[1] - b[1]) > 6 for a, b in itertools.combinations(corrections, 2)):
            return None
        correction = tuple(sum(value[axis] for value in corrections) / len(corrections) for axis in (0, 1))
        return replace(
            reference,
            point=tuple(value + delta + offset for value, delta, offset in zip(reference.point, shift, correction)),
            evidence_boxes=tuple(boxes),
            scores=tuple(scores),
            current_parts=tuple(current),
            visible_fractions=tuple(fractions),
            provenance="current_motion_approach_only",
        )

    def confirm_relocalized_parts(self, frame, reference):
        """과거의 전체 배치와 현재 서로 다른 두 부위의 공통 이동을 대조한다. 접근 전용이다."""
        if frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("native reviewed terminal frame required")
        images = self._images(frame[55:TERMINAL_OBSERVATION_BOTTOM, 24:1900], reference.part_ids)
        views = {encoding: (image, cv.resize(image, None, fx=0.5, fy=0.5)) for encoding, image in images.items()}
        gray, small = views["gray"]
        occluded = label_occlusion_mask(frame)[55:TERMINAL_OBSERVATION_BOTTOM, 24:1900]
        matches, groups = {}, []
        for name in reference.part_ids:
            rule, native = self.native_parts[name]
            found = self._match_groups(gray, small, ((rule, native, cv.resize(native, None, fx=0.5, fy=0.5)),), matches, occluded, views)
            groups.append([] if found is None else found[0])
        candidates = []
        for first, second in itertools.combinations(range(len(groups)), 2):
            for a, b in itertools.product(groups[first], groups[second]):
                shifts = [
                    tuple(box[axis] - reference.evidence_boxes[index][axis] for axis in (0, 1))
                    for index, (box, _, _) in ((first, a), (second, b))
                ]
                if math.hypot(*(x - y for x, y in zip(*shifts))) > 6:
                    continue
                centers = [((box[0] + box[2]) / 2, (box[1] + box[3]) / 2) for box, _, _ in (a, b)]
                shift = tuple((x + y) / 2 for x, y in zip(*shifts))
                if math.hypot(*(x - y for x, y in zip(*centers))) < 84 or math.hypot(*shift) > 900:
                    continue
                candidates.append((a[1] + b[1], first, second, a, b, shift))
        if not candidates:
            return None
        _, first, second, a, b, shift = max(candidates, key=lambda item: item[0])
        boxes = [tuple(int(round(value + shift[axis % 2])) for axis, value in enumerate(box)) for box in reference.evidence_boxes]
        scores, current, fractions = [None] * 3, [False] * 3, [None] * 3
        for index, (box, score, fraction) in ((first, a), (second, b)):
            boxes[index], scores[index], current[index], fractions[index] = box, score, True, fraction
        return replace(
            reference,
            point=tuple(value + delta for value, delta in zip(reference.point, shift)),
            evidence_boxes=tuple(boxes),
            scores=tuple(scores),
            current_parts=tuple(current),
            visible_fractions=tuple(fractions),
            provenance="current_landmark_approach_only",
        )

    @staticmethod
    def _match_groups(gray, small, parts, matches, occluded, views=None):
        groups = []
        for rule, native, template in parts:
            key = (rule["path"], rule["threshold"])
            if key in matches:
                if not matches[key]:
                    return None
                groups.append(matches[key])
                continue
            current, reduced = (gray, small) if views is None else views[rule.get("encoding", "gray")]
            found = ArcaneTerminalObserver._match_part(current, reduced, (rule, native, template), occluded)
            matches[key] = found
            if not found:
                return None
            groups.append(found)
        return groups

    @staticmethod
    def _match_part(gray, small, part, occluded):
        rule, native, template = part
        coarse_scores = cv.matchTemplate(small, template, cv.TM_CCOEFF_NORMED)
        scores = coarse_scores.copy()
        found = []
        height, width = native.shape
        # 작은 문양은 반 축소의 홀짝 정렬로 점수가 낮아진다. 후보만 넓히고 native 승인은 그대로 둔다.
        coarse_threshold = 0.8 if min(height, width) <= 48 else 0.85
        for _ in range(4):
            _, score, _, (x, y) = cv.minMaxLoc(scores)
            if score < coarse_threshold:
                break
            x0, y0 = max(0, x * 2 - 3), max(0, y * 2 - 3)
            patch = gray[y0 : y * 2 + height + 4, x0 : x * 2 + width + 4]
            if patch.shape[0] >= height and patch.shape[1] >= width:
                _, exact, _, (dx, dy) = cv.minMaxLoc(cv.matchTemplate(patch, native, cv.TM_CCOEFF_NORMED))
                if exact >= rule["threshold"]:
                    left, top = 24 + x0 + dx, 55 + y0 + dy
                    box = (left, top, left + width, top + height)
                    if _landmark_box_visible(box):
                        found.append((box, float(exact), 1.0))
            scores[max(0, y - height // 2) : y + height // 2 + 1, max(0, x - width // 2) : x + width // 2 + 1] = -1
        if np.any(occluded) and len(found) < 4:
            found.extend(ArcaneTerminalObserver._occluded_matches(gray, coarse_scores, rule, native, occluded, found))
        return found

    @staticmethod
    def _occluded_matches(gray, coarse_scores, rule, native, occluded, full_matches):
        scores = coarse_scores.copy()
        height, width = native.shape
        found = []
        for _ in range(8):
            _, coarse, _, (x, y) = cv.minMaxLoc(scores)
            if coarse < 0.45:
                break
            x0, y0 = max(0, x * 2 - 3), max(0, y * 2 - 3)
            patch = gray[y0 : y * 2 + height + 4, x0 : x * 2 + width + 4]
            mask = occluded[y0 : y * 2 + height + 4, x0 : x * 2 + width + 4]
            if patch.shape[0] >= height and patch.shape[1] >= width and np.any(mask):
                exact, (dx, dy), fraction = visible_template_peak(patch, native, mask == 0)
                if exact >= rule["threshold"] and fraction < 1:
                    left, top = 24 + x0 + dx, 55 + y0 + dy
                    box = (left, top, left + width, top + height)
                    previous = full_matches + found
                    if _landmark_box_visible(box) and not any(math.hypot(left - old[0][0], top - old[0][1]) < 10 for old in previous):
                        found.append((box, exact, fraction))
            scores[max(0, y - height // 2) : y + height // 2 + 1, max(0, x - width // 2) : x + width // 2 + 1] = -1
        return found

    @staticmethod
    def _layout(groups, kind, parts, minimum_separation=84, minimum_area=8000):
        valid = []
        for trio in itertools.product(*groups):
            points = [((box[0] + box[2]) / 2, (box[1] + box[3]) / 2) for box, _, _ in trio]
            distances = [math.hypot(a[0] - b[0], a[1] - b[1]) for a, b in itertools.combinations(points, 2)]
            if min(distances) < minimum_separation or max(distances) > 700:
                continue
            if len(points) == 3:
                a, b, c = points
                area = abs((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2
                if area < minimum_area:
                    continue
            valid.append((sum(score for _, score, _ in trio), points, trio))
        if not valid:
            return None
        _, points, trio = max(valid, key=lambda item: item[0])
        point = tuple(sum(value[axis] for value in points) / len(points) for axis in (0, 1))
        return TerminalReading(
            kind,
            point,
            tuple(box for box, _, _ in trio),
            tuple(score for _, score, _ in trio),
            tuple(rule["path"] for rule, _, _ in parts),
            provenance="current_masked_three_parts" if any(fraction < 1 for _, _, fraction in trio) else "current_three_parts",
            current_parts=(True, True, True),
            visible_fractions=tuple(fraction for _, _, fraction in trio),
            minimum_separation=minimum_separation,
            minimum_area=minimum_area,
        )
