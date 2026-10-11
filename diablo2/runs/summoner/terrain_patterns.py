from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

import cv2 as cv
import numpy as np


@dataclass(frozen=True)
class TerrainLanding:
    kind: str
    point: tuple[int, int]
    score: float
    evidence_box: tuple[int, int, int, int]


class ArcaneTerrainPatterns:
    """관측한 계단 유형의 실제 발판 후보. 상대 오프셋만 소비하며 고정 화면 좌표로 시전하지 않는다."""

    def __init__(self, manifest=None):
        if manifest is None:
            manifest = (
                Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/navigation-patterns.json"
            )
        manifest = Path(manifest)
        data = json.loads(manifest.read_text(encoding="utf-8"))
        if data["schema"] != 1 or data["frame_size"] != [1922, 1140]:
            raise ValueError("reviewed terrain-pattern layout required")
        self.templates = []
        owner = manifest.parent.resolve()
        for rule in data["patterns"]:
            path = (manifest.parent / rule["path"]).resolve()
            if path.parent not in (owner, owner / "templates/navigation"):
                raise ValueError("terrain pattern outside owner")
            image = cv.imread(str(path), cv.IMREAD_GRAYSCALE)
            if image is None or not 0.9 <= rule["threshold"] <= 1:
                raise ValueError("positive reviewed terrain pattern required")
            self.templates.append((rule, image, cv.resize(image, None, fx=0.5, fy=0.5)))

    def observe(self, frame):
        if frame.shape != (1140, 1922, 3) or frame.dtype != np.uint8:
            raise ValueError("native reviewed terrain frame required")
        gray = cv.cvtColor(frame[55:900, 24:1900], cv.COLOR_BGR2GRAY)
        small = cv.resize(gray, None, fx=0.5, fy=0.5)
        found = []
        for rule, native, template in self.templates:
            scores = cv.matchTemplate(small, template, cv.TM_CCOEFF_NORMED)
            # 한 계단 여러 칸의 중복은 억제하되 화면의 별도 계단은 제한적으로 보존한다.
            for _ in range(4):
                _, score, _, (x, y) = cv.minMaxLoc(scores)
                if score < 0.9:
                    break
                left, top = 24 + x * 2, 55 + y * 2
                width, height = native.shape[1], native.shape[0]
                x0, y0 = max(0, left - 24 - 3), max(0, top - 55 - 3)
                patch = gray[y0 : min(gray.shape[0], top - 55 + height + 4), x0 : min(gray.shape[1], left - 24 + width + 4)]
                if patch.shape[0] >= height and patch.shape[1] >= width:
                    _, native_score, _, (nx, ny) = cv.minMaxLoc(cv.matchTemplate(patch, native, cv.TM_CCOEFF_NORMED))
                    if native_score >= rule["threshold"]:
                        left, top = 24 + x0 + nx, 55 + y0 + ny
                        point = (left + rule["landing_offset"][0], top + rule["landing_offset"][1])
                        found.append(TerrainLanding(rule["id"], point, float(native_score), (left, top, left + width, top + height)))
                scores[max(0, y - height // 2) : y + height // 2 + 1, max(0, x - width // 2) : x + width // 2 + 1] = -1
        return tuple(found)
