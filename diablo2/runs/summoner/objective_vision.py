from __future__ import annotations

from dataclasses import dataclass
from itertools import combinations, permutations
import hashlib
import json
from pathlib import Path

import cv2 as cv
import numpy as np


@dataclass(frozen=True)
class ObjectiveReading:
    kind: str
    journal_box: tuple
    journal_score: float
    rune_boxes: tuple
    evidence_kind: str = "journal_and_surrounding_runes"
    glyph_scores: tuple = ()
    ground_point: tuple = None
    slot_indices: tuple = ()
    layout_residual: float = None


def rune_core_mask(frame):
    blue = cv.inRange(cv.cvtColor(frame, cv.COLOR_BGR2HSV), (95, 50, 150), (135, 255, 255))
    b, g, r = cv.split(frame)
    # 움직이는 푸른 후광 대신 그 안의 밝은 획을 비교한다.
    return cv.dilate(blue, np.ones((7, 7), np.uint8)) & (((b >= 220) & (g >= 200) & (r >= 180)).astype(np.uint8) * 255)


class ArcaneObjectiveObserver:
    """일지와 주변 빛나는 룬으로 목표 발판을 찾는다. 생존/사망을 추정하지 않는다."""

    def __init__(self):
        owner = Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        path = owner / "templates/objective/journal.png"
        self.template = cv.imread(str(path), cv.IMREAD_GRAYSCALE)
        if self.template is None:
            raise ValueError("reviewed Horazon journal template required")
        manifest = json.loads((owner / "objective-runes.json").read_text(encoding="utf-8"))
        self.glyphs = []
        for part in manifest["parts"]:
            asset = owner / part["path"]
            if hashlib.sha256(asset.read_bytes()).hexdigest() != part["sha256"]:
                raise ValueError("reviewed objective glyph fingerprint required")
            self.glyphs.append((part, cv.resize(cv.imread(str(asset), 0), None, fx=0.5, fy=0.5, interpolation=cv.INTER_AREA)))
        self.glyph_threshold = manifest["match_threshold"]
        self.layout_tolerance = manifest["slot_layout_tolerance_pixels"]
        slots = np.asarray(manifest["glyph_slot_centers"], dtype=float)
        if slots.shape != (6, 2) or not np.isfinite(slots).all():
            raise ValueError("reviewed six-slot objective layout required")
        self.slot_assignments = {}
        for count in (4, 5, 6):
            indices = np.asarray(list(permutations(range(len(slots)), count)))
            self.slot_assignments[count] = (indices, slots[indices])
        self.approach_ground = tuple(manifest["approach_ground"])
        if len(self.approach_ground) != 2 or any(type(value) is not int for value in self.approach_ground):
            raise ValueError("reviewed platform approach ground required")

    @staticmethod
    def _visible_ground(point):
        return tuple(float(value) for value in point) if 24 <= point[0] < 1900 and 55 <= point[1] < 900 else None

    def _glyph_platform(self, frame):
        mask = cv.resize(rune_core_mask(frame[55:900, 24:1900]), None, fx=0.5, fy=0.5, interpolation=cv.INTER_AREA)
        joined = cv.morphologyEx((mask > 0).astype(np.uint8), cv.MORPH_CLOSE, np.ones((5, 5), np.uint8))
        _, _, stats, _ = cv.connectedComponentsWithStats(joined)
        if sum(width >= 10 and height >= 6 and area >= 20 for _, _, width, height, area in stats[1:]) < 4:
            return None
        matches = []
        for part, template in self.glyphs:
            scores = cv.matchTemplate(mask, template, cv.TM_CCOEFF_NORMED)
            _, score, _, (x, y) = cv.minMaxLoc(scores)
            if score < self.glyph_threshold:
                continue
            x, y = x * 2 + 24, y * 2 + 55
            x0, y0, x1, y1 = part["crop"]
            matches.append((part["id"], score, (x, y, x + x1 - x0, y + y1 - y0)))
        for count in range(len(matches), 3, -1):
            candidates = []
            indices, slots = self.slot_assignments[count]
            for group in combinations(matches, count):
                boxes = tuple(item[2] for item in group)
                centers = np.array([((a + c) / 2, (b + d) / 2) for a, b, c, d in boxes])
                if any(np.linalg.norm(a - b) < 100 for a, b in combinations(centers, 2)):
                    continue
                if np.ptp(centers[:, 0]) < 300 or np.ptp(centers[:, 1]) < 70:
                    continue
                # 룬 모양의 순서가 달라져도 현재 외형과 발판 자리는 일대일로 정합한다.
                offsets = centers - slots
                translations = np.median(offsets, axis=1)
                residuals = np.max(np.abs(offsets - translations[:, None, :]), axis=(1, 2))
                for index in np.flatnonzero(residuals <= self.layout_tolerance):
                    candidates.append((group, translations[index], indices[index], float(residuals[index])))
            if not candidates:
                continue
            translations = np.array([candidate[1] for candidate in candidates])
            if np.max(np.abs(translations - np.median(translations, axis=0))) > self.layout_tolerance:
                return None
            group, translation, assigned, residual = min(candidates, key=lambda candidate: candidate[3])
            return ObjectiveReading(
                "summoner_platform",
                None,
                None,
                tuple(item[2] for item in group),
                "distinct_glyph_shapes_and_slot_layout",
                tuple((item[0], float(item[1])) for item in group),
                self._visible_ground(np.add(self.approach_ground, translation)),
                tuple(int(index) for index in assigned),
                residual,
            )
        return None

    def observe(self, frame):
        if frame.dtype != np.uint8 or frame.shape != (1140, 1922, 3):
            raise ValueError("native reviewed objective frame required")
        world = frame[55:900, 24:1900]
        scores = cv.matchTemplate(cv.cvtColor(world, cv.COLOR_BGR2GRAY), self.template, cv.TM_CCOEFF_NORMED)
        _, score, _, (x, y) = cv.minMaxLoc(scores)
        if score < 0.92:
            return self._glyph_platform(frame)
        x, y = x + 24, y + 55
        h, w = self.template.shape
        # 바닥/아이템의 파란 효과만으로 목표를 만들지 않고 현재 일지 주변의 큰 룬을 요구한다.
        left, top = max(24, x - 500), max(55, y - 350)
        right, bottom = min(1900, x + w + 500), min(900, y + 80)
        patch = frame[top:bottom, left:right]
        mask = cv.inRange(cv.cvtColor(patch, cv.COLOR_BGR2HSV), (95, 50, 150), (135, 255, 255))
        mask = cv.morphologyEx(mask, cv.MORPH_CLOSE, np.ones((5, 5), np.uint8))
        _, _, stats, _ = cv.connectedComponentsWithStats(mask)
        boxes = tuple(
            (int(left + px), int(top + py), int(left + px + width), int(top + py + height))
            for px, py, width, height, area in stats[1:]
            if 20 <= width <= 100 and 20 <= height <= 110 and area >= 150
        )
        if not 4 <= len(boxes) <= 12 or max(b[0] for b in boxes) - min(b[0] for b in boxes) < 200:
            return self._glyph_platform(frame)
        return ObjectiveReading(
            "summoner_platform",
            (x, y, x + w, y + h),
            float(score),
            boxes,
            ground_point=self._visible_ground((x + w / 2, y + h + 80)),
        )
