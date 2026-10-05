from __future__ import annotations

import json
import math
import re
from dataclasses import dataclass
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.survival import CharacterSurvivalPolicy, ObservationStamp, SafetyObservation


DEFAULT_ASSETS = Path(__file__).resolve().parents[2] / "assets/ui/survival"


def text_mask(image: np.ndarray) -> np.ndarray:
    pixels = image.astype(np.int16)
    return ((pixels.min(axis=2) > 155) & (pixels.max(axis=2) - pixels.min(axis=2) < 85)).astype(np.uint8)


def glyph_parts(mask: np.ndarray) -> list[np.ndarray]:
    columns = np.flatnonzero(mask.any(axis=0))
    parts = []
    for run in np.split(columns, np.where(np.diff(columns) > 1)[0] + 1):
        if len(run):
            rows = np.flatnonzero(mask[:, run].any(axis=1))
            parts.append(mask[rows[0] : rows[-1] + 1, run[0] : run[-1] + 1])
    return parts


def normalize_glyph(mask: np.ndarray) -> np.ndarray:
    height, width = mask.shape
    scale = min(28 / height, 36 / width)
    resized = cv.resize(mask, (max(1, round(width * scale)), max(1, round(height * scale))), interpolation=cv.INTER_NEAREST)
    output = np.zeros((32, 40), dtype=np.uint8)
    top = (32 - resized.shape[0]) // 2
    left = (40 - resized.shape[1]) // 2
    output[top : top + resized.shape[0], left : left + resized.shape[1]] = resized
    return output


def mask_score(left: np.ndarray, right: np.ndarray) -> float:
    union = np.count_nonzero(left | right)
    return float(np.count_nonzero(left & right) / union) if union else 0.0


@dataclass(frozen=True)
class ResourceReading:
    current: int | None = None
    maximum: int | None = None
    reason: str = "unverified"
    score: float | None = None

    @property
    def ratio(self) -> float | None:
        if self.current is None or self.maximum is None:
            return None
        return self.current / self.maximum

    @classmethod
    def from_text(cls, value: str, score: float) -> ResourceReading:
        match = re.fullmatch(r"(0|[1-9][0-9]{0,6})/(0|[1-9][0-9]{0,6})", value)
        if match is None:
            return cls(reason="number_unreadable", score=score)
        current, maximum = map(int, match.groups())
        if maximum <= 0 or current > maximum:
            return cls(reason="number_out_of_range", score=score)
        return cls(current, maximum, "read", score)


@dataclass(frozen=True)
class SlotReading:
    row: int
    column: int
    kind: str | None
    distance: float


@dataclass(frozen=True)
class HudReading:
    layout_id: str
    revision: str
    life: ResourceReading
    mana: ResourceReading
    belt_status: str
    slots: tuple[SlotReading, ...] = ()
    belt_visibility: str = "unknown"

    def belt_contents(self, policy: CharacterSurvivalPolicy) -> dict[int, tuple[str | None, int]] | None:
        if self.belt_status != "read" or len(self.slots) != 16:
            return None
        if {(slot.row, slot.column) for slot in self.slots} != {(row, column) for row in range(1, 5) for column in range(1, 5)}:
            return None
        contents = {}
        for column in policy.belt_columns:
            slots = sorted((slot for slot in self.slots if slot.column == column.column), key=lambda slot: slot.row)
            if len(slots) != 4 or any(slot.kind is None for slot in slots):
                return None
            if any(slot.kind != "empty" for slot in slots[: 4 - column.capacity]):
                return None
            active = slots[4 - column.capacity :]
            if any(slot.kind not in ("empty", column.kind) for slot in active):
                return None
            count = sum(slot.kind == column.kind for slot in active)
            contents[column.column] = (column.kind if count else None, count)
        return contents

    def safety_observation(self, stamp: ObservationStamp) -> SafetyObservation:
        # 최대값은 같은 프레임에서 읽는다. 버프나 장비 변경 전 값을 재사용하지 않는다.
        return SafetyObservation(stamp, self.life.ratio, self.mana.ratio, location="unknown", monsters_clear=None)


