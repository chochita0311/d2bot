import unittest
import json
import hashlib
from dataclasses import asdict
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver


class ArcaneObjectiveVisionTests(unittest.TestCase):
    def setUp(self):
        self.observer = ArcaneObjectiveObserver()

    def test_four_glyph_shapes_need_distinct_objects_and_reviewed_layout(self):
        for mode in ("four", "three", "shuffled", "blobs"):
            with self.subTest(mode=mode):
                frame = np.zeros((1140, 1922, 3), np.uint8)
                glyphs = [self.observer.glyphs[i] for i in (0, 1, 4, 5)]
                for index, (part, template) in enumerate(glyphs):
                    if mode == "three" and index == 3:
                        continue
                    x0, y0, x1, y1 = part["crop"]
                    if mode == "shuffled":
                        x0, y0 = (250 + index * 250, 700 - index * 180)
                    h, w = y1 - part["crop"][1], x1 - part["crop"][0]
                    owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
                    mask = cv.imread(str(owner / part["path"]), cv.IMREAD_GRAYSCALE) > 0
                    if mode == "blobs":
                        mask[:] = True
                    patch = frame[y0 : y0 + h, x0 : x0 + w]
                    patch[cv.dilate(mask.astype(np.uint8), np.ones((5, 5), np.uint8)) > 0] = (255, 150, 100)
                    patch[mask] = (255, 245, 235)
                reading = self.observer.observe(frame)
                if mode == "four":
                    self.assertIsNotNone(reading)
                    self.assertEqual("distinct_glyph_shapes_and_slot_layout", reading.evidence_kind)
                    self.assertIsNone(reading.journal_box)
                    np.testing.assert_allclose(reading.ground_point, self.observer.approach_ground, atol=3)
                    shifted = cv.warpAffine(frame, np.float32(((1, 0, 200), (0, 1, 100))), (1922, 1140))
                    moved = self.observer.observe(shifted)
                    np.testing.assert_allclose(np.subtract(moved.ground_point, reading.ground_point), (200, 100), atol=3)
                else:
                    self.assertIsNone(reading)

    def test_actual_labels_and_effects_allow_platform_without_inventing_journal_or_kill(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/platform-labels-combat.png"
        )
        if not path.exists():
            self.fail("Required shared combat platform reference not installed")
        reading = self.observer.observe(cv.imread(str(path)))
        self.assertIsNotNone(reading)
        self.assertIsNone(reading.journal_box)
        self.assertIsNone(reading.journal_score)
        self.assertEqual(4, len(reading.glyph_scores))
        self.assertFalse(hasattr(reading, "dead"))

    def test_glyph_identity_can_change_slots_but_distorted_geometry_cannot(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        for mode in ("different_slots", "translated", "distorted", "duplicate_shape", "three"):
            with self.subTest(mode=mode):
                frame = np.zeros((1140, 1922, 3), np.uint8)
                centers = ((531, 355.5), (673, 262.5), (788, 265), (1074.5, 411.5))
                for index, (glyph, center) in enumerate(zip((0, 1, 3, 5), centers)):
                    if mode == "three" and index == 3:
                        continue
                    if mode == "duplicate_shape" and index == 3:
                        glyph = 0
                    part = self.observer.glyphs[glyph][0]
                    mask = cv.imread(str(owner / part["path"]), cv.IMREAD_GRAYSCALE) > 0
                    h, w = mask.shape
                    x, y = center
                    if mode == "translated":
                        x, y = x + 250, y + 100
                    if mode == "distorted":
                        x, y = 740 + (x - 740) * 1.4, 340 + (y - 340) * 1.4
                    x, y = int(round(x - w / 2)), int(round(y - h / 2))
                    patch = frame[y : y + h, x : x + w]
                    patch[cv.dilate(mask.astype(np.uint8), np.ones((5, 5), np.uint8)) > 0] = (255, 150, 100)
                    patch[mask] = (255, 245, 235)
                reading = self.observer.observe(frame)
                if mode in ("different_slots", "translated"):
                    self.assertIsNotNone(reading)
                    self.assertEqual(4, len(set(reading.slot_indices)))
                    self.assertEqual(4, len(set(glyph for glyph, _ in reading.glyph_scores)))
                    expected = (990, 660) if mode == "translated" else (740, 560)
                    np.testing.assert_allclose(reading.ground_point, expected, atol=3)
                    self.assertLessEqual(reading.layout_residual, 24)
                else:
                    self.assertIsNone(reading)

    def test_actual_changed_slot_layout_and_low_score_negative(self):
        owner = Path(__file__).resolve().parents[1]
        manifest = json.loads(
            (owner / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/objective-runes.json").read_text(encoding="utf-8")
        )
        for reference in manifest["layout_references"]:
            with self.subTest(reference=reference["reference"]):
                path = owner / reference["reference"]
                if not path.exists():
                    self.fail("Required shared slot-layout reference not installed")
                self.assertEqual(reference["sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
                reading = self.observer.observe(cv.imread(str(path)))
                if not reference["platform_observed"]:
                    self.assertIsNone(reading)
                    continue
                self.assertIsNotNone(reading)
                self.assertEqual(reference["glyph_count"], len(reading.glyph_scores))
                self.assertEqual(len(reading.glyph_scores), len(set(reading.slot_indices)))
                self.assertLessEqual(reading.layout_residual, 24)
                self.assertIsNone(reading.journal_box)
                self.assertFalse(hasattr(reading, "dead"))
                np.testing.assert_allclose(reading.ground_point, reference["approach_ground"], atol=1)

    def test_journal_requires_current_surrounding_runes(self):
        frame = np.zeros((1140, 1922, 3), np.uint8)
        h, w = self.observer.template.shape
        frame[400 : 400 + h, 700 : 700 + w] = cv.cvtColor(self.observer.template, cv.COLOR_GRAY2BGR)
        self.assertIsNone(self.observer.observe(frame))
        for x, y in ((400, 300), (550, 250), (900, 230), (1000, 320)):
            cv.rectangle(frame, (x, y), (x + 35, y + 45), (255, 100, 20), -1)
        result = self.observer.observe(frame)
        self.assertEqual("summoner_platform", result.kind)
        self.assertEqual(4, len(result.rune_boxes))
        self.assertEqual("summoner_platform", json.loads(json.dumps(asdict(result)))["kind"])
        # 일지 없는 룬/파란 효과만으로 위치나 보스를 만들지 않는다.
        frame[400 : 400 + h, 700 : 700 + w] = 0
        self.assertIsNone(self.observer.observe(frame))

    def test_actual_platform_reuses_journal_without_claiming_boss_life_state(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/platform-journal.png"
        )
        if not path.exists():
            self.fail("Required shared objective reference not installed")
        reading = self.observer.observe(cv.imread(str(path)))
        self.assertIsNotNone(reading)
        self.assertEqual("summoner_platform", reading.kind)
        self.assertGreaterEqual(reading.journal_score, 0.92)
        self.assertFalse(hasattr(reading, "dead"))
