import unittest

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.scene_vision import FlashGroundObserver, ArcaneHoverObserver, ArcaneHazardObserver, ArcaneTargetObserver


class ArcaneTargetTests(unittest.TestCase):
    def test_current_title_inside_red_health_panel_is_positive(self):
        observer = ArcaneTargetObserver()
        frame = np.zeros((1140, 1922, 3), np.uint8)
        frame[76:116, 880:1050] = (0, 0, 67)
        title = observer.titles[0][1]
        frame[83:109, 906:1017] = cv.cvtColor(title, cv.COLOR_GRAY2BGR)
        reading = observer.observe(frame)
        self.assertIsNotNone(reading)
        self.assertEqual("ghoul-lord", reading.kind)
        self.assertGreater(reading.title_score, 0.9)
        self.assertTrue(all(type(value) is int for value in reading.evidence_box))

    def test_blank_or_nontext_red_region_does_not_confirm_enemy(self):
        observer = ArcaneTargetObserver()
        frame = np.zeros((1140, 1922, 3), np.uint8)
        self.assertIsNone(observer.observe(frame))
        frame[76:116, 880:1050] = (0, 0, 67)
        self.assertIsNone(observer.observe(frame))
        with self.assertRaises(ValueError):
            observer.observe(frame[:900])


class FlashGroundTests(unittest.TestCase):
    def setUp(self):
        self.marker = np.zeros((20, 32, 3), np.uint8)
        cv.line(self.marker, (3, 4), (27, 16), (220, 180, 20), 2)
        cv.line(self.marker, (3, 16), (27, 4), (220, 180, 20), 2)
        self.observer = FlashGroundObserver(self.marker, search_box=(800, 480, 1120, 700), anchor_offset=(16, 10))

    def frame(self, points):
        frame = np.zeros((1140, 1922, 3), np.uint8)
        for x, y, shield in points:
            frame[y - 10 : y + 10, x - 16 : x + 16] = self.marker
            if shield:
                cv.circle(frame, (x, y - 150), 13, (35, 100, 125), -1)
        return frame

    def test_ground_follows_current_marker_instead_of_fixed_anchor(self):
        for point in ((960, 580), (997, 610)):
            reading = self.observer.observe(self.frame([(point[0], point[1], True)]))
            self.assertEqual(point, reading.point)

    def test_merc_like_marker_without_player_effect_is_not_an_anchor(self):
        self.assertIsNone(self.observer.observe(self.frame([(960, 580, False)])).point)

    def test_marker_blended_over_bright_floor_keeps_shape_but_gray_floor_does_not(self):
        frame = self.frame([(960, 580, True)])
        patch = frame[570:590, 944:976]
        frame[570:590, 944:976] = (patch.astype(np.float32) * 0.5 + 100).astype(np.uint8)
        self.assertEqual((960, 580), self.observer.observe(frame).point)
        frame[570:590, 944:976] = 200
        self.assertIsNone(self.observer.observe(frame).point)

    def test_two_player_like_candidates_are_ambiguous(self):
        self.assertEqual("ground_ambiguous", self.observer.observe(self.frame([(900, 580, True), (1030, 580, True)])).reason)

    def test_wrong_layout_and_missing_marker_do_not_reuse_previous_ground(self):
        self.assertEqual((960, 580), self.observer.observe(self.frame([(960, 580, True)])).point)
        self.assertIsNone(self.observer.observe(self.frame([])).point)
        self.assertEqual("layout_unverified", self.observer.observe(np.zeros((900, 1600, 3), np.uint8)).reason)

    def test_warcry_colored_fragment_touching_patch_edge_is_not_energy_shield(self):
        frame = self.frame([(960, 580, False)])
        frame[422:454, 948:986] = (35, 100, 125)
        self.assertIsNone(self.observer.observe(frame).point)

    def test_energy_shield_with_a_thin_effect_tail_keeps_its_round_core(self):
        frame = self.frame([(960, 580, True)])
        cv.line(frame, (970, 430), (984, 430), (35, 100, 125), 1)
        self.assertEqual((960, 580), self.observer.observe(frame).point)

    def test_occluded_shield_requires_same_frame_reviewed_body_at_current_marker(self):
        body = np.random.RandomState(17).randint(0, 255, (80, 48, 3), dtype=np.uint8)
        observer = FlashGroundObserver(
            self.marker,
            search_box=(800, 480, 1120, 700),
            anchor_offset=(16, 10),
            body_templates=(body,),
            body_box=(-24, -120, 24, -40),
        )
        for point in ((960, 580), (997, 610)):
            frame = self.frame([(point[0], point[1], False)])
            self.assertIsNone(observer.observe(frame).point)
            x, y = point
            frame[y - 120 : y - 40, x - 24 : x + 24] = body
            reading = observer.observe(frame)
            self.assertEqual(point, reading.point)
            self.assertEqual("flash_marker_and_body", reading.reason)
            frame[y - 120 : y - 40, x - 24 : x + 24] = 0
            self.assertIsNone(observer.observe(frame).point)

    def test_bounded_identity_tracking_requires_current_marker_and_does_not_extend_strong_proof(self):
        self.observer.tracking = dict(maximum_seconds=0.6, maximum_gap_seconds=0.25, maximum_displacement=8)
        self.assertIsNone(self.observer.observe(self.frame([(960, 580, False)]), captured_at=1).point)
        self.assertEqual((960, 580), self.observer.observe(self.frame([(960, 580, True)]), captured_at=2).point)
        for at, point in ((2.2, (963, 580)), (2.4, (964, 582)), (2.59, (965, 581))):
            reading = self.observer.observe(self.frame([(point[0], point[1], False)]), captured_at=at)
            self.assertEqual(point, reading.point)
            self.assertEqual("flash_marker_tracked", reading.reason)
        self.assertIsNone(self.observer.observe(self.frame([(965, 581, False)]), captured_at=2.61).point)
        for at, points in ((3.3, [(960, 580, False)]), (3.1, [(980, 580, False)]), (3.1, []), (2.9, [(960, 580, False)])):
            self.observer.observe(self.frame([(960, 580, True)]), captured_at=3)
            self.assertIsNone(self.observer.observe(self.frame(points), captured_at=at).point)
            self.assertIsNone(self.observer.observe(self.frame([(960, 580, False)]), captured_at=3.2).point)

    def test_without_automap_ground_requires_current_body_and_shield_and_moves_with_them(self):
        body = np.random.RandomState(71).randint(0, 100, (80, 48, 3), dtype=np.uint8)
        observer = FlashGroundObserver(
            self.marker,
            search_box=(800, 480, 1120, 700),
            anchor_offset=(16, 10),
            body_templates=(body,),
            body_box=(-24, -120, 24, -40),
        )
        for point in ((960, 580), (997, 610)):
            x, y = point
            frame = self.frame([])
            frame[y - 120 : y - 40, x - 24 : x + 24] = body
            self.assertIsNone(observer.observe(frame).point)
            cv.circle(frame, (x, y - 150), 13, (35, 100, 125), -1)
            reading = observer.observe(frame)
            self.assertEqual(point, reading.point)
            self.assertEqual("flash_body_and_es", reading.reason)
            frame[y - 120 : y - 40, x - 24 : x + 24] = 0
            self.assertIsNone(observer.observe(frame).point)

    def test_changed_pose_needs_shield_and_vertical_foreground_not_uniform_fog(self):
        template = np.random.RandomState(92).randint(0, 100, (80, 48, 3), dtype=np.uint8)
        observer = FlashGroundObserver(
            self.marker,
            search_box=(800, 480, 1120, 700),
            anchor_offset=(16, 10),
            body_templates=(template,),
            body_box=(-24, -120, 24, -40),
        )
        for x, y in ((960, 580), (997, 610)):
            frame = self.frame([])
            cv.circle(frame, (x, y - 150), 13, (35, 100, 125), -1)
            self.assertIsNone(observer.observe(frame).point)
            frame[y - 112 : y - 48, x - 8 : x + 8] = (20, 70, 100)
            reading = observer.observe(frame)
            self.assertEqual((x, y), reading.point)
            self.assertEqual("flash_es_and_body_shape", reading.reason)
            self.assertIsNone(reading.score)
        fog = np.full_like(frame, (80, 30, 10))
        cv.circle(fog, (960, 430), 13, (35, 100, 125), -1)
        self.assertIsNone(observer.observe(fog).point)
        frame = self.frame([])
        for x in (900, 1030):
            cv.circle(frame, (x, 430), 13, (35, 100, 125), -1)
            frame[468:532, x - 8 : x + 8] = (20, 70, 100)
        self.assertEqual("ground_ambiguous", observer.observe(frame).reason)


