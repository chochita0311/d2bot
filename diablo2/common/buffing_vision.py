from __future__ import annotations

import json
import math
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.survival_vision import HudReading


def resource_increase_candidate(before: HudReading, after: HudReading) -> bool:
    # 최대값 증가만으로 버프 종류나 전체 순서의 성공을 확정하지 않는다.
    pairs = ((before.life, after.life), (before.mana, after.mana))
    return all(left.ratio is not None and right.ratio is not None and right.maximum > left.maximum for left, right in pairs)


class WeaponSetObserver:
    """열린 인벤토리의 공통 I/II 표시만 읽으며 장비나 개인 전투 세트를 추측하지 않는다."""

    def __init__(self, assets: str | Path | None = None):
        self.assets = Path(assets or Path(__file__).resolve().parents[2] / "assets/ui/weapon-set").resolve()
        self.manifest = json.loads((self.assets / "manifest.json").read_text(encoding="utf-8"))
        raw = self.manifest
        if type(raw["schema_version"]) is not int or raw["schema_version"] != 1 or raw["frame_size"] != [1922, 1140]:
            raise ValueError("Reviewed weapon-tab layout required")
        for name in ("maximum_distance", "minimum_margin"):
            value = raw[name]
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not 0 < value < 1:
                raise ValueError("Finite weapon matching ratio required")
        if len(raw["regions"]) != 2 or set(raw["templates"]) != {"1", "2"}:
            raise ValueError("Both weapon regions and sets required")
        for box in raw["regions"]:
            if len(box) != 4 or any(type(value) is not int for value in box):
                raise ValueError("Integer weapon region required")
            l, t, r, b = box
            if not 0 <= l < r <= 1922 or not 0 <= t < b <= 1140:
                raise ValueError("Weapon region outside reviewed frame")
        shape = (raw["regions"][0][3] - raw["regions"][0][1], sum(box[2] - box[0] for box in raw["regions"]))
        if any(box[3] - box[1] != shape[0] for box in raw["regions"]):
            raise ValueError("Equal-height tab regions required")
        self.templates = {}
        for key, name in raw["templates"].items():
            path = (self.assets / name).resolve()
            if self.assets not in path.parents:
                raise ValueError("Weapon asset outside its owner")
            image = cv.imread(str(path))
            if image is None or image.shape[:2] != shape:
                raise ValueError("Weapon reference dimensions do not match regions")
            self.templates[int(key)] = image

    def observe(self, frame: np.ndarray) -> int | None:
        if not isinstance(frame, np.ndarray) or frame.dtype != np.uint8 or frame.ndim != 3 or frame.shape[2] != 3:
            raise ValueError("Expected uint8 BGR frame")
        if frame.shape[:2] != (1140, 1922):
            return None
        pieces = [frame[t:b, l:r] for l, t, r, b in self.manifest["regions"]]
        strip = np.concatenate(pieces, axis=1).astype(np.float32)
        candidates = sorted(
            (float(np.abs(strip - template.astype(np.float32)).mean() / 255), key) for key, template in self.templates.items()
        )
        distance, number = candidates[0]
        if distance > self.manifest["maximum_distance"] or candidates[1][0] - distance < self.manifest["minimum_margin"]:
            return None
        return number
