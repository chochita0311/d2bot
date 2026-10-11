import unittest
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.terminal_vision import ArcaneTerminalObserver, TerminalStabilityTracker


class ArcaneTerminalVisionTests(unittest.TestCase):
    def test_overlapping_gold_labels_keep_two_current_parts_without_inventing_the_third(self):
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
        self.assertIsNone(self.vision.observe(after))
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
        for x0, y0, x1, y1 in reading.evidence_boxes:
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

    def test_old_label_contaminated_source_is_not_a_positive_template(self):
        path = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/terminal/north-three-chest-end.png"
        )
        if not path.exists():
            self.fail("Required shared occlusion source not installed")
        self.assertIsNone(self.vision.observe(cv.imread(str(path))))

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
