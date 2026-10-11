import unittest
import json
import tempfile
from pathlib import Path
import numpy as np
import cv2 as cv

from diablo2.runs.summoner.navigation import ArcanePathObserver, PathChoice, PathSettings, PathView
from diablo2.runs.summoner.steering import ArcaneRouteFlow, ContinuousSteering
from diablo2.runs.summoner.continuous_navigation import restore_route_flow


class ContinuousSteeringTests(unittest.TestCase):
    def view(self, at, *, closed=False):
        floor = np.ones((100, 200), bool)
        if closed:
            floor[15:35, 140:160] = False
        return PathView(1, 0, at, (1600, 800), (0, 0, 1600, 800), (800, 400), 8, floor)

    def test_new_void_reaims_sharp_bend_before_cast_reference(self):
        steering = ContinuousSteering(0.32, 32)
        self.assertTrue(steering.update(self.view(1), PathChoice((1200, 200), "floor"), 1)[0])
        changed, reason = steering.update(self.view(1.08, closed=True), PathChoice((400, 200), "bend"), 1.08)
        self.assertTrue(changed)
        self.assertEqual("previous_floor_closed", reason)
        self.assertEqual((400, 200), steering.aim)

    def test_one_frame_side_flip_does_not_reverse_when_current_floor_is_open(self):
        steering = ContinuousSteering(0.32, 32)
        steering.update(self.view(1), PathChoice((1200, 200), "floor"), 1)
        changed, _ = steering.update(self.view(1.08), PathChoice((400, 200), "bend"), 1.08)
        self.assertFalse(changed)
        steering.update(self.view(1.16), PathChoice((1200, 200), "floor"), 1.16)
        self.assertEqual((1200, 200), steering.aim)

    def test_two_fresh_bend_frames_turn_without_waiting_full_cast(self):
        steering = ContinuousSteering(0.32, 32)
        steering.update(self.view(1), PathChoice((1200, 200), "floor"), 1)
        steering.update(self.view(1.08), PathChoice((400, 200), "bend"), 1.08)
        self.assertEqual((True, "bend_confirmed_twice"), steering.update(self.view(1.16), PathChoice((420, 210), "bend"), 1.16))

    def test_repeated_or_older_frame_cannot_confirm_turn(self):
        steering = ContinuousSteering(0.32, 32)
        steering.update(self.view(1), PathChoice((1200, 200), "floor"), 1)
        steering.update(self.view(1.08), PathChoice((400, 200), "bend"), 1.08)
        for at in (1.08, 1.02):
            changed, _ = steering.update(self.view(at), PathChoice((400, 200), "bend"), 1.16)
            self.assertFalse(changed)
        self.assertEqual((1200, 200), steering.aim)

    def test_bend_does_not_reverse_to_previous_platform_when_global_candidate_is_sideways(self):
        path = ArcanePathObserver(PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, clearance_pixels=32))
        flow = ArcaneRouteFlow(detour=(-1, -0.5))
        view = self.view(1)
        flow.choose(path, view, "north", PathChoice((400, 200), "detour"), 1)
        # 전체 북쪽 투영이 양수인 오른쪽 아래도 현재 좌상단 꺾임의 복귀 길일 수 있다.
        local = flow.choose(path, self.view(1.08), "north", PathChoice((1200, 600), "floor"), 1.08)
        self.assertLess(local.aim[0], 800)
        self.assertLess(local.aim[1], 400)
        self.assertEqual("follow_observed_bend", flow.reason)

    def test_two_fresh_forward_floor_frames_reopen_global_flow(self):
        path = ArcanePathObserver(PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, clearance_pixels=32))
        flow = ArcaneRouteFlow(detour=(-400, -200))
        forward = PathChoice((1200, 200), "floor")

        def corner(at):
            view = self.view(at)
            view.floor[:, :100] = False
            return view

        flow.choose(path, corner(1), "north", forward, 1)
        flow.choose(path, corner(1), "north", forward, 1.08)
        self.assertIsNotNone(flow.detour)
        result = flow.choose(path, corner(1.16), "north", forward, 1.16)
        self.assertEqual(forward, result)
        self.assertIsNone(flow.detour)

    def test_wide_platform_candidate_does_not_end_a_still_open_side_corridor(self):
        path = ArcanePathObserver(PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, clearance_pixels=32))
        flow = ArcaneRouteFlow(detour=(-400, -200))
        forward = PathChoice((1200, 200), "floor")
        flow.choose(path, self.view(1), "north", forward, 1)
        result = flow.choose(path, self.view(1.16), "north", forward, 1.16)
        self.assertIsNotNone(flow.detour)
        self.assertLess(result.aim[0], 800)

    def test_offset_landing_inside_forward_platform_does_not_rotate_corridor_axes(self):
        path = ArcanePathObserver(PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, clearance_pixels=32))
        flow = ArcaneRouteFlow()
        choice = PathChoice((1394, 353), "floor_inside_platform")
        self.assertEqual(choice, flow.choose(path, self.view(1), "north", choice, 1))
        self.assertIsNone(flow.detour)
        self.assertEqual((-1, -0.5), ArcaneRouteFlow(detour=(-414, -433)).detour)

    def test_opposite_connected_frontier_does_not_hide_current_forward_floor(self):
        path = ArcanePathObserver(
            PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, preferred_distance=600, clearance_pixels=32)
        )
        flow = ArcaneRouteFlow()
        result = flow.choose(path, self.view(1), "north", PathChoice((300, 650), "entry_frontier"), 1)
        self.assertGreater(result.aim[0], 800)
        self.assertLess(result.aim[1], 400)
        self.assertIsNone(flow.detour)

    def test_unobserved_bend_continuation_is_not_invented_as_an_escape(self):
        path = ArcanePathObserver(PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, clearance_pixels=32))
        flow = ArcaneRouteFlow(detour=(-400, -200))
        view = self.view(1)
        view.floor[:] = False
        result = flow.choose(path, view, "north", PathChoice((1200, 600), "floor"), 1)
        self.assertIsNone(result.aim)
        self.assertEqual("bend_continuation_unobserved", result.reason)

    def test_side_entry_frontier_cannot_hide_wide_forward_landing_across_stairs(self):
        path = ArcanePathObserver(
            PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, preferred_distance=600, clearance_pixels=32)
        )
        flow = ArcaneRouteFlow()
        result = flow.choose(path, self.view(1), "north", PathChoice((1200, 650), "side_entry_frontier"), 1)
        self.assertGreater(result.aim[0], 800)
        self.assertLess(result.aim[1], 400)
        self.assertIsNone(flow.detour)

    def test_side_corner_can_turn_locally_southwest_without_reversing_its_entry(self):
        path = ArcanePathObserver(
            PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, preferred_distance=600, clearance_pixels=32)
        )
        floor = np.zeros((800, 1600), np.uint8)
        cv.line(floor, (800, 400), (300, 650), 1, 120)
        cv.line(floor, (800, 400), (1300, 650), 1, 120)
        view = path.from_mask(floor, 1, 0, 1, (1600, 800), (0, 0, 1600, 800), (800, 400))
        flow = ArcaneRouteFlow(detour=(-400, -200))
        choice = flow.choose(path, view, "north", PathChoice((1250, 600), "old_entry"), 1)
        self.assertEqual("observed_corner_turn", choice.reason)
        self.assertLess(choice.aim[0], 800)
        self.assertGreater(choice.aim[1], 400)

    def test_opposite_entry_at_a_cap_uses_wide_observed_side_exit(self):
        path = ArcanePathObserver(
            PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, preferred_distance=600, clearance_pixels=32)
        )
        floor = np.zeros((800, 1600), np.uint8)
        cv.line(floor, (800, 400), (300, 650), 1, 120)
        cv.line(floor, (800, 400), (300, 150), 1, 120)
        view = path.from_mask(floor, 1, 0, 1, (1600, 800), (0, 0, 1600, 800), (800, 400))
        flow = ArcaneRouteFlow()
        choice = flow.choose(path, view, "north", PathChoice((300, 650), "entry_frontier"), 1)
        self.assertEqual("observed_corner_turn", choice.reason)
        self.assertLess(choice.aim[0], 800)
        self.assertLess(choice.aim[1], 400)

    def test_small_stair_cap_uses_observed_corner_before_next_wide_floor_is_visible(self):
        path = ArcanePathObserver(
            PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, preferred_distance=600, clearance_pixels=32)
        )
        floor = np.zeros((800, 1600), np.uint8)
        cv.line(floor, (800, 400), (650, 475), 1, 100)
        cv.line(floor, (800, 400), (1300, 650), 1, 100)
        view = path.from_mask(floor, 1, 0, 1, (1600, 800), (0, 0, 1600, 800), (800, 400))
        flow = ArcaneRouteFlow(detour=(-1, -0.5))
        choice = flow.choose(path, view, "north", PathChoice((1250, 600), "old_entry"), 1)
        self.assertEqual("observed_corner_probe", choice.reason)
        self.assertLess(choice.aim[0], 800)
        self.assertGreater(choice.aim[1], 400)

    def test_report_continuation_restores_only_bend_after_current_scene_match(self):
        frame = np.random.RandomState(12).randint(0, 200, (1140, 1922, 3), dtype=np.uint8)
        report = dict(
            continuous=True,
            wing="north",
            return_from="south",
            navigation=dict(route_detour=[-400, -200], position=[99999, 99999], continuous_frames=[dict(ground=dict(point=[960, 580]))]),
        )
        with tempfile.TemporaryDirectory() as temporary:
            owner = Path(temporary)
            path = owner / "report.json"
            path.write_text(json.dumps(report), encoding="utf-8")
            cv.imwrite(str(owner / "continuous-0001-after.png"), frame)
            detour, measured = restore_route_flow(path, frame.copy(), wing="north", return_from="south")
            self.assertEqual((-400, -200), detour)
            self.assertGreaterEqual(measured["inliers"], 24)
            pending = cv.warpAffine(frame, np.float32([[1, 0, -86], [0, 1, 42]]), (1922, 1140))
            _, shifted = restore_route_flow(path, pending, wing="north", return_from="south")
            self.assertGreater(np.linalg.norm(shifted["displacement"]), 90)
            with self.assertRaisesRegex(RuntimeError, "current scene"):
                restore_route_flow(path, pending, wing="north", return_from="south", maximum_pending_distance=50)
            with self.assertRaisesRegex(RuntimeError, "current scene"):
                restore_route_flow(path, np.zeros_like(frame), wing="north", return_from="south")
            with self.assertRaisesRegex(RuntimeError, "contract"):
                restore_route_flow(path, frame, wing="east", return_from="south")
            report["navigation"].update(wing="north", return_from="north")
            path.write_text(json.dumps(report), encoding="utf-8")
            detour, _ = restore_route_flow(path, frame, wing="north", return_from="north")
            self.assertEqual((-400, -200), detour)
            with self.assertRaisesRegex(RuntimeError, "contract"):
                restore_route_flow(path, frame, wing="north", return_from="south")


if __name__ == "__main__":
    unittest.main()
