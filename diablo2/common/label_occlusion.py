from __future__ import annotations

import cv2 as cv
import numpy as np


def label_occlusion_mask(frame, *, include_gold=True):
    """어두운 이름표 배경 안의 글자 행을 가림 후보로 찾는다. 아이템 이름/종류 판독은 아니다."""
    if frame.dtype != np.uint8 or frame.ndim != 3 or frame.shape[2] != 3:
        raise ValueError("BGR uint8 label frame required")
    height, width = frame.shape[:2]
    top, bottom = round(height * 0.05), round(height * 0.79)
    view = frame[top:bottom]
    value = cv.cvtColor(view, cv.COLOR_BGR2HSV)[:, :, 2]
    dark_neighborhood = cv.boxFilter((value < 65).astype(np.float32), -1, (9, 9))
    ink = ((value >= 135) & (dark_neighborhood >= 0.2)).astype(np.uint8)
    if not include_gold:
        blue, green, red = (channel.astype(np.int16) for channel in cv.split(view))
        gold = (red - green > 8) & (green - blue > 12) & (red - blue > 35)
        ink[gold] = 0
    rows = cv.morphologyEx(ink, cv.MORPH_CLOSE, np.ones((1, 19), np.uint8))
    joined = cv.dilate(rows, np.ones((3, 1), np.uint8))
    count, _, stats, _ = cv.connectedComponentsWithStats(joined, connectivity=8)
    mask = np.zeros((height, width), np.uint8)
    scale = height / 1140.0
    candidates = list(stats[1:count])
    merged = [(x, y, w, h) for x, y, w, h, _ in candidates if 24 * scale <= w <= 650 * scale and h > 31 * scale]
    if merged:
        # 세로 확장으로 붙은 이름표는 확장 전의 독립 글자 행으로 다시 검사한다.
        row_count, _, row_stats, _ = cv.connectedComponentsWithStats(rows, connectivity=8)
        candidates.extend(
            row
            for row in row_stats[1:row_count]
            if any(x <= row[0] and y <= row[1] and row[0] + row[2] <= x + w and row[1] + row[3] <= y + h for x, y, w, h in merged)
        )
    for x, y, w, h, _ in candidates:
        if not 24 * scale <= w <= 650 * scale or not 10 * scale <= h <= 31 * scale:
            continue
        patch = value[y : y + h, x : x + w]
        bright, dark = np.mean(patch >= 135), np.mean(patch < 65)
        if not 0.06 <= bright <= 0.65 or dark < 0.25:
            continue
        mask[max(top, top + y - 2) : min(bottom, top + y + h + 2), max(0, x - 3) : min(width, x + w + 3)] = 255
    return mask


def visible_template_peak(gray, template, visible, *, minimum_visible=0.6):
    """가린 픽셀을 채우지 않고 현재 보이는 텍스처만으로 국소 정규화 상관을 계산한다."""
    if gray.shape != visible.shape or any(a < b for a, b in zip(gray.shape, template.shape)):
        raise ValueError("matching grayscale patch and visibility mask required")
    target, reference = gray.astype(np.float32), template.astype(np.float32)
    valid = (visible != 0).astype(np.float32)
    ones = np.ones(reference.shape, np.float32)

    def correlation(source, pattern):
        return cv.matchTemplate(source, pattern, cv.TM_CCORR)

    count = correlation(valid, ones)
    safe_count = np.maximum(count, 1)
    sum_target = correlation(target * valid, ones)
    sum_reference = correlation(valid, reference)
    variance_target = correlation(target * target * valid, ones) - sum_target * sum_target / safe_count
    variance_reference = correlation(valid, reference * reference) - sum_reference * sum_reference / safe_count
    numerator = correlation(target * valid, reference) - sum_target * sum_reference / safe_count
    denominator = np.sqrt(np.maximum(variance_target * variance_reference, 0))
    scores = numerator / np.maximum(denominator, 1)
    sufficient = (count >= reference.size * minimum_visible) & (variance_target >= count * 25) & (variance_reference >= count * 25)
    scores[~sufficient] = -1
    _, score, _, location = cv.minMaxLoc(scores)
    x, y = location
    return float(np.clip(score, -1, 1)), location, float(count[y, x] / reference.size)
