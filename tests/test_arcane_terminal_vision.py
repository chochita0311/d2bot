import unittest
import json
from pathlib import Path
from unittest.mock import patch

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.terminal_vision import ArcaneTerminalObserver, TerminalStabilityTracker


class ArcaneTerminalVisionTests(unittest.TestCase):
    def test_gray_front_lighting_tuning_keeps_three_objects_and_strict_composition(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal"
        gray_front = "templates/terminal/small-stone-chest-gray-front.png"
        frame = cv.imread(str(owner / "north-gray-front-lighting-hidden.png"))
        observer = ArcaneTerminalObserver()
        reading = observer.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual((120, 16000), (reading.minimum_separation, reading.minimum_area))
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        for part, score in zip(reading.part_ids, reading.scores):
            self.assertGreaterEqual(score, 0.91 if part == gray_front else 0.92)
            if part == gray_front:
                self.assertLess(score, 0.92)
        for left, top, right, bottom in ((80, 380, 190, 455), (725, 460, 805, 535), (740, 600, 810, 685)):
            missing = frame.copy()
            missing[top:bottom, left:right] = 0
            self.assertIsNone(ArcaneTerminalObserver().observe(missing))
        for suffix in ("map-on", "unconfirmed"):
            self.assertIsNone(ArcaneTerminalObserver().observe(cv.imread(str(owner / ("north-gray-front-lighting-" + suffix + ".png")))))
        self.assertIsNotNone(ArcaneTerminalObserver().observe(cv.imread(str(owner / "north-gray-front-lighting-settled.png"))))
        baseline = ArcaneTerminalObserver()
        for parts in [baseline.parts] + [parts for _, parts in baseline.alternate_layouts]:
            for rule, _, _ in parts:
                if rule["path"] == gray_front:
                    rule["threshold"] = 0.92
        self.assertIsNone(baseline.observe(frame))
        data = json.loads((owner.parent.parent / "terminal-patterns.json").read_text(encoding="utf-8"))
        for minimum in (None, 0.90):
            data["chest_detail_minimum_scores"] = {} if minimum is None else {gray_front: minimum}
            with patch("diablo2.runs.summoner.terminal_vision.Path.read_text", return_value=json.dumps(data)):
                with self.assertRaisesRegex(ValueError, "below reviewed minimum"):
                    ArcaneTerminalObserver()

    def test_lower_floor_spoke_and_two_distinct_chests_survive_clipped_lower_body_but_require_each_landmark(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        frame = cv.imread(str(owner / "references/terminal/north-two-chests-lower-floor-spoke.png"))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertIn("templates/terminal/engraved-lower-floor-spoke.png", reading.part_ids)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual((6, 19), (len(self.vision.chest_parts), len(self.vision.chest_detail_parts)))
        np.testing.assert_array_equal(cv.imread(str(owner / "templates/terminal/engraved-lower-floor-spoke.png")), frame[833:876, 970:1024])
        for left, top, right, bottom in ((950, 817, 1040, 900), (1180, 680, 1320, 810), (620, 700, 720, 790)):
            missing = frame.copy()
            missing[top:bottom, left:right] = 0
            self.assertIsNone(self.vision.observe(missing))
        self.assertIsNone(
            self.vision.observe(cv.imread(str(owner / "references/terminal/north-two-chests-lower-floor-spoke-obscured.png")))
        )

    def test_two_arched_chests_and_small_box_require_each_current_object(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        frame = cv.imread(str(owner / "references/terminal/north-two-arched-chests-small-box.png"))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/dark-arched-chest-clean-front.png", reading.part_ids)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(6, len(self.vision.chest_parts))
        self.assertEqual(19, len(self.vision.chest_detail_parts))
        template = cv.imread(str(owner / "templates/terminal/dark-arched-chest-clean-front.png"))
        np.testing.assert_array_equal(template, frame[577:610, 887:925])
        for box in ((325, 555, 398, 620), (880, 540, 985, 625), (885, 680, 990, 775)):
            missing = frame.copy()
            x0, y0, x1, y1 = box
            missing[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(missing))

    def test_corpse_covered_gray_box_front_requires_three_separate_current_chests(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        frame = cv.imread(str(owner / "references/terminal/north-corpse-covered-gray-box-front.png"))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/small-stone-chest-gray-front.png", reading.part_ids)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        template = cv.imread(str(owner / "templates/terminal/small-stone-chest-gray-front.png"))
        np.testing.assert_array_equal(template, frame[599:624, 1111:1151])
        for x0, y0, x1, y1 in ((500, 550, 607, 640), (1090, 700, 1190, 790), (1100, 570, 1180, 635)):
            missing = frame.copy()
            missing[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(missing))

    def test_map_overlay_views_require_three_current_chests_without_relaxing_scores(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal"
        covered = cv.imread(str(owner / "north-map-overlay-on.png"))
        clear = cv.imread(str(owner / "north-map-overlay-off.png"))
        for source in (covered, clear, covered):
            reading = self.vision.observe(source)
            self.assertIsNotNone(reading)
            self.assertEqual("current_three_parts", reading.provenance)
            self.assertEqual(3, len(set(reading.evidence_boxes)))
            self.assertTrue(all(score >= 0.92 for score in reading.scores))
            for x0, y0, x1, y1 in ((520, 560, 620, 640), (1100, 710, 1190, 790), (1080, 560, 1210, 655)):
                missing = source.copy()
                missing[y0:y1, x0:x1] = 0
                self.assertIsNone(self.vision.observe(missing))

    def test_metal_chest_and_two_gray_box_edges_require_three_physical_objects(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-metal-chest-two-gray-box-edges.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertEqual(2, reading.part_ids.count("templates/terminal/small-stone-chest-right-edge.png"))
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(6, len(self.vision.chest_parts))
        for x0, y0, x1, y1 in ((184, 472, 289, 568), (776, 490, 850, 560), (776, 635, 850, 706)):
            missing = frame.copy()
            missing[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(missing))

    def test_speed_review_sources_require_each_of_three_physical_chests(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal"
        cases = (
            (
                "north-metal-chests-gray-stone-box.png",
                "small-stone-box-gray-body.png",
                ((1110, 450, 1230, 545), (1670, 440, 1810, 545), (1710, 615, 1785, 680)),
            ),
            (
                "north-dark-metal-chests-square-box.png",
                "dark-arched-chest-front-left.png",
                ((713, 250, 815, 337), (1290, 400, 1385, 483), (1295, 260, 1370, 335)),
            ),
        )
        for name, part, objects in cases:
            with self.subTest(name=name):
                frame = cv.imread(str(owner / name))
                reading = self.vision.observe(frame)
                self.assertIsNotNone(reading)
                self.assertIn("templates/terminal/" + part, reading.part_ids)
                self.assertTrue(all(score >= 0.92 for score in reading.scores))
                self.assertEqual(3, len(set(reading.evidence_boxes)))
                for x0, y0, x1, y1 in objects:
                    missing = frame.copy()
                    missing[y0:y1, x0:x1] = 0
                    self.assertIsNone(self.vision.observe(missing))

    def test_successive_current_layouts_follow_small_and_large_camera_shifts_and_reject_a_missing_object(self):
        root = Path(__file__).resolve().parents[1]
        frame = cv.imread(
            str(root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-metal-chest-two-stone-boxes.png")
        )
        first = self.vision.observe(frame)
        self.assertIsNotNone(first)
        for shift in (2, 12):
            moved = cv.warpAffine(frame, np.float32([[1, 0, shift], [0, 1, 0]]), (1922, 1140))
            current = self.vision.observe(moved)
            self.assertIsNotNone(current)
            np.testing.assert_allclose(current.point, (first.point[0] + shift, first.point[1]), atol=0.01)
            self.assertTrue(all(current.current_parts))
            x0, y0, x1, y1 = current.evidence_boxes[0]
            moved[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(moved))

    def test_metal_chest_and_two_stone_boxes_require_three_distinct_current_objects(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-metal-chest-two-stone-boxes.png"
        frame = cv.imread(str(path))
        self.assertIsNotNone(frame)
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/small-stone-chest-northwest-face.png", reading.part_ids)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_overlapping_labels_allow_new_layout_without_inventing_the_old_missing_part_score(self):
        root = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references"
        paths = (
            root / "terminal/north-gold-labels-full.png",
            root / "terminal/north-overlapping-gold-labels.png",
        )
        if not all(path.exists() for path in paths):
            self.fail("Required shared overlapping-label references unavailable")
        before, after = (cv.imread(str(path)) for path in paths)
        full = self.vision.observe(before)
        self.assertIsNotNone(full)
        current = self.vision.observe(after)
        self.assertIsNotNone(current)
        self.assertIn("templates/terminal/small-stone-box-gray-body.png", current.part_ids)
        self.assertTrue(all(current.current_parts))
        self.assertTrue(all(score >= 0.92 for score in current.scores))
        tracker = TerminalStabilityTracker(self.vision)
        tracker.update(before, full, 0, None)
        tracked = tracker.update(after, None, 0.2, (0, 0))
        self.assertEqual((True, False, True), tracked.current_parts)
        self.assertIsNone(tracked.scores[1])
        self.assertGreaterEqual(min(score for score in tracked.scores if score is not None), 0.92)
        self.assertGreaterEqual(tracked.visible_fractions[2], 0.6)
        self.assertEqual(0, tracked.reference_observed_at)
        hidden = after.copy()
        x0, y0, x1, y1 = full.evidence_boxes[2]
        hidden[y0:y1, x0:x1] = 0
        self.assertIsNone(tracker.update(hidden, None, 0.3, (0, 0)))
        self.assertIsNone(tracker.update(after, None, 2.01, (0, 0)))

    def test_open_and_closed_chests_require_three_distinct_current_objects(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-open-chest-terminal.png"
        if not path.exists():
            self.fail("Required shared open-chest terminal unavailable")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/dark-open-chest-southeast-face.png", reading.part_ids)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        for x0, y0, x1, y1 in ((140, 450, 240, 540), (710, 450, 815, 540), (710, 595, 815, 690)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_reviewed_floor_appearance_gap_keeps_only_two_current_relocalized_scores(self):
        root = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references"
        paths = (
            root / "terminal/north-dark-chests-gold-filigree.png",
            root / "terminal/north-floor-effect-relocalized.png",
        )
        if not all(path.exists() for path in paths):
            self.fail("Required shared floor-appearance transition unavailable")
        before, after = (cv.imread(str(path)) for path in paths)
        full = self.vision.observe(before)
        self.assertIsNotNone(full)
        self.assertIsNone(self.vision.observe(after))
        tracker = TerminalStabilityTracker(self.vision)
        tracker.update(before, full, 0, None)
        self.assertIsNone(tracker.update(after, None, 0.3, None))
        tracked = tracker.update(after, None, 0.5, (0, 0))
        self.assertEqual("current_relocalized_two_parts", tracked.provenance)
        self.assertEqual((True, True, False), tracked.current_parts)
        self.assertIsNone(tracked.scores[2])
        self.assertIsNone(tracked.visible_fractions[2])
        np.testing.assert_allclose(tracked.point, (966, 582.166667), atol=1)
        self.assertEqual(0, tracked.reference_observed_at)
        self.assertIsNone(tracker.update(after, None, 2.01, (0, 0)))

    def test_dark_chests_and_filigree_floor_require_three_distinct_current_parts(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-dark-chests-gold-filigree.png"
        if not path.exists():
            self.fail("Required shared dark-chest/filigree reference unavailable")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/gold-filigree-left-tip.png", reading.part_ids)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_two_existing_metal_chests_and_small_box_form_three_current_parts(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-metal-chests-small-box.png"
        if not path.exists():
            self.fail("Required shared metal-chest/small-box reference unavailable")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        tracked = self.vision.confirm_static_parts(frame, reading)
        self.assertIsNotNone(tracked)
        self.assertGreaterEqual(sum(tracked.current_parts), 2)
        self.assertTrue(any(fraction is not None and fraction < 1 for fraction in tracked.visible_fractions))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_dense_gold_labels_use_clean_floor_tip_and_two_distinct_arched_chests(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-gold-labels.png"
        if not path.exists():
            self.fail("Required shared dense-label terminal reference unavailable")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/gold-floor-right-tip.png", reading.part_ids)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_engraved_floor_and_two_chests_handle_the_occluded_third_chest(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-left-chest-occluded.png"
        if not path.exists():
            self.fail("Required shared terminal occlusion reference unavailable")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/engraved-stone-plate.png", reading.part_ids)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_dark_arched_chests_and_small_box_require_all_three_current_objects(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-dark-arched-chests.png"
        if not path.exists():
            self.fail("Required shared dark arched chest reference unavailable")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("nonboss_chest_platform", reading.kind)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            if x0 == 748:
                alternate = self.vision.observe(hidden)
                self.assertIsNotNone(alternate)
                self.assertIn("templates/terminal/engraved-stone-plate.png", alternate.part_ids)
                hidden[660:700, 1065:1111] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_square_stone_styles_require_three_objects_with_labels_on(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-stone-box-terminal.png"
        if not path.exists():
            self.fail("Required shared square stone reference unavailable")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertEqual("nonboss_chest_platform", reading.kind)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def setUp(self):
        self.vision = ArcaneTerminalObserver()

    def scene(self, offsets=((700, 400), (1268, 549), (1282, 397))):
        frame = np.zeros((1140, 1922, 3), np.uint8)
        for (_, native, _), (x, y) in zip(self.vision.parts, offsets):
            h, w = native.shape
            frame[y : y + h, x : x + w] = cv.cvtColor(native, cv.COLOR_GRAY2BGR)
        return frame

    def test_three_distinct_spatial_parts_follow_current_frame_translation(self):
        first = self.vision.observe(self.scene())
        second = self.vision.observe(self.scene(((500, 500), (1068, 649), (1082, 497))))
        self.assertEqual("nonboss_chest_trio", first.kind)
        np.testing.assert_allclose(np.array(second.point) - first.point, (-200, 100), atol=1)
        self.assertEqual(3, len(first.evidence_boxes))

    def test_inflight_cast_can_track_one_current_part_for_approach_but_never_arrival(self):
        frame = self.scene()
        full = self.vision.observe(frame)
        shifted = cv.warpAffine(frame, np.float32(((1, 0, 700), (0, 1, -150))), (1922, 1140))
        self.assertIsNone(self.vision.observe(shifted))
        tracker = TerminalStabilityTracker(self.vision)
        tracker.update(frame, full, 0, None)
        self.assertIsNone(tracker.update(shifted, None, 0.2, (-700, 150)))
        self.assertEqual("current_motion_approach_only", tracker.approach.provenance)
        self.assertEqual((True, False, False), tracker.approach.current_parts)
        self.assertIsNone(tracker.approach.scores[1])
        np.testing.assert_allclose(tracker.approach.point, np.array(full.point) + (700, -150), atol=1)
        self.assertIsNone(tracker.update(shifted, None, 0.9, (0, 0)))
        self.assertEqual(0, tracker.approach.reference_observed_at)
        self.assertIsNone(tracker.update(shifted, None, 2.01, (0, 0)))
        self.assertIsNone(tracker.approach)
        for displacement, current in ((None, shifted), ((-700, 150), np.zeros_like(frame))):
            tracker.update(frame, full, 3, None)
            self.assertIsNone(tracker.update(current, None, 3.1, displacement))
            self.assertIsNone(tracker.approach)

    def test_moving_parts_must_agree_with_the_same_background_translation(self):
        frame = self.scene()
        full = self.vision.observe(frame)
        shifted = cv.warpAffine(frame, np.float32(((1, 0, 100), (0, 1, 100))), (1922, 1140))
        x0, y0, x1, y1 = full.evidence_boxes[0]
        shifted[y0 + 100 : y1 + 100, x0 + 100 : x1 + 100] = 0
        shifted[y0 + 100 : y1 + 100, x0 + 107 : x1 + 107] = frame[y0:y1, x0:x1]
        self.assertIsNone(self.vision.confirm_approach_parts(shifted, full, (-100, -100)))

    def test_two_motion_tracked_parts_require_current_settling_before_arrival(self):
        frame = self.scene()
        full = self.vision.observe(frame)
        shifted = cv.warpAffine(frame, np.float32(((1, 0, 300), (0, 1, -150))), (1922, 1140))
        x0, y0, x1, y1 = full.evidence_boxes[2]
        shifted[y0 - 150 : y1 - 150, x0 + 300 : x1 + 300] = 0
        tracker = TerminalStabilityTracker(self.vision)
        tracker.update(frame, full, 0, None)
        self.assertIsNone(tracker.update(shifted, None, 0.2, (-300, 150)))
        self.assertEqual((True, True, False), tracker.approach.current_parts)
        self.assertIsNone(tracker.update(shifted, None, 0.3, (2, 0)))
        tracked = tracker.update(shifted, None, 0.4, (0, 0))
        self.assertEqual("current_motion_settled_two_parts", tracked.provenance)
        self.assertEqual((True, True, False), tracked.current_parts)
        self.assertIsNone(tracked.scores[2])
        self.assertEqual(0, tracked.reference_observed_at)
        self.assertIsNone(tracker.update(shifted, None, 2.01, (0, 0)))
        self.assertIsNone(tracker.approach)

    def test_lost_background_requires_relocalized_parts_then_independent_static_motion(self):
        frame = self.scene()
        full = self.vision.observe(frame)
        shifted = cv.warpAffine(frame, np.float32(((1, 0, 300), (0, 1, -150))), (1922, 1140))
        x0, y0, x1, y1 = full.evidence_boxes[2]
        shifted[y0 - 150 : y1 - 150, x0 + 300 : x1 + 300] = 0
        self.assertIsNone(self.vision.observe(shifted))
        tracker = TerminalStabilityTracker(self.vision)
        tracker.update(frame, full, 0, None)
        self.assertIsNone(tracker.update(shifted, None, 0.3, None))
        self.assertEqual("current_landmark_approach_only", tracker.approach.provenance)
        self.assertEqual((True, True, False), tracker.approach.current_parts)
        self.assertIsNone(tracker.approach.scores[2])
        np.testing.assert_allclose(tracker.approach.point, np.array(full.point) + (300, -150), atol=1)
        tracked = tracker.update(shifted, None, 0.9, (0, 0))
        self.assertEqual("current_relocalized_two_parts", tracked.provenance)
        self.assertIsNone(tracked.scores[2])
        self.assertEqual(0, tracker.approach.reference_observed_at)
        self.assertIsNone(tracker.update(shifted, None, 2.01, (0, 0)))
        self.assertIsNone(tracker.approach)
        tracker.update(frame, full, 3, None)
        self.assertIsNone(tracker.update(shifted, None, 3.1, None))
        self.assertIsNone(tracker.update(shifted, None, 3.2, None))
        self.assertIsNone(tracker.update(shifted, None, 3.3, (3, 0)))

    def test_landmark_relocalization_rejects_one_part_or_disagreeing_translations(self):
        frame = self.scene()
        full = self.vision.observe(frame)
        current = np.zeros_like(frame)
        for index, delta in ((0, (300, -150)), (1, (330, -150))):
            x0, y0, x1, y1 = full.evidence_boxes[index]
            dx, dy = delta
            current[y0 + dy : y1 + dy, x0 + dx : x1 + dx] = frame[y0:y1, x0:x1]
        self.assertIsNone(self.vision.confirm_relocalized_parts(current, full))
        x0, y0, x1, y1 = full.evidence_boxes[1]
        current[y0 - 150 : y1 - 150, x0 + 330 : x1 + 330] = 0
        self.assertIsNone(self.vision.confirm_relocalized_parts(current, full))

    def test_missing_one_part_or_collinear_trio_is_not_terminal(self):
        scene = self.scene()
        scene[390:470, 1280:1380] = 0
        self.assertIsNone(self.vision.observe(scene))
        self.assertIsNone(self.vision.observe(self.scene(((500, 400), (800, 400), (1100, 400)))))
        self.assertIsNone(self.vision.observe(np.zeros((1140, 1922, 3), np.uint8)))

    def test_reviewed_private_source_replays_when_present(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-three-chest-clean.png"
        )
        if not path.exists():
            self.fail("Required shared calibration source not installed")
        reading = self.vision.observe(cv.imread(str(path)))
        self.assertIsNotNone(reading)
        np.testing.assert_allclose(reading.point, (1056.5, 299.166667), atol=1)

    def test_label_rich_source_uses_three_current_unmasked_chest_details(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-three-chest-end.png"
        )
        if not path.exists():
            self.fail("Required shared occlusion source not installed")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual((1.0, 1.0, 1.0), reading.visible_fractions)
        for x0, y0, x1, y1 in ((825, 600, 930, 690), (1410, 610, 1480, 670), (1395, 745, 1500, 830)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_layout_mismatch_is_not_silently_resized(self):
        with self.assertRaises(ValueError):
            self.vision.observe(np.zeros((753, 1267, 3), np.uint8))

    def test_reviewed_two_chest_junction_is_not_a_terminal(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-two-chest-junction.png"
        )
        if not path.exists():
            self.fail("Required shared junction negative not installed")
        frame = cv.imread(str(path))
        self.assertIsNone(self.vision.observe(frame))

    def test_reviewed_floor_and_two_chests_require_all_three_parts(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-landmarks.png"
        )
        if not path.exists():
            self.fail("Required shared terminal variant not installed")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("nonboss_chest_platform", reading.kind)
        self.assertEqual(3, len(reading.evidence_boxes))
        frame[704:846, 890:1108] = 0
        self.assertIsNone(self.vision.observe(frame))

    def test_upper_view_floor_uses_distinct_chests_and_requires_the_floor(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-floor-view.png"
        )
        if not path.exists():
            self.fail("Required shared upper terminal view not installed")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("nonboss_chest_platform", reading.kind)
        np.testing.assert_allclose(reading.point, (1060.5, 316.166667), atol=1)
        frame[287:429, 976:1194] = 0
        self.assertIsNone(self.vision.observe(frame))

    def test_same_appearance_requires_three_separate_objects_not_duplicate_matches(self):
        frame = np.zeros((1140, 1922, 3), np.uint8)
        parts = next(parts for _, parts in self.vision.alternate_layouts if parts[0][0]["id"] == "square-chest-left-instance")
        for (_, native, _), (x, y) in zip(parts, ((500, 400), (1076, 400), (1067, 550))):
            h, w = native.shape
            frame[y : y + h, x : x + w] = cv.cvtColor(native, cv.COLOR_GRAY2BGR)
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        frame[400:451, 500:560] = 0
        self.assertIsNone(self.vision.observe(frame))

    def test_reviewed_three_square_chest_variation_and_missing_object(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-three-square-chests.png"
        )
        if not path.exists():
            self.fail("Required shared square-chest terminal variation not installed")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("nonboss_chest_platform", reading.kind)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        np.testing.assert_allclose(reading.point, (569.5, 549.666667), atol=2)
        frame[476:527, 159:219] = 0
        self.assertIsNone(self.vision.observe(frame))

    def test_tracking_requires_recent_full_layout_current_two_parts_and_unbroken_static_motion(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-floor-view.png"
        )
        if not path.exists():
            self.fail("Required shared upper terminal view not installed")
        frame = cv.imread(str(path))
        full = self.vision.observe(frame)
        hidden = frame.copy()
        hidden[287:429, 976:1194] = 0
        tracker = TerminalStabilityTracker(self.vision)
        self.assertIsNone(tracker.update(hidden, None, 0, (0, 0)))
        tracker.update(frame, full, 0, None)
        tracked = tracker.update(hidden, None, 0.8, (0, 0))
        self.assertEqual("current_static_two_parts", tracked.provenance)
        self.assertEqual((False, True, True), tracked.current_parts)
        self.assertIsNone(tracked.scores[0])
        self.assertEqual(0, tracked.reference_observed_at)
        self.assertIsNone(tracker.update(hidden, None, 2.01, (0, 0)))
        for displacement in (None, (2, 0), (300, -150)):
            tracker.update(frame, full, 3, None)
            self.assertIsNone(tracker.update(hidden, None, 3.1, displacement))
            tracked = tracker.update(hidden, None, 3.2, (0, 0))
            expected = "current_motion_settled_two_parts" if displacement == (2, 0) else "current_relocalized_two_parts"
            self.assertEqual(expected, tracked.provenance)
        tracker.update(frame, full, 4, None)
        hidden[253:329, 1287:1376] = 0
        self.assertIsNone(tracker.update(hidden, None, 4.1, (0, 0)))

    def test_reviewed_dark_chest_variation_requires_three_distinct_objects(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-dark-stone-chest-terminal.png"
        )
        if not path.exists():
            self.fail("Required shared dark-chest terminal variation not installed")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("nonboss_chest_platform", reading.kind)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        for x0, y0, x1, y1 in ((797, 613, 888, 691), (1373, 757, 1464, 835), (1385, 640, 1453, 692)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))
        self.assertIsNone(self.vision.observe(np.zeros_like(frame)))

    def test_bright_chest_face_keeps_three_distinct_current_objects(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-lit-metal-chest-face.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        for x0, y0, x1, y1 in ((480, 625, 580, 710), (1055, 625, 1150, 710), (1065, 775, 1160, 860)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_reviewed_detail_composition_requires_three_whole_chests_in_each_current_view(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal"
        self.assertEqual(6, len(self.vision.chest_parts))
        self.assertEqual(19, len(self.vision.chest_detail_parts))
        for rule, _, _ in self.vision.chest_detail_parts:
            minimum = 0.91 if rule["path"] == "templates/terminal/small-stone-chest-gray-front.png" else 0.92
            self.assertGreaterEqual(rule["threshold"], minimum)
        for suffix in ("", "-settled"):
            with self.subTest(suffix=suffix):
                frame = cv.imread(str(owner / ("north-dark-chests-small-box" + suffix + ".png")))
                reading = self.vision.observe(frame)
                self.assertIsNotNone(reading)
                self.assertEqual("current_three_parts", reading.provenance)
                self.assertEqual((1.0, 1.0, 1.0), reading.visible_fractions)
                self.assertEqual((120, 16000), (reading.minimum_separation, reading.minimum_area))
                repeated = self.vision.observe(frame)
                self.assertEqual((120, 16000), (repeated.minimum_separation, repeated.minimum_area))
                for x0, y0, x1, y1 in ((395, 585, 500, 675), (975, 585, 1075, 675), (980, 740, 1055, 805)):
                    hidden = frame.copy()
                    hidden[y0:y1, x0:x1] = 0
                    self.assertIsNone(self.vision.observe(hidden))

    def test_dark_metal_pair_and_small_box_require_each_physical_chest_in_both_views(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal"
        for suffix in ("", "-settled"):
            with self.subTest(suffix=suffix):
                frame = cv.imread(str(owner / ("north-dark-metal-pair-small-box" + suffix + ".png")))
                reading = self.vision.observe(frame)
                self.assertIsNotNone(reading)
                self.assertEqual("current_three_parts", reading.provenance)
                self.assertTrue(all(score >= 0.92 for score in reading.scores))
                for x0, y0, x1, y1 in ((350, 565, 430, 635), (925, 570, 1020, 645), (915, 695, 1020, 785)):
                    hidden = frame.copy()
                    hidden[y0:y1, x0:x1] = 0
                    self.assertIsNone(self.vision.observe(hidden))

    def test_three_dark_stone_boxes_are_not_a_two_box_junction(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-three-dark-stone-boxes.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_item_labels_use_current_visible_texture_and_still_require_each_object(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-item-label-occlusion.png"
        )
        if not path.exists():
            self.fail("Required shared item-label occlusion source not installed")
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_masked_three_parts", reading.provenance)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertTrue(0.6 <= min(reading.visible_fractions) < 0.7)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        for x0, y0, x1, y1 in reading.evidence_boxes:
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_mixed_full_chest_bodies_require_three_current_physical_objects(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-mixed-small-and-arched-chests.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertEqual(2, reading.part_ids.count("templates/terminal/square-chest.png"))
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        for x0, y0, x1, y1 in ((550, 670, 640, 740), (1100, 650, 1230, 750), (1125, 815, 1210, 885)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))
        tracker = TerminalStabilityTracker(self.vision)
        tracker.update(frame, reading, 0, None)
        partial = frame.copy()
        partial[670:740, 550:640] = 0
        tracked = tracker.update(partial, None, 0.2, (0, 0))
        self.assertIsNotNone(tracked)
        self.assertEqual(2, sum(tracked.current_parts))
        self.assertEqual(0, tracked.reference_observed_at)
        self.assertIsNone(tracker.update(partial, None, 2.01, (0, 0)))

    def test_three_arched_chests_reuse_reviewed_faces_without_expanding_body_pool(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-three-arched-chest-faces.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertEqual(6, len(self.vision.chest_parts))
        for x0, y0, x1, y1 in ((1110, 300, 1230, 400), (1680, 300, 1810, 400), (1680, 440, 1810, 550)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_corpse_covered_small_chest_uses_only_its_clear_face(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-corpse-covered-small-chest.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertIn("templates/terminal/small-stone-chest-clear-left-face.png", reading.part_ids)
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertEqual(6, len(self.vision.chest_parts))
        for x0, y0, x1, y1 in ((550, 670, 625, 740), (1100, 640, 1220, 750), (1100, 790, 1230, 885)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_corpse_covered_stone_box_and_metal_chest_reuse_three_current_objects(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        frame = cv.imread(str(owner / "references/terminal/north-corpse-covered-stone-box-and-metal-chest.png"))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertIn("templates/terminal/square-stone-chest-left.png", reading.part_ids)
        self.assertEqual(6, len(self.vision.chest_parts))
        for x0, y0, x1, y1 in ((170, 480, 255, 550), (750, 480, 835, 550), (725, 605, 840, 705)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_fire_obscured_terminal_requires_new_clear_current_objects(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal"
        self.assertIsNone(self.vision.observe(cv.imread(str(owner / "north-fire-obscured.png"))))
        reading = self.vision.observe(cv.imread(str(owner / "north-fire-cleared.png")))
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))

    def test_two_stone_boxes_and_dark_metal_face_require_three_physical_objects(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        frame = cv.imread(str(owner / "references/terminal/north-two-stone-boxes-dark-metal-face.png"))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(6, len(self.vision.chest_parts))
        for x0, y0, x1, y1 in ((550, 435, 635, 510), (1120, 435, 1200, 510), (1110, 565, 1220, 660)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_two_metal_chests_and_engraved_floor_require_all_three_current_landmarks(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        for name in ("north-metal-pair-engraved-floor.png", "north-metal-pair-engraved-floor-settled.png"):
            with self.subTest(name=name):
                frame = cv.imread(str(owner / "references/terminal" / name))
                reading = self.vision.observe(frame)
                self.assertIsNotNone(reading)
                self.assertEqual("current_three_parts", reading.provenance)
                self.assertEqual(3, len(set(reading.evidence_boxes)))
                self.assertEqual(6, len(self.vision.chest_parts))
                scores = dict(zip(reading.part_ids, reading.scores))
                self.assertGreaterEqual(scores["templates/terminal/engraved-stone-plate.png"], 0.91)
                for part, score in scores.items():
                    if part != "templates/terminal/engraved-stone-plate.png":
                        self.assertGreaterEqual(score, 0.92)
                if "settled" in name:
                    self.assertLess(scores["templates/terminal/engraved-stone-plate.png"], 0.92)
                for x0, y0, x1, y1 in ((735, 560, 815, 645), (990, 570, 1100, 655), (985, 705, 1100, 805)):
                    hidden = frame.copy()
                    hidden[y0:y1, x0:x1] = 0
                    self.assertIsNone(self.vision.observe(hidden))

    def test_fixed_floor_layouts_reject_shared_nonterminal_native_references(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references"
        checked = 0
        for group in ("hub", "entry", "navigation", "summoner"):
            for path in sorted((owner / group).glob("*.png")):
                frame = cv.imread(str(path))
                if frame.shape != (1140, 1922, 3):
                    continue
                with self.subTest(path=str(path)):
                    self.assertIsNone(self.vision.observe(frame))
                checked += 1
        self.assertGreaterEqual(checked, 36)

    def test_stone_plate_and_two_distinct_chests_reuse_current_reviewed_parts(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-stone-plate-arched-and-square.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertIn("templates/terminal/engraved-stone-plate.png", reading.part_ids)
        self.assertEqual(6, len(self.vision.chest_parts))
        for x0, y0, x1, y1 in ((1270, 350, 1370, 425), (1535, 345, 1660, 455), (1550, 505, 1645, 585)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))

    def test_dark_arched_clean_roof_stays_in_reviewed_three_object_layout(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-small-boxes-and-dark-arched-roof.png"
        )
        frame = cv.imread(str(path))
        reading = self.vision.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("current_three_parts", reading.provenance)
        self.assertIn("templates/terminal/dark-arched-chest-clean-roof.png", reading.part_ids)
        self.assertEqual(3, len(set(reading.evidence_boxes)))
        self.assertTrue(all(score >= 0.92 for score in reading.scores))
        self.assertEqual(6, len(self.vision.chest_parts))
        for x0, y0, x1, y1 in ((600, 455, 700, 540), (1160, 445, 1290, 540), (1170, 605, 1260, 685)):
            hidden = frame.copy()
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden))
