from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.color_palette import palette_distance_map_bgr
from diablo2.common.encounters import SceneContext, VisualCandidate
from diablo2.common.survival import ObservationStamp, _integer, _number


def _image(frame: np.ndarray, context: SceneContext) -> None:
    x0, y0, x1, y1 = context.viewport
    if not isinstance(frame, np.ndarray) or frame.dtype != np.uint8 or frame.shape != (y1 - y0, x1 - x0, 3):
        raise ValueError("image must be native BGR pixels for the declared viewport")


def _palette(raw) -> tuple[tuple[int, int, int], ...]:
    if not isinstance(raw, list) or not raw:
        raise ValueError("non-empty BGR palette required")
    colors = []
    for color in raw:
        if not isinstance(color, list) or len(color) != 3:
            raise ValueError("palette requires BGR triples")
        colors.append(tuple(_integer(v, "palette", 0, 255) for v in color))
    return tuple(colors)


@dataclass(frozen=True)
class TerrainEvidence:
    context: SceneContext
    box: tuple[int, int, int, int]
    floor_ratio: float
    void_ratio: float
    state: str


class RegionObserver:
    def __init__(self, manifest_path: str | Path):
        path = Path(manifest_path).resolve()
        raw = json.loads(path.read_text(encoding="utf-8"))
        if raw.get("schema") != 1:
            raise ValueError("unsupported region manifest schema")
        self.region_id = raw["region_id"]
        self.frame_size = tuple(raw["frame_size"])
        if len(self.frame_size) != 2:
            raise ValueError("frame_size requires width/height")
        for dimension in self.frame_size:
            _integer(dimension, "frame_size", 1)
        terrain = raw["terrain"]
        self.floor_palette = _palette(terrain["floor_bgr"])
        self.void_palette = _palette(terrain["void_bgr"])
        self.floor_distance = _number(terrain["floor_lab_distance"], "floor_lab_distance")
        self.void_distance = _number(terrain["void_lab_distance"], "void_lab_distance")
        self.templates = []
        names = set()
        for rule in raw["templates"]:
            asset = (path.parent / rule["path"]).resolve()
            try:
                asset.relative_to(path.parent)
            except ValueError:
                raise ValueError("templates must stay inside the region owner") from None
            if asset.suffix.lower() != ".png":
                raise ValueError("templates must be owned PNG files")
            if asset.read_bytes()[:8] != b"\x89PNG\r\n\x1a\n":
                raise ValueError("template must have PNG bytes")
            template = cv.imread(str(asset), cv.IMREAD_COLOR)
            if template is None or template.size == 0 or not np.any(np.ptp(template, axis=(0, 1))):
                raise ValueError("template must contain non-uniform BGR pixels")
            if rule["id"] in names:
                raise ValueError("template IDs must be unique")
            names.add(rule["id"])
            _number(rule["threshold"], "threshold", maximum=1)
            if rule["threshold"] <= 0:
                raise ValueError("positive threshold required")
            # 공통 후보 계약으로 종류/단계/지상 기준점도 로드 시 검증한다.
            w, h = template.shape[1], template.shape[0]
            check = SceneContext(
                ObservationStamp("validation", "validation", 0, 0), self.region_id, 0, self.frame_size, (0, 0, *self.frame_size)
            )
            VisualCandidate(check, rule["id"], rule["kind"], rule["category"], rule["appearance"], 1, (0, 0, w, h), rule["ground_offset"])
            self.templates.append((rule, template))

    def _validate(self, frame: np.ndarray, context: SceneContext) -> None:
        _image(frame, context)
        if context.region_id != self.region_id or context.frame_size != self.frame_size:
            raise ValueError("unreviewed region or capture size")

    def scan(
        self, frame: np.ndarray, context: SceneContext, *, now: float, max_age: float, limit_per_template: int
    ) -> tuple[VisualCandidate, ...]:
        self._validate(frame, context)
        _integer(limit_per_template, "limit_per_template", 1, 16)
        if not context.fresh(now, max_age):
            return ()
        candidates = []
        vx, vy, _, _ = context.viewport
        for rule, template in self.templates:
            h, w = template.shape[:2]
            if frame.shape[0] < h or frame.shape[1] < w:
                continue
            result = cv.matchTemplate(frame, template, cv.TM_CCOEFF_NORMED)
            for _ in range(limit_per_template):
                _, score, _, (x, y) = cv.minMaxLoc(result)
                if not np.isfinite(score) or score < rule["threshold"]:
                    break
                ground = rule["ground_offset"]
                absolute_ground = None if ground is None else (vx + x + ground[0], vy + y + ground[1])
                candidates.append(
                    VisualCandidate(
                        context,
                        rule["id"],
                        rule["kind"],
                        rule["category"],
                        rule["appearance"],
                        min(1.0, score),
                        (vx + x, vy + y, vx + x + w, vy + y + h),
                        absolute_ground,
                    )
                )
                # 같은 이미지의 겹치는 최고점들을 개체 수로 세지 않는다.
                result[max(0, y - h + 1) : min(result.shape[0], y + h), max(0, x - w + 1) : min(result.shape[1], x + w)] = -1
        return tuple(candidates)

    def terrain(
        self,
        frame: np.ndarray,
        context: SceneContext,
        box: tuple[int, int, int, int],
        *,
        now: float,
        max_age: float,
        minimum_floor: float,
        maximum_void: float,
    ) -> TerrainEvidence:
        self._validate(frame, context)
        _number(minimum_floor, "minimum_floor", maximum=1)
        _number(maximum_void, "maximum_void", maximum=1)
        if minimum_floor <= 0 or maximum_void >= 1:
            raise ValueError("terrain gates must reject unobserved/void-only patches")
        if len(box) != 4:
            raise ValueError("terrain box requires four coordinates")
        x0, y0, x1, y1 = (_integer(v, "terrain box") for v in box)
        vx0, vy0, vx1, vy1 = context.viewport
        if not (vx0 <= x0 < x1 <= vx1 and vy0 <= y0 < y1 <= vy1):
            raise ValueError("terrain patch must be inside the viewport")
        if not context.fresh(now, max_age):
            return TerrainEvidence(context, tuple(box), 0, 0, "unknown")
        patch = frame[y0 - vy0 : y1 - vy0, x0 - vx0 : x1 - vx0]
        floor = palette_distance_map_bgr(patch, self.floor_palette) <= self.floor_distance
        void = palette_distance_map_bgr(patch, self.void_palette) <= self.void_distance
        # 팔레트가 겹친 픽셀은 양쪽에서 제외한다.
        floor_ratio = float((floor & ~void).mean())
        void_ratio = float((void & ~floor).mean())
        state = "floor-candidate" if floor_ratio >= minimum_floor and void_ratio <= maximum_void else "reject-or-unknown"
        return TerrainEvidence(context, tuple(box), floor_ratio, void_ratio, state)
