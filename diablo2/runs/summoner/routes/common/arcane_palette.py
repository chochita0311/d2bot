from __future__ import annotations

from diablo2.common.color_palette import Palette


# Derived from the current project assets:
# - assets/map/act2/arcane_sanctuary/floor.png
# - assets/map/act2/arcane_sanctuary/star.png
# - assets/map/act2/arcane_sanctuary/north_way.png
#
# Stored in BGR order because OpenCV images are BGR.

ARCANE_FLOOR_GRAY_PALETTE_BGR: Palette = (
    (30, 34, 35),
    (47, 45, 44),
    (59, 64, 68),
    (67, 65, 65),
    (86, 88, 89),
    (90, 88, 88),
    (107, 108, 109),
    (111, 109, 109),
    (139, 144, 147),
)

# Star and void are treated as one dark-space family for fast path separation.
ARCANE_STAR_VOID_PALETTE_BGR: Palette = (
    (0, 0, 0),
    (8, 8, 8),
    (11, 11, 11),
    (18, 18, 18),
    (30, 31, 31),
    (52, 53, 53),
    (80, 82, 82),
    (125, 126, 126),
)

# Initial distance thresholds for Lab-space palette matching.
ARCANE_FLOOR_MAX_DISTANCE = 18.0
ARCANE_STAR_VOID_MAX_DISTANCE = 16.0
