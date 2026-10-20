from __future__ import annotations

import json
import math
from dataclasses import dataclass
from pathlib import Path

import cv2 as cv
import numpy as np


def _box(box, size):
    if len(box) != 4 or any(type(value) is not int for value in box):
        raise ValueError("integer client rectangle required")
    left, top, right, bottom = box
    if not 0 <= left < right <= size[0] or not 0 <= top < bottom <= size[1]:
        raise ValueError("client rectangle outside captured frame")
    return tuple(box)


@dataclass(frozen=True)
class ReferenceLayout:
    frame_size: tuple
    client_box: tuple
    aspect_tolerance: float = 0.01

    def __post_init__(self):
        if len(self.frame_size) != 2 or any(type(value) is not int or value <= 0 for value in self.frame_size):
            raise ValueError("positive reference frame size required")
        _box(self.client_box, self.frame_size)
        if not 0 <= self.aspect_tolerance <= 0.03:
            raise ValueError("bounded client aspect tolerance required")

    @classmethod
    def load(cls, path):
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        if data.get("schema") != 1:
            raise ValueError("unsupported reference layout schema")
        return cls(tuple(data["frame_size"]), tuple(data["client_box"]), data["aspect_tolerance"])


@dataclass(frozen=True)
class FrameGeometry:
    source_target: tuple
    source_client: tuple
    layout: ReferenceLayout

    @classmethod
    def create(cls, target, layout=None, client_box=None):
        values = tuple(target[name] for name in ("left", "top", "width", "height"))
        if any(type(value) is not int for value in values) or min(values[2:]) <= 0:
            raise ValueError("integer physical capture rectangle required")
        width, height = values[2:]
        if layout is None:
            layout = ReferenceLayout((width, height), (0, 0, width, height))
            client_box = layout.client_box
        source_client = _box(client_box, (width, height))
        a, b, c, d = source_client
        x0, y0, x1, y1 = layout.client_box
        error = abs(((c - a) / (d - b)) / ((x1 - x0) / (y1 - y0)) - 1)
        if error > layout.aspect_tolerance:
            raise ValueError("game client aspect differs from reference layout: {:.3%}".format(error))
        return cls(values, source_client, layout)

    @property
    def target(self):
        return dict(zip(("left", "top", "width", "height"), self.source_target))

    @property
    def frame_size(self):
        return self.layout.frame_size

    def normalize(self, frame):
        if frame.dtype != np.uint8 or frame.shape != (self.source_target[3], self.source_target[2], 3):
            raise ValueError("frame and captured geometry differ")
        if self.frame_size == self.source_target[2:] and self.source_client == self.layout.client_box:
            return frame
        # OS 제목 표시줄/테두리는 게임 내용과 배율이 다르므로 내용 영역을 따로 맞춘다.
        output = cv.resize(frame, self.frame_size, interpolation=cv.INTER_LINEAR)
        a, b, c, d = self.source_client
        x0, y0, x1, y1 = self.layout.client_box
        interpolation = cv.INTER_AREA if c - a >= x1 - x0 and d - b >= y1 - y0 else cv.INTER_LINEAR
        output[y0:y1, x0:x1] = cv.resize(frame[b:d, a:c], (x1 - x0, y1 - y0), interpolation=interpolation)
        return output

    @staticmethod
    def _axis(value, source, target):
        if not math.isfinite(value) or not 0 <= value < source[-1]:
            raise ValueError("point outside coordinate frame")
        # 내용·제목 표시줄·테두리 각각의 픽셀 중심을 대응시킨다.
        for start, end, left, right in zip(source, source[1:], target, target[1:]):
            if start <= value < end:
                return left + (value - start + 0.5) * (right - left) / (end - start) - 0.5
        raise ValueError("unmapped coordinate")

    def _coordinates(self, point, inverse=False):
        result = []
        for axis, value in enumerate(point):
            ref = (0, self.layout.client_box[axis], self.layout.client_box[axis + 2], self.frame_size[axis])
            raw = (0, self.source_client[axis], self.source_client[axis + 2], self.source_target[axis + 2])
            result.append(self._axis(float(value), raw, ref) if inverse else self._axis(float(value), ref, raw))
        return tuple(result)

    def to_desktop(self, point):
        local = self._coordinates(point)
        return tuple(round(value + origin) for value, origin in zip(local, self.source_target[:2]))

    def from_desktop(self, point):
        return self._coordinates(tuple(value - origin for value, origin in zip(point, self.source_target[:2])), inverse=True)

    def as_dict(self):
        return dict(
            source_target=self.target, source_client=self.source_client, frame_size=self.frame_size, client_box=self.layout.client_box
        )


def geometry_is_current(capture):
    geometry = getattr(capture, "frame_geometry", None)
    if not isinstance(geometry, FrameGeometry):
        return True
    if capture.current_target() != geometry.target:
        return False
    current_client = getattr(capture, "current_client_box", None)
    return current_client is None or tuple(current_client()) == geometry.source_client


def frame_to_desktop(capture, point):
    geometry = getattr(capture, "frame_geometry", None)
    if isinstance(geometry, FrameGeometry):
        if not geometry_is_current(capture):
            raise RuntimeError("window geometry changed after capture")
        return geometry.to_desktop(point)
    return tuple(value + capture.target[name] for value, name in zip(point, ("left", "top")))


def desktop_to_frame(capture, point):
    geometry = getattr(capture, "frame_geometry", None)
    if isinstance(geometry, FrameGeometry):
        if not geometry_is_current(capture):
            raise RuntimeError("window geometry changed after capture")
        # 영상 소비자는 이 점으로 정수 crop을 만든다. 연속 기하 계산은 from_desktop이 보존한다.
        return tuple(round(value) for value in geometry.from_desktop(point))
    return tuple(round(value - capture.target[name]) for value, name in zip(point, ("left", "top")))
