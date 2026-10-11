import unittest
import json
import tempfile
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.navigation import ArcanePathObserver, BackgroundMotionObserver, CameraMotion, PathSettings
from diablo2.runs.summoner.search import ArcaneSearch, SearchLimits, SearchObservation, run_search
from diablo2.common.field_control import DryFieldInput
from diablo2.tools.arcane_navigation_replay import replay


class NavigationTests(unittest.TestCase):
    def setUp(self):
        self.observer = ArcanePathObserver(PathSettings(4, 0.5, 20, 48, 40, 16, 8))

    def view(self, mask, sequence=1, at=0, generation=0, anchor=(100, 100)):
        height, width = mask.shape
        return self.observer.from_mask(mask, sequence, generation, at, (width, height), (0, 0, width, height), anchor)

    def test_automap_node_is_not_world_floor_in_reviewed_native_crops(self):
        directory = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        references = json.loads((directory / "automap-floor-references.json").read_text(encoding="utf-8"))
        calibration = json.loads((directory / "navigation-calibration.json").read_text(encoding="utf-8"))
        frame = np.zeros((1140, 1922, 3), np.uint8)
        for case in references["references"]:
            x0, y0, x1, y1 = case["box"]
            frame[y0:y1, x0:x1] = cv.imread(str(directory / case["path"]))
        observer = ArcanePathObserver(
            PathSettings(**calibration["path"]), terrain=calibration["terrain"], automap_overlay=calibration["automap_overlay"]
        )
        view = observer.observe(frame, sequence=1, generation=0, captured_at=0, viewport=(24, 55, 1900, 900), anchor=(1480, 600))
        for case in references["references"]:
            x, y = case["point"]
            self.assertEqual(case["expected_floor"], bool(view.floor[(y - 55) // 8, (x - 24) // 8]))
        self.assertFalse(view.excluded_pixels.flags.writeable)

    def test_four_headings_follow_current_connected_floor(self):
        mask = np.ones((200, 200), bool)
        for wing, signs in (("north", (1, -1)), ("east", (1, 1)), ("south", (-1, 1)), ("west", (-1, -1))):
            with self.subTest(wing=wing):
                choice = self.observer.choose(self.view(mask), wing)
                self.assertIsNotNone(choice.aim)
                self.assertGreater((choice.aim[0] - 100) * signs[0], 0)
                self.assertGreater((choice.aim[1] - 100) * signs[1], 0)
                self.assertLessEqual(np.linalg.norm(np.asarray(choice.aim) - (100, 100)), 49)

    def test_disconnected_floor_and_ui_do_not_become_reachable_path(self):
        mask = np.zeros((200, 200), bool)
        mask[80:116, 60:116] = True
        mask[0:64, 136:200] = True
        choice = self.observer.choose(self.view(mask), "north")
        self.assertLess(choice.aim[0], 116)
        self.assertGreater(choice.aim[1], 80)
        blank = self.observer.choose(self.view(np.zeros_like(mask)), "north")
        self.assertIsNone(blank.aim)
        frame = np.zeros((240, 200, 3), np.uint8)
        frame[200:] = (111, 109, 109)
        view = self.observer.observe(frame, sequence=1, generation=0, captured_at=0, viewport=(0, 0, 200, 200), anchor=(100, 100))
        self.assertFalse(view.floor.any())

    def test_small_effect_island_at_feet_is_not_the_path_seed(self):
        mask = np.zeros((200, 200), bool)
        mask[96:104, 96:104] = True
        mask[76:92, 100:180] = True
        choice = self.observer.choose(self.view(mask), "north")
        self.assertIsNotNone(choice.aim)
        self.assertGreater(choice.aim[0], 104)
        self.assertLess(choice.aim[1], 92)

    def test_teleport_can_cross_void_but_targets_observed_floor_interior(self):
        mask = np.zeros((200, 200), bool)
        mask[80:120, 64:112] = True
        mask[20:72, 140:196] = True
        mask[72:84, 140:196] = True
        view = self.view(mask)
        self.observer.teleport = dict(maximum_distance=120, clearance_pixels=12)
        choice = self.observer.choose(view, "north")
        self.assertEqual("observed_teleport_floor", choice.reason)
        x, y = choice.aim
        self.assertTrue(mask[y - 8 : y + 9, x - 8 : x + 9].all())
        self.assertGreaterEqual(x, 148)
        self.assertLessEqual(np.linalg.norm(np.asarray(choice.aim) - view.anchor), 120)

    def test_teleport_rejects_narrow_wall_tongue_and_unknown_viewport_edges(self):
        mask = np.zeros((200, 200), bool)
        mask[0:200, 188:200] = True
        mask[80:92, 108:184] = True
        self.observer.teleport = dict(maximum_distance=120, clearance_pixels=12)
        choice = self.observer.choose(self.view(mask), "north")
        self.assertIsNone(choice.aim)
        self.assertEqual("teleport_floor_unobserved", choice.reason)

    def test_wide_continuous_detour_can_turn_left_when_north_floor_ends(self):
        mask = np.zeros((200, 400), bool)
        mask[20:100, 20:160] = True
        self.observer.teleport = dict(maximum_distance=300, preferred_distance=300, clearance_pixels=12)
        view = self.view(mask, anchor=(300, 140))
        self.assertIsNone(self.observer.choose(view, "north").aim)
        choice = self.observer.choose(view, "north", allow_detour=True)
        self.assertEqual("observed_teleport_detour", choice.reason)
        self.assertLess(choice.aim[0], 160)
        self.assertGreater(np.linalg.norm(np.asarray(choice.aim) - view.anchor), 200)

    def test_visible_corridor_exit_beats_capped_platform_in_global_heading(self):
        mask = np.zeros((240, 400), bool)
        mask[100:180, 140:380] = True
        mask[80:140, 0:180] = True
        self.observer.teleport = dict(maximum_distance=300, preferred_distance=300, clearance_pixels=12)
        view = self.view(mask, anchor=(300, 140))
        choice = self.observer.choose(view, "north", allow_detour=True, prefer_frontier=True)
        self.assertEqual("observed_floor_frontier", choice.reason)
        self.assertLess(choice.aim[0], 120)

    def test_teleport_prefers_near_floor_and_crosses_gap_when_it_is_absent(self):
        mask = np.zeros((200, 500), bool)
        mask[80:120, 60:180] = True
        mask[20:100, 350:450] = True
        self.observer.teleport = dict(maximum_distance=400, preferred_distance=100, clearance_pixels=12)
        near = self.observer.choose(self.view(mask), "north")
        self.assertLess(near.aim[0], 180)
        mask[80:120, 60:180] = False
        far = self.observer.choose(self.view(mask), "north")
        self.assertGreater(far.aim[0], 350)

    def test_gap_crossing_uses_nearest_observed_interior_before_farther_floor(self):
        mask = np.zeros((240, 600), bool)
        mask[40:120, 280:360] = True
        mask[16:120, 440:560] = True
        self.observer.teleport = dict(maximum_distance=500, preferred_distance=100, clearance_pixels=12)
        choice = self.observer.choose(self.view(mask), "north")
        self.assertLess(choice.aim[0], 360)
        self.assertGreaterEqual(choice.aim[0], 288)
        blocked = self.observer.choose(self.view(np.zeros_like(mask)), "north")
        self.assertIsNone(blocked.aim)

    def test_bend_shortens_step_and_variable_rectangle_has_no_fixed_stage_ticks(self):
        for length in (48, 72, 96):
            mask = np.zeros((240, 240), bool)
            mask[100:116, 20 : 40 + length] = True
            mask[24:116, 24 + length : 40 + length] = True
            view = self.view(mask, anchor=(30, 108))
            choice = self.observer.choose(view, "north")
            self.assertIsNotNone(choice.aim)
            self.assertTrue(view.floor[choice.aim[1] // 4, choice.aim[0] // 4])
            self.assertLessEqual(np.linalg.norm(np.asarray(choice.aim) - (30, 108)), 49)

    def test_return_destination_and_hover_exclusion_use_current_frame(self):
        view = self.view(np.ones((200, 200), bool))
        choice = self.observer.choose(view, "north", position=(40, -20), destination=(0, 0))
        self.assertLess(choice.aim[0], 100)
        self.assertGreater(choice.aim[1], 100)
        first = self.observer.choose(view, "north")
        x, y = first.aim
        second = self.observer.choose(view, "north", excluded=((x - 8, y - 8, x + 8, y + 8),))
        self.assertNotEqual(first.aim, second.aim)

    def test_background_translation_not_effect_or_duplicate_frame(self):
        random = np.random.RandomState(7)
        original = random.randint(25, 200, (240, 320, 3), dtype=np.uint8)
        observer = BackgroundMotionObserver(minimum_matches=12, minimum_inlier_ratio=0.7, maximum_error=2, scale=1)
        view = self.view(np.ones((240, 320), bool), anchor=(160, 120))
        self.assertIsNone(observer.observe(original, view).displacement)
        shifted = cv.warpAffine(original, np.float32([[1, 0, -24], [0, 1, 12]]), (320, 240))
        next_view = self.view(np.ones((240, 320), bool), sequence=2, at=0.1, anchor=(160, 120))
        motion = observer.observe(shifted, next_view)
        self.assertIsNotNone(motion.displacement)
        np.testing.assert_allclose(motion.displacement, (24, -12), atol=1)
        self.assertEqual(observer.observe(shifted, next_view).reason, "frame_not_newer")
        blank = self.view(np.ones((240, 320), bool), sequence=3, at=0.2, anchor=(160, 120))
        self.assertIsNone(observer.observe(np.zeros_like(original), blank).displacement)
        recovered = self.view(np.ones((240, 320), bool), sequence=4, at=0.3, anchor=(160, 120))
        motion = observer.observe(shifted, recovered)
        np.testing.assert_allclose(motion.displacement, (0, 0), atol=1)

    def test_overlap_hint_limits_features_without_substituting_requested_motion(self):
        frame = np.random.RandomState(53).randint(25, 200, (500, 800, 3), dtype=np.uint8)
        observer = BackgroundMotionObserver(
            minimum_matches=12,
            minimum_inlier_ratio=0.65,
            maximum_error=3,
            landmark_viewport=(20, 20, 780, 480),
            actor_exclusion=(30, 30, 30, 30),
        )
        view = self.view(np.ones((500, 800), bool), anchor=(400, 250))
        observer.observe(frame, view)
        current = self.view(np.ones((500, 800), bool), sequence=2, at=0.1, anchor=(400, 250))
        unchanged = observer.observe(frame, current, overlap_hint=(300, -100))
        np.testing.assert_allclose(unchanged.displacement, (0, 0), atol=1)
        shifted = cv.warpAffine(frame, np.float32([[1, 0, -300], [0, 1, 100]]), (800, 500))
        current = self.view(np.ones((500, 800), bool), sequence=3, at=0.2, anchor=(400, 250))
        measured = observer.observe(shifted, current, overlap_hint=(300, -100))
        np.testing.assert_allclose(measured.displacement, (300, -100), atol=2)
        current = self.view(np.ones((500, 800), bool), sequence=4, at=0.3, anchor=(400, 250))
        self.assertIsNone(observer.observe(np.zeros_like(frame), current, overlap_hint=(300, -100)).displacement)

    def test_failed_cast_large_hint_recovers_actual_stationary_but_not_unrelated_translation(self):
        frame = np.random.RandomState(13).randint(25, 200, (500, 800, 3), dtype=np.uint8)
        observer = BackgroundMotionObserver(
            minimum_matches=12,
            minimum_inlier_ratio=0.65,
            maximum_error=3,
            landmark_viewport=(20, 20, 780, 480),
            actor_exclusion=(30, 30, 30, 30),
        )
        observer.observe(frame, self.view(np.ones((500, 800), bool), anchor=(400, 250)))
        current = self.view(np.ones((500, 800), bool), sequence=2, at=0.1, anchor=(400, 250))
        np.testing.assert_allclose(observer.observe(frame, current, overlap_hint=(780, -450)).displacement, (0, 0), atol=1)
        shifted = cv.warpAffine(frame, np.float32([[1, 0, -70], [0, 1, 35]]), (800, 500))
        current = self.view(np.ones((500, 800), bool), sequence=3, at=0.2, anchor=(400, 250))
        self.assertIsNone(observer.observe(shifted, current, overlap_hint=(780, -450)).displacement)

    def test_replay_uses_native_crop_and_explicit_calibration_without_inputs(self):
        calibration = {
            "mode": "input_free_calibration",
            "frame_size": [200, 200],
            "anchor": [100, 100],
            "wing": "north",
            "path": dict(self.observer.settings.__dict__),
            "motion": {"minimum_matches": 12, "minimum_inlier_ratio": 0.7, "maximum_error": 2},
            "terrain": {
                "floor_bgr": [[111, 109, 109]],
                "void_bgr": [[0, 0, 0]],
                "floor_lab_distance": 18,
                "void_lab_distance": 16,
            },
        }
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cv.imwrite(str(root / "source.png"), np.full((200, 200, 3), (111, 109, 109), np.uint8))
            manifest = {
                "revision": "synthetic",
                "frame_size": [200, 200],
                "cases": [
                    {
                        "id": "one",
                        "path": "source.png",
                        "viewport": [0, 0, 200, 200],
                        "provenance": {"episode": "one", "seconds_from_record_start": 0},
                    }
                ],
            }
            path = root / "manifest.json"
            path.write_text(json.dumps(manifest), encoding="utf-8")
            result = replay(path, calibration, overlays=root / "overlays")
            self.assertEqual(result["inputs_sent"], 0)
            self.assertIsNotNone(result["results"][0]["choice"]["aim"])
            self.assertTrue((root / "overlays/case-000.png").is_file())
            with self.assertRaisesRegex(ValueError, "input-free calibration"):
                replay(path, dict(calibration, mode="live"))
            with self.assertRaises(FileExistsError):
                replay(path, calibration, overlays=root / "overlays")

    def test_landmarks_track_motion_outside_missing_floor_and_ignore_actor_effects(self):
        random = np.random.RandomState(11)
        original = random.randint(25, 200, (480, 640, 3), dtype=np.uint8)
        view = self.view(np.zeros((480, 640), bool), anchor=(320, 240))
        next_view = self.view(np.zeros((480, 640), bool), sequence=2, at=0.1, anchor=(320, 240))
        for delta in ((24, -12), (0, 0)):
            observer = BackgroundMotionObserver(
                minimum_matches=12,
                minimum_inlier_ratio=0.7,
                maximum_error=2,
                scale=1,
                landmark_viewport=(20, 20, 620, 460),
                actor_exclusion=(100, 120, 100, 60),
            )
            observer.observe(original, view)
            changed = cv.warpAffine(original, np.float32([[1, 0, -delta[0]], [0, 1, -delta[1]]]), (640, 480))
            changed[120:300, 220:420] = random.randint(0, 255, (180, 200, 3), dtype=np.uint8)
            measured = observer.observe(changed, next_view)
            self.assertIsNotNone(measured.displacement)
            np.testing.assert_allclose(measured.displacement, delta, atol=1)


class SearchTests(unittest.TestCase):
    def setUp(self):
        self.observer = ArcanePathObserver(PathSettings(4, 0.5, 20, 32, 40, 8, 8))
        self.search = ArcaneSearch(
            self.observer,
            SearchLimits(0.5, 2, 20, 20, 100, 4, 8, 100, 2, settled_frames=2, settled_motion_pixels=2),
            reference_seconds=0.32,
            started_at=0,
        )
        self.at, self.sequence = 0.0, 0

    def observation(self, *, motion=(0.0, 0.0), generation=None, **values):
        self.sequence += 1
        view = self.observer.from_mask(
            np.ones((200, 200), bool),
            self.sequence,
            (self.search.expected_generation or 0) if generation is None else generation,
            self.at,
            (200, 200),
            (0, 0, 200, 200),
            (100, 100),
        )
        return SearchObservation(view, CameraMotion(motion, 20, "synthetic"), values.pop("movement_allowed", True), **values)

    def step(self, **values):
        return self.search.step(self.observation(**values), self.at)

    def acknowledge(self, action):
        self.assertTrue(self.search.acknowledge(action, self.at, True))

    def cast(self):
        aim = self.step()
        self.assertEqual(aim.kind, "aim")
        self.acknowledge(aim)
        self.at += 0.05
        cast = self.step()
        self.assertEqual(cast.kind, "cast")
        self.acknowledge(cast)
        return cast

    def test_cast_waits_for_new_generation_motion_and_reference_time(self):
        cast = self.cast()
        self.at += 0.1
        self.assertEqual(self.step(generation=cast.generation).reason, "frame_precedes_cast")
        self.assertEqual(self.step(motion=(20, -10)).reason, "landing_pending")
        self.at += 0.3
        self.assertEqual(self.step().reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.step().kind, "aim")
        self.assertEqual(self.search.position, (20, -10))
        self.assertEqual(self.search.cast_count, 1)

    def test_no_motion_is_not_arrival_and_timeout_does_not_restart(self):
        self.cast()
        for at in (0.2, 0.6, 1, 1.5):
            self.at = at
            self.assertEqual(self.step(motion=None).kind, "hold")
        self.at = 2.05
        self.assertEqual(self.step().reason, "landing_unconfirmed")
        self.at += 0.1
        self.assertEqual(self.step(motion=(20, -10)).kind, "review")

    def test_hover_requires_new_clear_frame_and_never_accepts_npc(self):
        aim = self.step(hover=("chest", (120, 60, 160, 100)))
        self.assertEqual(aim.kind, "aim")
        self.acknowledge(aim)
        self.assertEqual(self.step(hover=("chest", (120, 60, 160, 100))).kind, "aim")
        with self.assertRaises(ValueError):
            self.observation(hover=("npc", (120, 60, 160, 100)))

    def test_four_empty_wings_require_hub_between_each_and_never_set_user_stop(self):
        for index, wing in enumerate(("north", "east", "south", "west")):
            self.assertEqual(self.search.wing, wing)
            self.cast()
            self.at += 0.4
            self.assertEqual(self.step(motion=(20, -10), terminal="nonboss_end").reason, "landing_pending")
            for _ in range(2):
                self.at += 0.05
                returned = self.step(terminal="nonboss_end")
            self.assertEqual(self.search.phase, "return")
            self.acknowledge(returned)
            self.at += 0.05
            cast = self.step()
            self.assertEqual(cast.kind, "cast")
            self.acknowledge(cast)
            self.at += 0.4
            before_hub = self.step(motion=(-20, 10), hub_ready=False)
            self.assertNotEqual(before_hub.kind, "wing_start")
            self.at += 0.05
            self.assertEqual(self.step(hub_ready=True).reason, "landing_pending")
            self.at += 0.05
            result = self.step(hub_ready=True)
            self.assertEqual(result.kind, "wing_start" if index < 3 else "review")
            self.at += 0.05
        self.assertEqual(self.search.checked_wings, ["north", "east", "south", "west"])
        self.assertEqual(self.search.review_reason, "all_wings_empty")

    def test_summoner_stops_remaining_wings_and_does_not_claim_kill(self):
        result = self.step(terminal="summoner_present")
        self.assertEqual(result.kind, "summoner")
        self.assertEqual(result.reason, "handoff_combat")
        self.assertEqual(self.search.wing, "north")
        self.assertEqual(self.search.checked_wings, [])

    def test_uncertain_survival_and_pending_ack_never_cast(self):
        self.assertEqual(self.step(movement_allowed=False).kind, "hold")
        aim = self.step()
        self.assertEqual(self.step().reason, "input_outcome_pending")
        self.acknowledge(aim)
        self.at += 0.1
        self.assertEqual(self.step(movement_allowed=False).kind, "hold")
        self.at += 0.1
        self.assertEqual(self.step().kind, "aim")

    def test_unknown_camera_registration_invalidates_cursor_and_landing_proof(self):
        aim = self.step()
        self.acknowledge(aim)
        self.at += 0.05
        reacquire = self.step(motion=None)
        self.assertEqual(reacquire.kind, "aim")
        self.acknowledge(reacquire)
        self.at += 0.05
        cast = self.step()
        self.acknowledge(cast)
        self.at += 0.1
        self.step(motion=(20, -10))
        self.at += 0.3
        self.assertEqual(self.step(motion=None).reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.step().reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.step().kind, "aim")

    def test_late_camera_movement_resets_settle_count_without_another_cast(self):
        self.cast()
        self.at += 0.4
        self.assertEqual(self.step(motion=(20, -10)).reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.step().reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.step(motion=(4, -2)).reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.step().reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.step().kind, "aim")
        self.assertEqual(1, self.search.cast_count)

    def test_runner_uses_current_observations_and_releases_on_exception(self):
        backend = DryFieldInput()
        observations = [self.observation()]

        def observe(generation):
            if observations:
                return observations.pop()
            raise RuntimeError("capture failed")

        with self.assertRaisesRegex(RuntimeError, "capture failed"):
            run_search(
                self.search,
                observe=observe,
                backend=backend,
                movement_key="f2",
                focused=lambda: True,
                interrupted=lambda: False,
                clock=lambda: self.at,
                wait=lambda seconds: False,
            )
        self.assertEqual(backend.events[-1], ("up", "f2"))
        self.assertFalse(any(event[0] == "press" for event in backend.events))


if __name__ == "__main__":
    unittest.main()
