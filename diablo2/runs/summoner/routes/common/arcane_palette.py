from __future__ import annotations

from diablo2.common.color_palette import Palette


# Arcane 바닥/우주 계열 팔레트는 현재 프로젝트 에셋에서 추출했다.
# - assets/map/act2/arcane_sanctuary/floor.png
# - assets/map/act2/arcane_sanctuary/star.png
# - assets/map/act2/arcane_sanctuary/north_way.png
# OpenCV 입력과 바로 맞추기 위해 BGR 순서를 그대로 사용한다.

ARCANE_FLOOR_GRAY_PALETTE_BGR: Palette = (
    (59, 64, 68),
    (67, 65, 65),
    (107, 108, 109),
    (111, 109, 109),
    (139, 144, 147),
)

# star와 void는 fast path 분리에서 하나의 어두운 공간 계열로 묶는다.
# floor와 너무 가까웠던 중간 회색 샘플은 겹침을 줄이기 위해 제외했다.
ARCANE_STAR_VOID_PALETTE_BGR: Palette = (
    (0, 0, 0),
    (8, 8, 8),
    (11, 11, 11),
    (18, 18, 18),
    (125, 126, 126),
)

# Lab 거리 기반 팔레트 매칭 임계값.
ARCANE_FLOOR_MAX_DISTANCE = 18.0
ARCANE_STAR_VOID_MAX_DISTANCE = 16.0
