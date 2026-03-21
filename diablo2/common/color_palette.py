from __future__ import annotations

from functools import lru_cache
from typing import Tuple

import cv2 as cv
import numpy as np


PaletteColor = Tuple[int, int, int]
Palette = Tuple[PaletteColor, ...]


@lru_cache(maxsize=64)
def palette_to_lab(palette: Palette) -> np.ndarray:
    bgr = np.asarray([[list(color) for color in palette]], dtype=np.uint8)
    lab = cv.cvtColor(bgr, cv.COLOR_BGR2LAB)
    return lab.reshape(-1, 3).astype(np.float32)


def palette_distance_map_bgr(image: np.ndarray, palette: Palette) -> np.ndarray:
    if image.size == 0:
        return np.empty(image.shape[:2], dtype=np.float32)
    image_lab = cv.cvtColor(image, cv.COLOR_BGR2LAB).astype(np.float32)
    palette_lab = palette_to_lab(palette)
    flat = image_lab.reshape(-1, 1, 3)
    distances = np.linalg.norm(flat - palette_lab[None, :, :], axis=2)
    return distances.min(axis=1).reshape(image.shape[:2])


def palette_match_ratio(image: np.ndarray, palette: Palette, max_distance: float) -> float:
    if image.size == 0:
        return 0.0
    distance_map = palette_distance_map_bgr(image, palette)
    return float((distance_map <= max_distance).mean())
