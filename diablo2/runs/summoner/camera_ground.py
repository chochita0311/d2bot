from __future__ import annotations

from collections import deque
import json
import math
from pathlib import Path

import numpy as np

from diablo2.runs.summoner.scene_vision import GroundReading


class CalibratedCameraGroundObserver:
    """감독 확인한 닫힌 viewport에서 검토 배치/현재 몸으로 제한된 카메라 투영을 보정한다."""

    def __init__(self, ground, panels_open, *, maximum_occlusion=2.0, maximum_spread=12, maximum_shift=24, persist_reviewed_layout=False):
        self.ground, self.panels_open = ground, panels_open
        self.maximum_occlusion, self.maximum_spread, self.maximum_shift = maximum_occlusion, maximum_spread, maximum_shift
        self.authorized = False
        self.context = None
        self.samples = deque(maxlen=3)
        self.center, self.last_strong_at, self.last_at = None, None, None
        self.viewport_anchor = None
        self.reviewed_anchor = None
        self.calibrated_at = None
        self.projections = 0
        self.calibrations = 0
        self.seed_started = None
        self.reference_kind = "current_body"
        self.persist_reviewed_layout = persist_reviewed_layout
        self.reviewed_projection_active = False
        self.reset_events = deque(maxlen=32)

    def reset(self, reason="reset", *, captured_at=None, preserve_reviewed=False):
        retained = preserve_reviewed and self.reviewed_projection_active and self.persist_reviewed_layout
        self.samples.clear()
        self.center, self.last_strong_at, self.calibrated_at = self.viewport_anchor if retained else None, None, None
        self.seed_started = None
        self.reviewed_projection_active = bool(retained)
        self.reset_events.append(dict(reason=reason, captured_at=captured_at, reviewed_reference_retained=bool(retained)))

    def confirm_closed_viewport(self, proof, now, *, reviewed_layout=None):
        at = proof.get("observed_at")
        if (
            proof.get("source") != "supervised_visual"
            or proof.get("panels_closed") is not True
            or type(at) not in (int, float)
            or not math.isfinite(at)
            or not 0 <= now - at <= 15
        ):
            raise ValueError("fresh supervised closed-panel viewport proof required")
        self.reset("new_supervised_confirmation")
        self.viewport_anchor = None
        self.reviewed_anchor = None
        self.authorized, self.context, self.last_at = True, None, None
        self.reference_kind = "current_body"
        if reviewed_layout is not None:
            if proof.get("camera_layout_verified") is not True:
                raise ValueError("fresh current camera-layout confirmation required")
            self._seed_reviewed_layout(reviewed_layout)

    def confirm_runtime_viewport(self, frame, hud, focused, captured_at, now, *, reviewed_layout):
        """현재 HUD·닫힌 패널·native 배치로 공용 카메라 기준을 활성화한다."""
        if (
            not focused
            or frame.shape != (1140, 1922, 3)
            or not 0 <= now - captured_at < 0.25
            or self.panels_open(frame)
            or hud.belt_visibility != "closed"
            or hud.life.ratio is None
            or hud.mana.ratio is None
        ):
            raise ValueError("current closed native runtime viewport required")
        self.reset("new_runtime_viewport")
        self.authorized, self.context, self.last_at = True, None, None
        self._seed_reviewed_layout(reviewed_layout)
        self.prepare_frame(frame, captured_at, hud, focused)

    def _seed_reviewed_layout(self, path):
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        points = data.get("body_observations", [])
        if (
            data.get("schema") != 1
            or data.get("frame_size") != [1922, 1140]
            or len(points) != 3
            or any(len(item.get("point", [])) != 2 or not all(type(v) is int for v in item["point"]) for item in points)
        ):
            raise ValueError("reviewed native three-body camera calibration required")
        center = tuple(int(round(v)) for v in np.median([item["point"] for item in points], axis=0))
        if not 24 <= center[0] < 1900 or not 55 <= center[1] < 900:
            raise ValueError("camera reference outside world viewport")
        if any(math.hypot(*(a - b for a, b in zip(item["point"], center))) > self.maximum_spread for item in points):
            raise ValueError("camera reference body samples disagree")
        self.center = self.viewport_anchor = self.reviewed_anchor = center
        self.reference_kind = "reviewed_layout"
        self.reviewed_projection_active = self.persist_reviewed_layout

    def prepare_frame(self, frame, captured_at, hud, focused):
        layout_valid = self.authorized and focused and frame.shape == (1140, 1922, 3)
        if layout_valid and self.panels_open(frame):
            self.authorized, layout_valid = False, False
        if not focused or frame.shape != (1140, 1922, 3):
            self.authorized = False
        valid = layout_valid and hud.belt_visibility == "closed" and hud.life.ratio is not None and hud.mana.ratio is not None
        if not valid:
            # HUD 한 프레임 누락은 현재 투영을 막지만 확인된 고정 viewport 자체를 바꾸지는 않는다.
            # 실제 패널/포커스/크기 변화나 강한 몸 위치 모순은 아래와 같이 보정을 폐기한다.
            self.reset(
                "resource_observation_unverified" if layout_valid else "viewport_unverified",
                captured_at=captured_at,
                preserve_reviewed=layout_valid,
            )
        elif self.center is not None and self.last_strong_at is None and self.seed_started is None:
            self.seed_started = captured_at
        # 프레임 자체를 보관하지 않는다. 이 호출과 동일한 프레임/시각만 유효하다.
        self.context = (id(frame), captured_at, bool(valid))

    def observe(self, frame, *, captured_at=None):
        if type(captured_at) not in (int, float) or not math.isfinite(captured_at) or captured_at < 0:
            raise ValueError("finite camera capture time required")
        if self.last_at is not None and captured_at < self.last_at:
            self.reset("camera_time_reversed", captured_at=captured_at)
            self.last_at = captured_at
            return GroundReading(None, "camera_time_reversed")
        self.last_at = captured_at
        reading = self.ground.observe(frame, captured_at=captured_at)
        if self.context != (id(frame), captured_at, True):
            self.reset(
                "current_frame_context_unverified",
                captured_at=captured_at,
                preserve_reviewed=self.authorized and self.context == (id(frame), captured_at, False),
            )
            return reading
        if (
            reading.point is not None
            and reading.reason == "flash_es_and_body_shape"
            and self.viewport_anchor is not None
            and math.hypot(*(a - b for a, b in zip(reading.point, self.viewport_anchor))) > self.maximum_shift
        ):
            # 금색 지형+전경 조합은 몸 신원이 아니다. 검토한 카메라 밖의 이 약한 후보로 보정을 옮기지 않는다.
            reading = GroundReading(None, "ground_unverified")
        if reading.point is not None and reading.reason != "flash_marker_tracked":
            if (
                self.viewport_anchor is not None
                and math.hypot(*(a - b for a, b in zip(reading.point, self.viewport_anchor))) > self.maximum_shift
            ):
                # 전경/효과 후보가 순간 다른 개체로 튀면 그 좌표를 이동에 사용하지 않는다.
                # 같은 viewport의 원래 중심에서 실제 몸3개를 다시 읽어야 보정을 복구한다.
                self.reset("camera_actor_displaced", captured_at=captured_at)
                return GroundReading(None, "camera_actor_displaced")
            if self.samples and captured_at - self.samples[-1][0] > 0.5:
                self.samples.clear()
            if not self.samples or captured_at > self.samples[-1][0]:
                self.samples.append((captured_at, reading.point))
            self.last_strong_at = captured_at
            if len(self.samples) == 3 and self.samples[-1][0] - self.samples[0][0] >= 0.15:
                center = tuple(int(round(value)) for value in np.median([point for _, point in self.samples], axis=0))
                if all(math.hypot(*(a - b for a, b in zip(point, center))) <= self.maximum_spread for _, point in self.samples):
                    if self.center is None:
                        self.center, self.calibrated_at = center, captured_at
                        self.viewport_anchor = center
                        self.calibrations += 1
                        if (
                            self.persist_reviewed_layout
                            and self.reviewed_anchor is not None
                            and math.hypot(*(a - b for a, b in zip(center, self.reviewed_anchor))) <= self.maximum_spread
                        ):
                            # 강한 위치 모순 뒤에도 현재 몸3개가 검토 중심을 다시 확인하면 가림 투영을 복구한다.
                            self.reviewed_projection_active = True
            return reading
        reference_at = self.last_strong_at if self.last_strong_at is not None else self.seed_started
        if (
            not self.reviewed_projection_active
            and self.center is not None
            and (reference_at is None or captured_at - reference_at > self.maximum_occlusion)
        ):
            self.reset("camera_reference_expired", captured_at=captured_at)
        if reading.point is not None or reading.reason != "ground_unverified" or self.center is None:
            return reading
        self.projections += 1
        return GroundReading(
            self.center,
            "camera_ground_projection",
            calibration={
                "calibrated_at": self.calibrated_at,
                "last_body_at": self.last_strong_at,
                "occluded_seconds": captured_at - reference_at,
                "panel_basis": "supervised_closed_and_controlled_inputs",
                "reference_kind": self.reference_kind,
                "current_reviewed_viewport": self.reviewed_projection_active,
            },
        )
