from __future__ import annotations

from dataclasses import dataclass

import cv2 as cv
import numpy as np

from diablo2.common.survival import _integer, _number


@dataclass(frozen=True)
class CameraMotion:
    displacement: tuple[float, float] | None
    inliers: int
    reason: str
    matches: int = 0
    inlier_ratio: float | None = None
    linear_error: float | None = None


@dataclass(frozen=True)
class SceneFeatures:
    sequence: int
    captured_at: float
    frame_size: tuple[int, int]
    points: np.ndarray
    descriptors: np.ndarray | None


class BackgroundMotionObserver:
    """현재 발판 특징의 공통 이동만 누적하며 효과 변화나 미검출을 도착으로 쓰지 않는다."""

    def __init__(self, *, minimum_matches, minimum_inlier_ratio, maximum_error, scale=0.5, landmark_viewport=None, actor_exclusion=None):
        self.minimum_matches = _integer(minimum_matches, "minimum_matches", 4)
        self.minimum_inlier_ratio = _number(minimum_inlier_ratio, "minimum_inlier_ratio", maximum=1)
        self.maximum_error, self.scale = _number(maximum_error, "maximum_error"), _number(scale, "scale", maximum=1)
        if min(self.minimum_inlier_ratio, self.maximum_error, self.scale) == 0:
            raise ValueError("positive motion limits required")
        self.orb = cv.ORB_create(nfeatures=600)
        self.matcher = cv.BFMatcher(cv.NORM_HAMMING)
        self.previous = None
        self.last_seen = None
        self.features = None
        if (landmark_viewport is None) != (actor_exclusion is None):
            raise ValueError("Landmark viewport and actor exclusion must be supplied together")
        if landmark_viewport is not None:
            for box in (landmark_viewport, actor_exclusion):
                if len(box) != 4 or any(type(value) is not int or value < 0 for value in box):
                    raise ValueError("Native landmark/exclusion geometry required")
        self.landmark_viewport, self.actor_exclusion = landmark_viewport, actor_exclusion

    def reset(self):
        self.previous = None
        self.last_seen = None
        self.features = None

    def observe(self, frame, view, *, overlap_hint=None):
        width, height = view.frame_size
        if frame.shape[:2] != (height, width):
            raise ValueError("motion frame and path view differ")
        mask = np.zeros((height, width), np.uint8)
        ax, ay = (int(value) for value in view.anchor)
        if self.landmark_viewport is None:
            x0, y0, x1, y1 = view.viewport
            occupied = np.repeat(np.repeat(view.floor.astype(np.uint8) * 255, view.cell_pixels, axis=0), view.cell_pixels, axis=1)
            mask[y0 : y0 + occupied.shape[0], x0 : x0 + occupied.shape[1]] = occupied
            radius = int(0.1 * min(x1 - x0, y1 - y0))
            mask[max(y0, ay - radius) : min(y1, ay + radius), max(x0, ax - radius) : min(x1, ax + radius)] = 0
        else:
            x0, y0, x1, y1 = self.landmark_viewport
            if not 0 <= x0 < x1 <= width or not 0 <= y0 < y1 <= height:
                raise ValueError("Landmark viewport outside frame")
            # 이동 경로의 좁은 타일 대신 벽/계단까지 추적하되 캐릭터·용병 효과는 제외한다.
            mask[y0:y1, x0:x1] = 255
            left, above, right, below = self.actor_exclusion
            mask[max(y0, ay - above) : min(y1, ay + below), max(x0, ax - left) : min(x1, ax + right)] = 0
        excluded = getattr(view, "excluded_pixels", None)
        if excluded is not None:
            if excluded.shape != mask.shape or excluded.dtype != np.bool_:
                raise ValueError("same-frame excluded-pixel mask required")
            mask[excluded] = 0
        return self.observe_mask(frame, mask, sequence=view.sequence, captured_at=view.captured_at, overlap_hint=overlap_hint)

    def observe_mask(self, frame, mask, *, sequence, captured_at, overlap_hint=None):
        gray = cv.resize(cv.cvtColor(frame, cv.COLOR_BGR2GRAY), None, fx=self.scale, fy=self.scale)
        if mask.shape != frame.shape[:2]:
            raise ValueError("motion mask and frame differ")
        mask = cv.resize(mask, (gray.shape[1], gray.shape[0]), interpolation=cv.INTER_NEAREST)
        frame_size = (frame.shape[1], frame.shape[0])
        points, descriptors = self.orb.detectAndCompute(gray, mask)
        current = (sequence, captured_at, frame_size, points, descriptors)
        previous = self.previous
        if self.last_seen is not None and (sequence <= self.last_seen[0] or captured_at < self.last_seen[1]):
            return CameraMotion(None, 0, "frame_not_newer")
        self.last_seen = (sequence, captured_at)
        # 재위치 판독은 같은 특징을 재사용하며 원본 프레임을 지도 기억에 보관하지 않는다.
        native_points = np.float32([point.pt for point in points]).reshape(-1, 2) / self.scale
        self.features = SceneFeatures(sequence, captured_at, frame_size, native_points, descriptors)
        if previous is None or previous[2] != frame_size or descriptors is None or previous[4] is None:
            if previous is None or previous[2] != frame_size or previous[4] is None:
                self.previous = current
            return CameraMotion(None, 0, "correspondence_unavailable")
        source_points, source_descriptors = previous[3], previous[4]
        target_points, target_descriptors = points, descriptors
        if overlap_hint is not None:
            if (
                not isinstance(overlap_hint, tuple)
                or len(overlap_hint) != 2
                or any(type(value) not in (int, float) or not np.isfinite(value) for value in overlap_hint)
                or self.landmark_viewport is None
            ):
                raise ValueError("finite native overlap hint and landmark viewport required")
            x0, y0, x1, y1 = self.landmark_viewport

            def visible(points, descriptors, sign):
                projected = np.float32([point.pt for point in points]) / self.scale + sign * np.asarray(overlap_hint)
                included = ((projected >= (x0, y0)) & (projected < (x1, y1))).all(axis=1)
                return [point for point, keep in zip(points, included) if keep], descriptors[included]

            # 조준 예상치는 두 화면에 함께 보일 수 있는 영역만 고른다.
            # 실제 변위는 아래 동일한 특징 대응/RANSAC 기준으로 따로 측정한다.
            source_points, source_descriptors = visible(source_points, source_descriptors, -1)
            target_points, target_descriptors = visible(target_points, target_descriptors, 1)
        result = self._measure(source_points, source_descriptors, target_points, target_descriptors)
        if result.displacement is None and overlap_hint is not None:
            # 실패/대기 시전은 예상 조준만큼 움직이지 않는다. 같은 프레임 특징의 전체
            # 재검사로 실제 정지만 복구하며, 큰 이동을 조준 예상치 없이 승인하지 않는다.
            stationary = self._measure(previous[3], previous[4], points, descriptors)
            if stationary.displacement is not None and np.linalg.norm(stationary.displacement) < 2:
                result = stationary
        if result.displacement is not None:
            self.previous = current
        return result

    def _measure(self, source_points, source_descriptors, target_points, target_descriptors):
        if min(len(source_descriptors), len(target_descriptors)) < 2:
            return CameraMotion(None, 0, "correspondence_insufficient")
        pairs = self.matcher.knnMatch(source_descriptors, target_descriptors, k=2)
        matches = [pair[0] for pair in pairs if len(pair) == 2 and pair[0].distance < 0.7 * pair[1].distance]
        if len(matches) < self.minimum_matches:
            return CameraMotion(None, len(matches), "correspondence_insufficient", matches=len(matches))
        source = np.float32([source_points[match.queryIdx].pt for match in matches])
        target = np.float32([target_points[match.trainIdx].pt for match in matches])
        transform, accepted = cv.estimateAffinePartial2D(
            source, target, method=cv.RANSAC, ransacReprojThreshold=self.maximum_error * self.scale
        )
        if transform is None or accepted is None:
            return CameraMotion(None, 0, "translation_unverified", matches=len(matches))
        accepted = accepted.ravel().astype(bool)
        count = int(accepted.sum())
        linear = transform[:, :2]
        ratio, linear_error = count / len(matches), float(np.max(np.abs(linear - np.eye(2))))
        details = dict(matches=len(matches), inlier_ratio=ratio, linear_error=linear_error)
        if count < self.minimum_matches or ratio < self.minimum_inlier_ratio or linear_error > 0.03:
            return CameraMotion(None, count, "background_model_rejected", **details)
        errors = np.linalg.norm(source @ linear.T + transform[:, 2] - target, axis=1)
        if float(np.median(errors[accepted])) > self.maximum_error * self.scale:
            return CameraMotion(None, count, "background_error", **details)
        delta = -transform[:, 2] / self.scale
        return CameraMotion(tuple(float(value) for value in delta), count, "background_translation", **details)
