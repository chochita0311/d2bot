import math
import unittest
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.scene_vision import ArcaneHubObserver, HubStabilityTracker


class ArcaneHubVisionTests(unittest.TestCase):
    def setUp(self):
        self.vision = ArcaneHubObserver()

    def scene(self, point=(960, 600), parts=3, flames=True):
        frame = np.zeros((1140, 1922, 3), np.uint8)
        for rule, image in self.vision.parts[:parts]:
            x, y = np.array(point) + rule["offset"]
            h, w = image.shape
            frame[y : y + h, x : x + w] = cv.cvtColor(image, cv.COLOR_GRAY2BGR)
        if flames:
            for dx in (-48, 48):
                cv.rectangle(frame, (point[0] + dx - 8, point[1] - 63), (point[0] + dx + 8, point[1] - 37), (255, 100, 20), -1)
        return frame

    def test_paired_flames_and_two_ornaments_translate_together(self):
        self.assertEqual((960, 600), self.vision.observe(self.scene(parts=2)))
        self.assertEqual((500, 820), self.vision.observe(self.scene((500, 820))))
        self.assertEqual("blue_pair_and_floor_ornaments", self.vision.last_evidence["kind"])

    def test_flames_or_one_ornament_or_floor_alone_do_not_produce_waypoint(self):
        for frame in (self.scene(parts=0), self.scene(parts=1), self.scene(flames=False)):
            self.assertIsNone(self.vision.observe(frame))

    def test_actual_near_and_distant_waypoints_and_hostile_effect_negative(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references"
        cases = (
            ("hub/main.png", (961, 500)),
            ("hub/clear-floor.png", (961, 676)),
            ("hub/distant-effects.png", (500, 822)),
            ("navigation/stairs-hostile-effects-no-waypoint.png", None),
            ("hub/actor-occluded.png", (961, 503)),
        )
        for name, expected in cases:
            path = owner / name
            if not path.exists():
                self.fail("Required shared waypoint references not installed")
            point = self.vision.observe(cv.imread(str(path)))
            if expected is None:
                self.assertIsNone(point, name)
            else:
                self.assertIsNotNone(point, name)
                self.assertLessEqual(math.hypot(*(a - b for a, b in zip(point, expected))), 8, name)

    def test_layout_mismatch_is_not_resized(self):
        with self.assertRaises(ValueError):
            self.vision.observe(np.zeros((753, 1267, 3), np.uint8))

    def test_actual_return_map_overlay_is_unknown_until_current_landmarks_are_uncovered(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/hub"
        covered = cv.imread(str(owner / "return-map-overlay-on.png"))
        clear = cv.imread(str(owner / "return-map-overlay-off.png"))
        self.assertIsNone(self.vision.observe(covered))
        self.assertEqual((1105, 546), self.vision.observe(clear))
        self.assertEqual({"right", "upper", "lower"}, {part["id"] for part in self.vision.last_evidence["parts"]})
        for box in ((1020, 460, 1085, 540), (1130, 460, 1190, 540)):
            hidden = clear.copy()
            left, top, right, bottom = box
            hidden[top:bottom, left:right] = 0
            self.assertIsNone(self.vision.observe(hidden), box)

    def test_merged_braziers_can_seed_from_other_current_ornaments_but_not_dim_blue(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/hub"
        frame = cv.imread(str(owner / "merged-brazier-town-finish.png"))
        self.vision.parts = [(rule, image) for rule, image in self.vision.parts if rule["id"] in ("upper", "left-tip")]
        self.assertEqual((1047, 575), self.vision.observe(frame))
        self.assertEqual({"upper", "left-tip"}, {part["id"] for part in self.vision.last_evidence["parts"]})
        self.assertEqual("floor_anchored_current_pixels", self.vision.last_evidence["blue_evidence"])
        for box in ((1032, 499, 1080, 525), (831, 565, 883, 600), (980, 507, 1017, 544), (1078, 507, 1115, 544)):
            hidden = frame.copy()
            x0, y0, x1, y1 = box
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden), box)
        dim = cv.imread(str(owner / "merged-dim-brazier.png"))
        self.assertIsNotNone(self.vision._match_floor(cv.cvtColor(dim, cv.COLOR_BGR2GRAY), (1047, 575)))
        self.assertIsNone(self.vision.observe(dim))

    def test_current_southwest_approach_uses_distinct_ornaments_and_never_double_counts_variants(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references"
        frame = cv.imread(str(owner / "hub/southwest-approach-floor.png"))
        self.assertEqual((1306, 475), self.vision.observe(frame))
        self.assertEqual({"right", "lower"}, {part["id"] for part in self.vision.last_evidence["parts"]})
        for box in ((1380, 456, 1470, 505), (1230, 515, 1390, 570), (1230, 405, 1283, 455), (1325, 405, 1380, 455)):
            hidden = frame.copy()
            x0, y0, x1, y1 = box
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden), box)
        only_lower = [(rule, template) for rule, template in self.vision.parts if rule["id"] == "lower"]
        self.vision.parts = only_lower + only_lower
        self.assertIsNone(self.vision._match_floor(cv.cvtColor(frame, cv.COLOR_BGR2GRAY), (1306, 475)))

    def test_effect_merged_braziers_need_two_current_ornaments_and_both_blue_patches(self):
        source = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/hub/effects.png"
        if not source.exists():
            self.fail("Required shared main185 central reference not installed")
        frame = cv.imread(str(source))
        self.assertEqual((1047, 575), self.vision.observe(frame))
        self.assertEqual("floor_anchored_current_pixels", self.vision.last_evidence["blue_evidence"])
        self.assertEqual({"right", "left-tip"}, {part["id"] for part in self.vision.last_evidence["parts"]})
        for box in ((831, 565, 883, 600), (1126, 556, 1209, 601), (976, 500, 1024, 548), (1072, 500, 1120, 548)):
            hidden = frame.copy()
            x0, y0, x1, y1 = box
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden), box)

    def test_current_return_floor_variant_needs_two_distinct_ornaments_and_flames(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/hub"
        frame = cv.imread(str(owner / "left-floor-return.png"))
        self.assertEqual((961, 503), self.vision.observe(frame))
        self.assertEqual({"left", "upper-left"}, {part["id"] for part in self.vision.last_evidence["parts"]})
        for box in ((800, 492, 884, 535), (800, 427, 870, 472), (884, 421, 937, 489), (983, 421, 1036, 489)):
            hidden = frame.copy()
            x0, y0, x1, y1 = box
            hidden[y0:y1, x0:x1] = 0
            self.assertIsNone(self.vision.observe(hidden), box)
        only_left = [(rule, template) for rule, template in self.vision.parts if rule["id"] == "left"]
        self.vision.parts = only_left + only_left
        self.assertIsNone(self.vision._match_floor(cv.cvtColor(frame, cv.COLOR_BGR2GRAY), (961, 503)))

    def track(self, tracker, frame, at, motion=(0, 0)):
        point = self.vision.observe(frame)
        return tracker.update(frame, point, self.vision.last_evidence, at, motion)

    def test_static_pair_needs_recent_full_reference_and_never_extends_its_age(self):
        tracker = HubStabilityTracker(self.vision)
        hidden = self.scene(parts=0)
        self.assertEqual((None, None), self.track(tracker, hidden, 0))
        self.track(tracker, self.scene(parts=2), 1)
        point, evidence = self.track(tracker, hidden, 1.8)
        self.assertEqual((960, 600), point)
        self.assertEqual("recent_floor_and_current_blue_pair", evidence["kind"])
        self.assertEqual(1, evidence["reference_observed_at"])
        self.assertEqual([], evidence["current_floor_parts"])
        self.assertEqual((None, None), self.track(tracker, hidden, 3.01))

    def test_movement_unknown_motion_or_shifted_pair_cannot_confirm_static_hub(self):
        for motion in (None, (2, 0), (500, -250)):
            tracker = HubStabilityTracker(self.vision)
            self.track(tracker, self.scene(parts=2), 0)
            self.assertEqual((None, None), self.track(tracker, self.scene(parts=0), 0.2, motion))
            self.assertEqual((None, None), self.track(tracker, self.scene(parts=0), 0.3))
        tracker = HubStabilityTracker(self.vision)
        self.track(tracker, self.scene(parts=2), 0)
        self.assertEqual((None, None), self.track(tracker, self.scene((980, 600), parts=0), 0.2))

    def test_actual_floor_occlusion_retains_only_current_pair_with_static_reference(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references"
        paths = [owner / ("hub/floor-{}.png".format(name)) for name in ("recovered", "occluded")]
        if not all(path.exists() for path in paths):
            self.fail("Required shared main156 waypoint references not installed")
        full, hidden = (cv.imread(str(path)) for path in paths)
        self.assertIsNone(self.vision.observe(hidden))
        tracker = HubStabilityTracker(self.vision)
        self.assertIsNotNone(self.track(tracker, full, 0)[0])
        point, evidence = self.track(tracker, hidden, 0.7)
        self.assertLessEqual(math.hypot(point[0] - 961, point[1] - 503), 8)
        self.assertEqual("recent_floor_and_current_blue_pair", evidence["kind"])
