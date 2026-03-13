from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import cv2 as cv
import numpy as np

from d2bot.config import TemplateRule


@dataclass
class TemplateMatch:
    name: str
    confidence: float
    top_left: tuple[int, int]
    bottom_right: tuple[int, int]
    action: str


class TemplateMatcher:
    def __init__(self, rules: list[TemplateRule]):
        self.templates: list[tuple[TemplateRule, np.ndarray]] = []
        for rule in rules:
            image = cv.imread(str(Path(rule.path)), cv.IMREAD_UNCHANGED)
            if image is None:
                raise FileNotFoundError(f"Template not found: {rule.path}")
            self.templates.append((rule, image))

    def scan(self, frame: np.ndarray) -> list[TemplateMatch]:
        matches: list[TemplateMatch] = []
        for rule, template in self.templates:
            result = cv.matchTemplate(frame, template, cv.TM_CCOEFF_NORMED)
            _, max_val, _, max_loc = cv.minMaxLoc(result)
            if max_val < rule.threshold:
                continue
            h, w = template.shape[:2]
            matches.append(
                TemplateMatch(
                    name=rule.name,
                    confidence=float(max_val),
                    top_left=max_loc,
                    bottom_right=(max_loc[0] + w, max_loc[1] + h),
                    action=rule.action,
                )
            )
        return matches


def draw_overlay(frame: np.ndarray, matches: list[TemplateMatch], status_text: str) -> np.ndarray:
    output = frame.copy()
    for match in matches:
        cv.rectangle(output, match.top_left, match.bottom_right, (0, 255, 0), 2)
        label = f"{match.name} {match.confidence:.2f}"
        cv.putText(
            output,
            label,
            (match.top_left[0], max(20, match.top_left[1] - 8)),
            cv.FONT_HERSHEY_SIMPLEX,
            0.6,
            (0, 255, 0),
            2,
            cv.LINE_AA,
        )
    cv.putText(
        output,
        status_text,
        (20, 30),
        cv.FONT_HERSHEY_SIMPLEX,
        0.8,
        (0, 215, 255),
        2,
        cv.LINE_AA,
    )
    return output
