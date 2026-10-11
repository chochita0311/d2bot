import unittest

import cv2 as cv
import numpy as np

from diablo2.common.scene_motion import SceneFeatures
from diablo2.runs.summoner.navigation import ArcanePathObserver, PathSettings
from diablo2.runs.summoner.route_graph import ArcaneRouteGraph
from diablo2.runs.summoner.terrain_patterns import TerrainLanding


class ArcaneRouteGraphTests(unittest.TestCase):
    def test_actual_retreat_marks_the_branch_back_to_the_cap_after_arrival_at_another_place(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (400, -200), 2)
        cap = self.view(path, [(300, 650)], 2)
        self.assertEqual("graph_observed_exit_south", graph.choose(path, cap, "north", 2).reason)
        self.assertFalse(graph.retreated_branches)
        graph.observe(None, cap.anchor, (0, 0), 2.1)
        self.assertFalse(graph.retreated_branches)
        graph.observe(None, cap.anchor, (-400, 200), 3)
        junction = self.view(path, [(1300, 150), (300, 150)], 3)
        self.assertEqual("graph_observed_exit_west", graph.choose(path, junction, "north", 3).reason)
        self.assertEqual({(0, 0, "north")}, graph.retreated_branches)
        self.assertEqual("graph_retreat_branch_only", graph.choose(path, self.view(path, [(1300, 150)], 3.1), "north", 3.1).reason)

    def test_requested_retreat_does_not_close_a_branch_without_actual_matching_travel(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (400, -200), 2)
        graph.choose(path, self.view(path, [(300, 650)], 2), "north", 2)
        graph.observe(None, (800, 400), (400, 200), 3)
        self.assertFalse(graph.retreated_branches)
        self.assertIsNone(graph.pending_retreat)
        graph.fail_landing((300, 650), (800, 400))
        self.assertIsNone(graph.pending_retreat)

    def test_revisited_floor_keeps_interior_landing_while_failed_pixels_remain_excluded(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        view = self.view(path, [(300, 150)], 1)
        first = graph.choose(path, view, "west", 1)
        self.assertIsNotNone(first.aim)
        # 복귀/순환으로 다시 본 실제 바닥은 방문 이력만으로 사라지지 않는다.
        graph.places.append(tuple(a - b for a, b in zip(first.aim, view.anchor)))
        graph.place_charts.append(0)
        self.assertEqual(first.aim, graph.choose(path, view, "west", 1).aim)
        x, y = first.aim
        alternate = graph.choose(path, view, "west", 1, excluded=((x - 64, y - 64, x + 64, y + 64),))
        self.assertNotEqual(first.aim, alternate.aim)

    def test_current_wide_exit_does_not_reverse_just_because_motion_started_a_new_chart(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.traversed[(0, 0, "east")] = 8
        view = self.view(path, [(1300, 650), (300, 150)], 1)
        self.assertEqual("graph_observed_exit_west", graph.choose(path, view, "north", 1).reason)
        graph.observe(None, view.anchor, None, 2)
        graph.observe(None, view.anchor, None, 3)
        self.assertEqual(1, graph.chart)
        self.assertIsNone(graph.entry)
        graph.traversed[(1, graph.current_place, "west")] = 8
        view = self.view(path, [(1300, 650), (300, 150)], 3)
        choice = graph.choose(path, view, "north", 3)
        self.assertEqual("graph_observed_exit_west", choice.reason)
        graph.fail_landing(choice.aim, view.anchor)
        self.assertEqual("graph_observed_exit_east", graph.choose(path, view, "north", 3).reason)

    def test_exit_intent_allows_current_goal_reversal_and_a_closed_side_exit(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.exit_intent = "west"
        view = self.view(path, [(1300, 650), (300, 150)], 1)
        self.assertEqual("graph_observed_exit_east", graph.choose(path, view, "east", 1).reason)
        graph.exit_intent, graph.pending_exit = "west", None
        view = self.view(path, [(1300, 650)], 2)
        self.assertEqual("graph_observed_exit_east", graph.choose(path, view, "north", 2).reason)

    def test_visible_unfailed_forward_exit_is_not_reversed_for_a_lower_visit_count(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.traversed[(0, 0, "north")] = 8
        graph.pending_exit = "south"
        view = self.view(path, [(1300, 150), (300, 650)], 1)
        self.assertEqual("graph_observed_exit_north", graph.choose(path, view, "north", 1).reason)
        graph.fail_landing((1300, 150), view.anchor)
        self.assertEqual("graph_observed_exit_south", graph.choose(path, view, "north", 1).reason)

    def test_failed_forward_landing_tries_visible_side_exit_before_another_forward_pixel(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        view = self.view(path, [(1300, 150), (1300, 650)], 1)
        first = graph.choose(path, view, "north", 1)
        self.assertEqual("graph_observed_exit_north", first.reason)
        graph.fail_landing(first.aim, view.anchor)
        side = graph.choose(path, view, "north", 1)
        self.assertEqual("graph_observed_exit_east", side.reason)
        graph.observe(None, view.anchor, (400, 200), 2)
        next_view = self.view(path, [(1300, 150), (1300, 650)], 2)
        self.assertEqual("graph_observed_exit_north", graph.choose(path, next_view, "north", 2).reason)
        self.assertEqual(1, graph.failed_exits[(0, 0, "north")])

    def test_current_central_waypoint_uses_requested_wing_despite_traversal_history(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.traversed[(0, 0, "north")] = 8
        graph.entry, graph.pending_exit = "north", "south"
        view = self.view(path, [(1300, 150), (1300, 650), (300, 650), (300, 150)], 1)
        choice = graph.choose(path, view, "north", 1, hub=(800, 400))
        self.assertEqual("current_hub_departure_north", choice.reason)
        self.assertGreater(choice.aim[0], 800)
        self.assertLess(choice.aim[1], 400)
        self.assertIsNone(graph.pending_exit)
        self.assertIsNone(graph.entry)
        self.assertEqual(8, graph.traversed[(0, 0, "north")])

    def test_central_departure_does_not_substitute_another_wing_when_exit_missing(self):
        path, graph = self.path(), ArcaneRouteGraph()
        view = self.view(path, [(300, 650)], 1)
        choice = graph.choose(path, view, "north", 1, hub=(800, 400))
        self.assertIsNone(choice.aim)
        self.assertEqual("hub_wing_exit_unobserved", choice.reason)
        self.assertEqual("graph_frame_stale", graph.choose(path, view, "north", 1.25, hub=(800, 400)).reason)

    def test_distant_central_observation_does_not_override_local_corner(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (-400, -200), 2)
        view = self.view(path, [(1300, 650), (300, 650)], 2)
        self.assertEqual("graph_observed_exit_south", graph.choose(path, view, "north", 2, hub=(300, 650)).reason)

    def features(self, at, translation=(0, 0), seed=42):
        random = np.random.RandomState(seed)
        points = random.uniform((120, 180), (1720, 890), (100, 2)).astype(np.float32)
        descriptors = random.randint(0, 256, (100, 32)).astype(np.uint8)
        return SceneFeatures(int(at * 10), at, (1922, 1140), points + translation, descriptors)

    def path(self):
        return ArcanePathObserver(
            PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, preferred_distance=600, clearance_pixels=32)
        )

    def view(self, path, directions, at):
        floor = np.zeros((800, 1600), np.uint8)
        for point in directions:
            cv.line(floor, (800, 400), point, 1, 130)
        return path.from_mask(floor, 1, 0, at, (1600, 800), (0, 0, 1600, 800), (800, 400))

    def test_motion_gap_retains_places_and_recognizes_return_without_a_new_origin(self):
        graph = ArcaneRouteGraph()
        graph.observe(self.features(1), (960, 580), None, 1)
        graph.observe(self.features(2, (-500, 250)), (960, 580), (500, -250), 2)
        places = tuple(graph.places)
        graph.observe(self.features(3, seed=99), (960, 580), None, 3)
        self.assertFalse(graph.localized)
        self.assertEqual(places, tuple(graph.places))
        self.assertTrue(graph.observe(self.features(4, (-20, 10)), (960, 580), None, 4))
        self.assertTrue(graph.localized)
        np.testing.assert_allclose(graph.position, (20, -10), atol=0.01)
        self.assertEqual(1, graph.relocalizations)
        self.assertEqual(2, len(graph.places))
        self.assertIsNone(graph.entry)

    def test_entry_is_from_observed_translation_and_corner_selects_forward_exit(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (-400, -200), 2)
        self.assertEqual("east", graph.entry)
        view = self.view(path, [(1300, 650), (1300, 150)], 2)
        choice = graph.choose(path, view, "north", 2)
        self.assertGreater(choice.aim[0], 800)
        self.assertLess(choice.aim[1], 400)
        self.assertEqual("graph_observed_exit_north", choice.reason)
        self.assertEqual("corner_candidate", graph.kind)
        self.assertEqual(1, len(graph.edges))

    def test_single_observed_entry_allows_one_retreat_but_not_a_repeated_or_failed_exit(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (400, -200), 2)
        view = self.view(path, [(300, 650)], 2)
        choice = graph.choose(path, view, "north", 2)
        self.assertEqual("graph_observed_exit_south", choice.reason)
        key = (graph.chart, graph.current_place, "south")
        graph.traversed[key] = 1
        self.assertIsNone(graph.choose(path, view, "north", 2).aim)
        graph.traversed.clear()
        graph.fail_landing(choice.aim, view.anchor)
        self.assertEqual("graph_entry_retreat_exhausted", graph.choose(path, view, "north", 2).reason)

    def test_wrong_turn_can_reverse_entry_when_it_is_the_actual_goal_direction(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (-400, 200), 2)
        self.assertEqual("north", graph.entry)
        # live120은 실제 북쪽 후보가 있어도 entry=north 때문에 동쪽 실패 후보를 반복했다.
        view = self.view(path, [(1300, 150), (1300, 650), (300, 650)], 2)
        choice = graph.choose(path, view, "north", 2)
        self.assertEqual("graph_observed_exit_north", choice.reason)
        self.assertGreater(choice.aim[0], 800)
        self.assertLess(choice.aim[1], 400)

    def test_local_southwest_corner_is_allowed_when_actual_entry_is_southeast(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (-400, -200), 2)
        choice = graph.choose(path, self.view(path, [(1300, 650), (300, 650)], 2), "north", 2)
        self.assertEqual("graph_observed_exit_south", choice.reason)
        self.assertLess(choice.aim[0], 800)
        self.assertGreater(choice.aim[1], 400)

    def test_short_forward_end_does_not_hide_wide_side_exit(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        view = self.view(path, [(650, 475), (1300, 650)], 1)
        choice = graph.choose(path, view, "south", 1)
        self.assertEqual("graph_observed_exit_east", choice.reason)
        self.assertGreater(np.linalg.norm(np.asarray(choice.aim) - (800, 400)), 300)
        # 넓은 옆길이 없으면 좁은 실제 꺾임 후보를 모두 금지하지 않는다.
        choice = graph.choose(path, self.view(path, [(650, 475)], 2), "south", 2)
        self.assertIsNotNone(choice.aim)
        self.assertEqual("graph_observed_exit_south", choice.reason)

    def test_wide_observed_stairs_landing_is_not_replaced_by_short_failed_portal_side(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        view = self.view(path, [(960, 440)], 1)
        landing = TerrainLanding("stairs-descend-south", (185, 236), 0.97, (127, 185, 252, 263))
        self.assertLess(np.linalg.norm(np.asarray(landing.point) - view.anchor), 800)
        self.assertGreater(np.linalg.norm(np.asarray(landing.point) - view.anchor), 600)
        self.assertNotEqual(landing.point, graph.choose(path, view, "south", 1, landings=(landing,)).aim)
        path.teleport["maximum_distance"] = 800
        path.teleport["preferred_distance"] = 800
        self.assertEqual(landing.point, graph.choose(path, view, "south", 1, landings=(landing,)).aim)

    def test_revisited_place_uses_untraversed_exit_and_holds_intent_until_actual_move(self):
        path, graph = self.path(), ArcaneRouteGraph()
        graph.observe(None, (800, 400), None, 1)
        graph.observe(None, (800, 400), (400, -200), 2)
        graph.observe(None, (800, 400), (-400, 200), 3)
        view = self.view(path, [(1300, 150), (1300, 650)], 3)
        choice = graph.choose(path, view, "north", 3)
        self.assertEqual("graph_observed_exit_east", choice.reason)
        graph.observe(None, (800, 400), (0, 0), 3.1)
        view = self.view(path, [(1300, 150), (1300, 650)], 3.1)
        self.assertEqual("graph_observed_exit_east", graph.choose(path, view, "north", 3.1).reason)
        graph.fail_landing(choice.aim, view.anchor)
        self.assertIsNone(graph.pending_exit)

    def test_stale_duplicate_and_missing_descriptors_do_not_relocate(self):
        graph = ArcaneRouteGraph()
        graph.observe(self.features(1), (960, 580), None, 1)
        graph.observe(self.features(2), (960, 580), (400, -200), 2)
        self.assertFalse(graph.observe(self.features(1), (960, 580), None, 1))
        self.assertEqual((400, -200), graph.position)
        graph.observe(SceneFeatures(30, 3, (1922, 1140), np.empty((0, 2)), None), (960, 580), None, 3)
        self.assertFalse(graph.localized)
        self.assertEqual(2, len(graph.references))

    def test_same_place_reacquisition_preserves_verified_entry(self):
        graph = ArcaneRouteGraph()
        graph.observe(self.features(1), (960, 580), None, 1)
        graph.observe(self.features(2, (-400, 200)), (960, 580), (400, -200), 2)
        self.assertEqual("south", graph.entry)
        graph.observe(self.features(3, (-402, 201)), (960, 580), None, 3)
        self.assertEqual("south", graph.entry)
        graph.observe(self.features(4), (960, 580), None, 4)
        self.assertIsNone(graph.entry)

    def test_feature_and_place_retention_is_bounded_and_holds_no_raw_images(self):
        graph = ArcaneRouteGraph(maximum_references=3, maximum_places=4)
        for at in range(1, 20):
            graph.observe(self.features(at, (-400 * (at - 1), 200 * (at - 1))), (960, 580), (400, -200), at)
        self.assertEqual(3, len(graph.references))
        self.assertEqual(4, len(graph.places))
        self.assertLessEqual(graph.summary()["feature_bytes"], 3 * 100 * 48)
        self.assertTrue(all(not hasattr(item, "frame") for item in graph.references))

    def test_relocalization_adjusts_for_current_player_anchor(self):
        graph = ArcaneRouteGraph()
        graph.observe(self.features(1), (960, 580), None, 1)
        graph.observe(self.features(2, (-100, 50)), (968, 584), None, 2)
        np.testing.assert_allclose(graph.position, (108, -46), atol=0.01)

    def test_unmatched_new_scene_gets_separate_chart_and_preserves_old_reference(self):
        graph = ArcaneRouteGraph()
        graph.observe(self.features(1), (960, 580), None, 1)
        graph.observe(self.features(2, seed=99), (960, 580), None, 2)
        self.assertTrue(graph.observe(self.features(3, seed=99), (960, 580), None, 3))
        self.assertEqual(1, graph.chart)
        self.assertEqual([0, 1], graph.place_charts)
        self.assertEqual(2, len(graph.references))
        self.assertEqual(0, len(graph.edges))
        self.assertTrue(graph.observe(self.features(4), (960, 580), None, 4))
        self.assertEqual(0, graph.chart)
        self.assertEqual(2, len(graph.places))


if __name__ == "__main__":
    unittest.main()
