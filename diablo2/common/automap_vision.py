from __future__ import annotations

import json
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.label_occlusion import label_occlusion_mask


class AutomapObserver:
    """자동지도와 함께 표시되는 지역 제목의 긍정 근거만 읽는다."""

    def __init__(self, directory=None):
        directory = directory or Path(__file__).resolve().parents[2] / "assets/ui/automap"
        self.manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
        self.templates = {}
        for area, definition in self.manifest["areas"].items():
            template = cv.imdecode(np.fromfile(str(directory / definition["file"]), dtype=np.uint8), cv.IMREAD_COLOR)
            if template is None:
                raise ValueError("Unreadable automap title: " + area)
            self.templates[area] = self._title_mask(template)
        self.last_evidence = None
        self.last_obscured = False

    @staticmethod
    def _title_mask(frame):
        b, g, r = (channel.astype(np.int16) for channel in cv.split(frame))
        # 제목 밑을 통과하는 회색 지도 선을 글자 외형에서 제외한다.
        mask = ((r > 70) & (g > 45) & (r - g > 8) & (g - b > 12) & (r - b > 35)).astype(np.uint8) * 255
        return cv.GaussianBlur(mask, (3, 3), 0.8)

    def visible(self, frame, area):
        self.last_evidence = None
        self.last_obscured = False
        width, height = self.manifest["reference_size"]
        if abs(frame.shape[1] / frame.shape[0] / (width / height) - 1) > 0.03:
            return False
        definition = self.manifest["areas"][area]
        normalized = cv.resize(frame, (width, height), interpolation=cv.INTER_AREA)
        x0, y0, x1, y1 = definition["search"]
        patch = self._title_mask(normalized[y0:y1, x0:x1])
        _, score, _, location = cv.minMaxLoc(cv.matchTemplate(patch, self.templates[area], cv.TM_CCOEFF_NORMED))
        if score < definition["minimum_score"]:
            # 이름표가 제목 검색 영역을 가리면 미검출을 토글 요청으로 쓰지 않는다.
            mask = label_occlusion_mask(normalized, include_gold=False)
            self.last_obscured = bool(np.any(mask[y0:y1, x0:x1]))
            return False
        self.last_evidence = dict(area=area, score=score, title_at=(x0 + location[0], y0 + location[1]))
        return True