class SurvivalHudObserver:
    """검토한 HUD 배치의 후보 관찰만 반환하며 게임 입력과 효과 확인을 하지 않는다."""

    def __init__(self, asset_directory: str | Path = DEFAULT_ASSETS):
        self.asset_directory = Path(asset_directory).resolve()
        self.manifest = json.loads((self.asset_directory / "manifest.json").read_text(encoding="utf-8"))
        if type(self.manifest["schema_version"]) is not int or self.manifest["schema_version"] != 1:
            raise ValueError("Unsupported survival vision schema")
        self.layout_id = self.manifest["layout_id"]
        self.revision = self.manifest["revision"]
        self.frame_size = tuple(self.manifest["frame_size"])
        if len(self.frame_size) != 2 or any(type(value) is not int or value <= 0 for value in self.frame_size):
            raise ValueError("Positive integer reference frame dimensions required")
        if any(not isinstance(value, str) or not value.strip() for value in (self.layout_id, self.revision)):
            raise ValueError("Layout and revision must be explicit")
        settings = self.manifest["matching"]
        for key in (
            "label_score",
            "glyph_score",
            "glyph_margin",
            "belt_anchor_distance",
            "closed_anchor_distance",
            "item_distance",
            "item_margin",
        ):
            value = settings[key]
            if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not 0 < value < 1:
                raise ValueError("Vision matching limits must be finite ratios in (0,1)")
        if type(settings["minimum_glyph_height"]) is not int or settings["minimum_glyph_height"] < 1:
            raise ValueError("Positive integer glyph height required")
        if type(settings["number_max_gap"]) is not int or settings["number_max_gap"] < 1:
            raise ValueError("Positive integer numeric spacing limit required")
        if set(self.manifest["resources"]) != {"life", "mana"}:
            raise ValueError("Both HUD resource references required")
        self.labels = {key: self._load_image(value["template"], gray=True) for key, value in self.manifest["resources"].items()}
        self.label_variants = {
            key: (self.labels[key],) + tuple(self._load_image(name, gray=True) for name in value.get("label_variants", []))
            for key, value in self.manifest["resources"].items()
        }
        self.glyphs = {key: self._load_image(value, gray=True) for key, value in self.manifest["glyphs"].items()}
        if set(self.glyphs) != set("0123456789/") or any(image.shape != (32, 40) for image in self.glyphs.values()):
            raise ValueError("Complete normalized digit/slash atlas required")
        self.belt_anchor = self._load_image(self.manifest["belt"]["anchor_template"])
        self.closed_anchor = self._load_image(self.manifest["belt"]["closed_anchor_template"])
        self.items = {kind: tuple(self._load_image(name) for name in names) for kind, names in self.manifest["belt"]["items"].items()}
        if set(self.items) != {"purple_potion", "town_portal_scroll", "empty"} or any(not values for values in self.items.values()):
            raise ValueError("Complete belt reference classes required")
        belt = self.manifest["belt"]
        for name in ("origin", "step", "slot_size"):
            values = belt[name]
            if len(values) != 2 or any(type(value) is not int or value < (0 if name == "origin" else 1) for value in values):
                raise ValueError("Invalid belt geometry")
        for key, resource in self.manifest["resources"].items():
            label = self._box_shape(resource["label_box"])
            self._box_shape(resource["number_box"])
            search = self._box_shape(resource["label_search_box"])
            if any(image.shape != label for image in self.label_variants[key]) or search[0] < label[0] or search[1] < label[1]:
                raise ValueError("Label dimensions must match the reference box")
        if self.belt_anchor.shape[:2] != self._box_shape(belt["anchor_box"]):
            raise ValueError("Belt anchor dimensions must match its box")
        if self.closed_anchor.shape[:2] != self._box_shape(belt["closed_anchor_box"]):
            raise ValueError("Closed belt anchor dimensions must match its box")
        for row in range(4):
            for column in range(4):
                left, top = belt["origin"][0] + column * belt["step"][0], belt["origin"][1] + row * belt["step"][1]
                self._box_shape((left, top, left + belt["slot_size"][0], top + belt["slot_size"][1]))
        if any(image.shape[:2] != tuple(reversed(belt["slot_size"])) for images in self.items.values() for image in images):
            raise ValueError("Item dimensions must match slots")

    def _box_shape(self, box) -> tuple[int, int]:
        if len(box) != 4 or any(type(value) is not int for value in box):
            raise ValueError("Vision box requires four integers")
        left, top, right, bottom = box
        if not (0 <= left < right <= self.frame_size[0] and 0 <= top < bottom <= self.frame_size[1]):
            raise ValueError("Vision box outside reference frame")
        return bottom - top, right - left

    def _load_image(self, name: str, gray: bool = False) -> np.ndarray:
        path = (self.asset_directory / name).resolve()
        if self.asset_directory not in path.parents:
            raise ValueError("Vision asset must stay within its owner")
        image = cv.imread(str(path), cv.IMREAD_GRAYSCALE if gray else cv.IMREAD_COLOR)
        if image is None:
            raise ValueError(f"Missing vision asset: {name}")
        return (image > 0).astype(np.uint8) if gray else image

    @staticmethod
    def _crop(frame: np.ndarray, box) -> np.ndarray:
        left, top, right, bottom = box
        if not (0 <= left < right <= frame.shape[1] and 0 <= top < bottom <= frame.shape[0]):
            raise ValueError("Vision box outside frame")
        return frame[top:bottom, left:right]

    def _read_number(self, image: np.ndarray) -> ResourceReading:
        settings = self.manifest["matching"]
        mask = text_mask(image)
        columns = np.flatnonzero(mask.any(axis=0))
        breaks = np.flatnonzero(np.diff(columns) > settings["number_max_gap"] + 1)
        if len(breaks):
            mask = mask[:, : columns[breaks[0]] + 1]
        parts = glyph_parts(mask)
        if not 3 <= len(parts) <= 15:
            return ResourceReading(reason="number_unreadable")
        output = []
        scores = []
        for part in parts:
            if part.shape[0] < settings["minimum_glyph_height"]:
                return ResourceReading(reason="number_unreadable")
            glyph = normalize_glyph(part)
            candidates = sorted(((mask_score(glyph, template), key) for key, template in self.glyphs.items()), reverse=True)
            score, character = candidates[0]
            if score < settings["glyph_score"] or score - candidates[1][0] < settings["glyph_margin"]:
                return ResourceReading(reason="glyph_ambiguous", score=score)
            output.append(character)
            scores.append(score)
        return ResourceReading.from_text("".join(output), min(scores))

    def _resource_box(self, frame: np.ndarray, key: str):
        resource = self.manifest["resources"][key]
        search = resource["label_search_box"]
        mask = text_mask(self._crop(frame, search)).astype(np.float32)
        candidates = []
        for template in self.label_variants[key]:
            reference = template.astype(np.float32)
            intersection = cv.matchTemplate(mask, reference, cv.TM_CCORR)
            total = cv.matchTemplate(mask, np.ones_like(reference), cv.TM_CCORR)
            union = total + reference.sum() - intersection
            scores = intersection / np.maximum(union, 1)
            _, _, _, (x, y) = cv.minMaxLoc(scores)
            score = mask_score(mask[y : y + template.shape[0], x : x + template.shape[1]].astype(np.uint8), template)
            candidates.append((score, x, y, template.shape))
        score, x, y, shape = max(candidates)
        if score < self.manifest["matching"]["label_score"]:
            return None
        # 숫자 길이에 따라 문구가 이동하므로 같은 프레임의 문구 끝에서 읽는다.
        gap = resource["number_box"][0] - resource["label_box"][2]
        left, top = search[0] + x + shape[1] + gap, search[1] + y
        return (left, top, search[2], top + shape[0])

    @staticmethod
    def _distance(image: np.ndarray, template: np.ndarray) -> float:
        if image.shape != template.shape:
            return 1.0
        return float(np.abs(image.astype(np.float32) - template.astype(np.float32)).mean() / 255)

    def observe(self, frame: np.ndarray) -> HudReading:
        unknown = ResourceReading(reason="hud_unverified")
        unavailable = HudReading(self.layout_id, self.revision, unknown, unknown, "unverified")
        if not isinstance(frame, np.ndarray) or frame.dtype != np.uint8 or frame.ndim != 3 or frame.shape[2] != 3:
            raise ValueError("Expected uint8 BGR frame")
        if (frame.shape[1], frame.shape[0]) != self.frame_size:
            return HudReading(self.layout_id, self.revision, ResourceReading(reason="layout_unsupported"), unknown, "unverified")
        settings = self.manifest["matching"]
        boxes = {key: self._resource_box(frame, key) for key in self.manifest["resources"]}
        if any(box is None for box in boxes.values()):
            return unavailable
        life = self._read_number(self._crop(frame, boxes["life"]))
        mana = self._read_number(self._crop(frame, boxes["mana"]))
        belt = self.manifest["belt"]
        closed = self._distance(self._crop(frame, belt["closed_anchor_box"]), self.closed_anchor) <= settings["closed_anchor_distance"]
        if self._distance(self._crop(frame, belt["anchor_box"]), self.belt_anchor) > settings["belt_anchor_distance"]:
            return HudReading(
                self.layout_id, self.revision, life, mana, "not_verified_expanded", belt_visibility="closed" if closed else "unknown"
            )
        if closed:
            return HudReading(self.layout_id, self.revision, life, mana, "visibility_ambiguous")
        slots = []
        for row in range(4):
            for column in range(4):
                left = belt["origin"][0] + column * belt["step"][0]
                top = belt["origin"][1] + row * belt["step"][1]
                image = self._crop(frame, (left, top, left + belt["slot_size"][0], top + belt["slot_size"][1]))
                candidates = sorted(
                    (min(self._distance(image, template) for template in templates), kind) for kind, templates in self.items.items()
                )
                distance, kind = candidates[0]
                if distance > settings["item_distance"] or candidates[1][0] - distance < settings["item_margin"]:
                    kind = None
                slots.append(SlotReading(row + 1, column + 1, kind, distance))
        status = "read" if all(slot.kind is not None for slot in slots) else "slot_unverified"
        return HudReading(self.layout_id, self.revision, life, mana, status, tuple(slots), "expanded")
