"""발판 탐색과 협곡 마감이 공유하는 현재 일지 외형 판독."""

from __future__ import annotations

import hashlib
import json
import math
from dataclasses import dataclass
from pathlib import Path

import cv2 as cv


@dataclass(frozen=True)
class JournalMatch:
    score: float
    box: tuple
    platform_ground: tuple | None


class HorazonJournalObserver:
    def __init__(self):
        owner = Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        data = json.loads((owner / "journal-exit.json").read_text(encoding="utf-8"))
        self.parts = []
        for rule in [data["journal"]] + data.get("journal_variants", []):
            path = owner / rule["path"]
            if hashlib.sha256(path.read_bytes()).hexdigest() != rule["sha256"]:
                raise ValueError("reviewed Horazon journal fingerprint required")
            template = cv.imread(str(path), cv.IMREAD_GRAYSCALE)
            if template is None:
                raise ValueError("reviewed Horazon journal template required")
            offset = rule.get("platform_ground_offset")
            if offset is not None and (
                len(offset) != 2
                or any(isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) for value in offset)
            ):
                raise ValueError("reviewed journal platform-ground offset required")
            self.parts.append((rule, template))

    def matches(self, frame):
        if frame.shape != (1140, 1922, 3):
            raise ValueError("native journal frame required")
        patch = cv.cvtColor(frame[55:900, 24:1900], cv.COLOR_BGR2GRAY)
        matches = []
        for rule, template in self.parts:
            _, score, _, (x, y) = cv.minMaxLoc(cv.matchTemplate(patch, template, cv.TM_CCOEFF_NORMED))
            if score >= rule["threshold"]:
                height, width = template.shape
                left, top = x + 24, y + 55
                offset = rule.get("platform_ground_offset")
                ground = None if offset is None else (left + offset[0], top + offset[1])
                matches.append(JournalMatch(float(score), (left, top, left + width, top + height), ground))
        return sorted(matches, key=lambda match: match.score, reverse=True)

    def observe(self, frame):
        matches = self.matches(frame)
        if not matches:
            return None
        left, top, right, bottom = matches[0].box
        return ((left + right) / 2, (top + bottom) / 2)