class ArcaneHazardTests(unittest.TestCase):
    def test_parallel_current_and_landing_scans_keep_both_positive_regions(self):
        observer = ArcaneHazardObserver()
        rule, template = observer.region.templates[0]
        height, width = template.shape[:2]
        frame = np.zeros((1140, 1922, 3), np.uint8)
        points = ((730, 500), (1360, 120))
        for x, y in points:
            frame[y : y + height, x : x + width] = template
        found = observer.observe(frame, (960, 580), 10, 10.01, aim=(1380, 260))
        boxes = {candidate.box for candidate in found if candidate.asset_id == rule["id"]}
        self.assertEqual({(x, y, x + width, y + height) for x, y in points}, boxes)

    def test_positive_enemy_near_distant_landing_is_checked_before_teleport(self):
        observer = ArcaneHazardObserver()
        rule, template = observer.region.templates[0]
        height, width = template.shape[:2]
        frame = np.zeros((1140, 1922, 3), np.uint8)
        frame[120 : 120 + height, 1360 : 1360 + width] = template
        found = observer.observe(frame, (960, 580), 10, 10.01, aim=(1380, 260))
        self.assertTrue(
            any(candidate.asset_id == rule["id"] and candidate.box == (1360, 120, 1360 + width, 120 + height) for candidate in found)
        )


class ArcaneHoverTests(unittest.TestCase):
    def test_native_hover_evidence_box_keeps_viewport_offset_and_excludes_town_npcs(self):
        template = np.random.RandomState(31).randint(0, 255, (24, 49, 3), dtype=np.uint8)
        observer = ArcaneHoverObserver([("shrine", template)])
        frame = np.zeros((1140, 1922, 3), np.uint8)
        frame[400:424, 1000:1049] = template
        found = observer.observe(frame, (1030, 480))
        self.assertEqual(1, len(found))
        self.assertEqual((1000, 400, 1049, 424), found[0].evidence_box)
        self.assertEqual("shrine", found[0].kind)
        self.assertEqual((), observer.observe(np.zeros_like(frame), (1030, 480)))
        with self.assertRaisesRegex(ValueError, "NPC"):
            ArcaneHoverObserver([("npc", template)])
        with self.assertRaisesRegex(ValueError, "native"):
            observer.observe(np.zeros((753, 1267, 3), np.uint8), (1030, 480))


if __name__ == "__main__":
    unittest.main()
