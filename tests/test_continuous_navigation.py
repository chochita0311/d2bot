import threading
import time
import unittest
from dataclasses import dataclass, replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np

from diablo2.common.config import load_config
from diablo2.common.field_control import DryFieldInput, FieldDirective, HeldFieldInput
from diablo2.common.supervised_field import RecordingInput
from diablo2.common.scene_motion import CameraMotion
from diablo2.runs.summoner.continuous_navigation import (
    run_continuous_route,
    validate_travel_permit,
    validate_live_continuation,
    validate_terminal_return,
    validate_hub_departure,
)
from diablo2.runs.summoner.navigation import PathChoice, PathView
from diablo2.runs.summoner.scene_vision import GroundReading, HoverCandidate
from diablo2.runs.summoner.route_graph import ArcaneRouteGraph
from diablo2.runs.summoner.terminal_vision import TerminalReading
from diablo2.runs.summoner.objective_vision import ObjectiveReading


@dataclass
class Enemy:
    ground: tuple


class ContinuousNavigationTests(unittest.TestCase):
    def test_proposed_near_actor_requires_fresh_hover_panel_and_combat_on(self):
        from diablo2.runs.summoner.scene_vision import HostileTarget

        for combat, panel in ((True, True), (True, False), (False, True)):
            with self.subTest(combat=combat, panel=panel):
                self.setUp()
                target = Mock()
                target.observe.side_effect = lambda frame: (
                    HostileTarget("hostile-target", (800, 80, 1100, 110), None) if panel and self.backend.pointer == (1120, 520) else None
                )
                with patch("diablo2.runs.summoner.continuous_navigation.nearby_actor_proposals", return_value=((1120, 520),)):
                    result = self.run_interval(engage_monsters=combat, target_vision=target)
                events = list(self.backend.events)
                attacked = ("down", "f4") in events
                self.assertEqual(combat and panel, attacked)
                if attacked:
                    moved = events.index(("move", 1120, 520))
                    self.assertGreater(events.index(("down", "f4")), moved)
                    self.assertTrue(any(row["nearby_hover_confirmed"] for row in result["continuous_frames"]))
                if not combat:
                    self.assertNotIn(("move", 1120, 520), events)
                    target.observe.assert_not_called()

    def test_current_frame_preflight_keeps_f2_through_long_analysis_and_vetoes_obstructions(self):
        for obstruction in (None, "enemy", "hover", "floor"):
            with self.subTest(obstruction=obstruction):
                self.setUp()
                owned = []

                def create(*args, **kwargs):
                    instance = HeldFieldInput(*args, **kwargs)
                    owned.append(instance)
                    return instance

                def advance(seconds):
                    self.at += seconds
                    if owned:
                        owned[0].service(self.at)

                def grab():
                    advance(0.01)
                    self.count += 1
                    self.frame[:] = self.count * 10
                    return SimpleNamespace(frame=self.frame)

                def plan(*args, **kwargs):
                    advance(0.06)
                    choice, ground, view = self.plan(*args, **kwargs)
                    if obstruction == "floor" and self.count >= 3:
                        view.floor[30:45, 175:190] = False
                    return choice, ground, replace(view, captured_at=args[4])

                def measured(*args, **kwargs):
                    advance(0.08)
                    return CameraMotion((0, 0), 24, "test")

                self.capture.grab.side_effect = grab
                if obstruction == "enemy":
                    self.hazards.observe.side_effect = lambda *args, **kwargs: (Enemy((1000, 600)),) if self.count >= 3 else ()
                elif obstruction == "hover":
                    self.hover.observe.side_effect = (
                        lambda *args: (HoverCandidate("chest", (1400, 250, 1500, 350), 0.99),) if self.count >= 3 else ()
                    )
                with patch("diablo2.runs.summoner.continuous_navigation.HeldFieldInput", side_effect=create), patch(
                    "diablo2.runs.summoner.continuous_navigation.threading", SimpleNamespace(Event=threading.Event, Thread=Mock())
                ):
                    result = self.run_interval(seconds=1, plan=plan, motion=measured, engage_monsters=obstruction == "enemy")
                rows = result["continuous_frames"]
                self.assertTrue(any(row["lease_renewed_at"] is not None for row in rows))
                if obstruction is None:
                    self.assertEqual(1, list(self.backend.events).count(("up", "f2")))
                    self.assertTrue(all(row["lease_renewed_at"] is not None for row in rows[1:]))
                else:
                    self.assertTrue(all(row["lease_renewed_at"] is None for row in rows[2:]))
                self.assertFalse(owned[0].held)

    def test_lease_renewal_cannot_restart_a_key_or_bridge_expiry_or_pointer_changes(self):
        backend, at = DryFieldInput(), [0.0]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0])
        self.assertFalse(held.renew_lease("f2", (1200, 300), 0.25, at[0]))
        held.apply(FieldDirective("travel", "current", aim=(1200, 300), deadline=0.25), at[0])
        at[0] = 0.2
        self.assertTrue(held.renew_lease("f2", (1200, 300), 0.4, at[0]))
        self.assertEqual([("move", 1200, 300), ("down", "f2")], list(backend.events))
        at[0] = 0.4
        self.assertFalse(held.renew_lease("f2", (1200, 300), 0.6, at[0]))
        self.assertFalse(held.held)
        held.apply(FieldDirective("travel", "new", aim=(1200, 300), deadline=0.6), at[0])
        backend.pointer = (500, 300)
        self.assertFalse(held.renew_lease("f2", (1200, 300), 0.65, at[0]))
        self.assertFalse(held.held)
        held.apply(FieldDirective("travel", "new", aim=(1200, 300), deadline=0.6), at[0])

        def slow_pointer(point):
            at[0] = 0.61
            return True

        backend.pointer_matches = slow_pointer
        self.assertFalse(held.renew_lease("f2", (1200, 300), 0.65, 0.5))
        self.assertFalse(held.held)

    def test_hover_avoids_actual_cursor_as_well_as_the_label_and_waits_for_a_fresh_frame(self):
        self.hover.observe.side_effect = lambda *args: (HoverCandidate("chest", (1478, 215, 1537, 244), 0.97),) if self.count == 1 else ()
        result = self.run_interval()
        first = result["continuous_frames"][0]
        self.assertEqual("hover_reaim_only", first["action"])
        self.assertEqual((1450, 300), first["hovered_aim_excluded"])
        exclusions = [call[1]["excluded"] for call in self.path.choose.call_args_list if call[1].get("excluded")]
        self.assertTrue(any(left <= 1450 < right and top <= 300 < bottom for boxes in exclusions for left, top, right, bottom in boxes))
        self.assertEqual((480, 300), first["hover_alternate"]["aim"])
        self.assertEqual(("move", 480, 300), self.backend.events[0])
        self.assertIn(("down", "f2"), list(self.backend.events)[1:])
        self.assertTrue(any(row["action"] == "hold_teleport" for row in result["continuous_frames"][1:]))

    def test_landing_pair_can_extend_floor_reacquisition_but_cannot_approve_arrival(self):
        for recovered in (True, False):
            with self.subTest(recovered=recovered):
                self.setUp()
                hub = Mock()
                hub.observe.side_effect = lambda frame: (960, 503) if self.count <= 2 or (recovered and self.at >= 3) else None
                hub.last_evidence = dict(kind="blue_pair_and_floor_ornaments", parts=[dict(id="left"), dict(id="upper-left")])
                hub.confirm_static_pair.return_value = None
                hub.current_pair_near.return_value = (961, 504)
                result = self.run_interval(seconds=8, return_from="north", hub_vision=hub)
                self.assertFalse(self.backend.events)
                if recovered:
                    self.assertEqual("hub_arrived", result["status"])
                    self.assertGreaterEqual(result["continuous_frames"][-1]["processed_at"], 3.6)
                else:
                    self.assertEqual("central_reacquisition_required", result["continuous_status"])
                    self.assertGreaterEqual(result["continuous_frames"][-1]["processed_at"], 6)
                    self.assertFalse(any(row["action"] == "central_arrival_hold" for row in result["continuous_frames"][2:]))

    def test_transient_near_pair_keeps_only_bounded_static_wait_and_requires_full_recovery(self):
        hub = Mock()
        hub.observe.side_effect = lambda frame: (960, 503) if self.count <= 2 or self.at >= 3 else None
        hub.last_evidence = dict(kind="blue_pair_and_floor_ornaments", parts=[dict(id="left"), dict(id="upper-left")])
        hub.confirm_static_pair.return_value = None
        hub.current_pair_near.side_effect = lambda *args: (961, 504) if self.count == 4 else None
        result = self.run_interval(seconds=8, return_from="north", hub_vision=hub)
        self.assertEqual("hub_arrived", result["status"])
        self.assertFalse(self.backend.events)
        waits = [row for row in result["continuous_frames"] if row["action"] == "central_reacquire_hold"]
        self.assertTrue(any(row["current_near_blue_pair"] is None and row["central_reacquisition_limit"] == 6 for row in waits))
        self.assertGreaterEqual(result["continuous_frames"][-1]["processed_at"], 3.6)

    def test_supervised_central_resume_cannot_depart_when_current_waypoint_is_missing(self):
        hub = Mock()
        hub.observe.return_value = None
        hub.confirm_static_pair.return_value = None
        hub.current_pair_near.return_value = None
        self.proof["at_central_waypoint"] = True
        result = self.run_interval(seconds=3, return_from="north", hub_vision=hub)
        self.assertEqual("central_reacquisition_required", result["continuous_status"])
        self.assertFalse(self.backend.events)
        self.assertFalse(any(row["action"] == "central_arrival_hold" for row in result["continuous_frames"]))

    def test_original_central_proof_does_not_hold_a_later_terminal_return(self):
        hub = Mock()
        hub.observe.return_value = None
        hub.confirm_static_pair.return_value = None
        self.proof["at_central_waypoint"] = True
        previous = dict(
            status="wing_end_observed",
            continuous_status="current_chest_trio_and_player_settled",
            wing="north",
            return_from=None,
            terminal=dict(kind="nonboss_chest_trio"),
            continuous_frames=[dict(processed_at=0, action="terminal_arrival_hold")],
        )
        result = self.run_interval(return_from="north", hub_vision=hub, transition_from=previous)
        self.assertIn(("down", "f2"), self.backend.events)
        self.assertFalse(any(row["action"] == "central_reacquire_hold" for row in result["continuous_frames"]))

    def test_outbound_passes_current_central_evidence_to_graph_departure(self):
        hub = Mock()
        hub.observe.return_value = (960, 580)
        self.planner[0].observe.return_value = GroundReading((960, 580), "current")
        self.path.choose.return_value = PathChoice((1450, 300), "current_north_floor")
        graph = ArcaneRouteGraph()
        graph.pending_exit = "south"
        result = self.run_interval(hub_vision=hub, route_memory=graph)
        self.assertTrue(all(row["choice"]["reason"] == "current_hub_departure_north" for row in result["continuous_frames"]))
        self.assertIn(("down", "f2"), self.backend.events)
        self.assertTrue(all(event[1] > 960 and event[2] < 580 for event in self.backend.events if event[0] == "move"))

    def test_hub_arrival_can_settle_on_current_pair_after_transient_floor_occlusion(self):
        hub = Mock()
        hub.observe.side_effect = lambda frame: (960, 503) if self.count <= 2 else None
        hub.last_evidence = dict(kind="blue_pair_and_floor_ornaments", parts=[dict(id="left"), dict(id="upper-left")])
        hub.confirm_static_pair.return_value = (960, 503)
        result = self.run_interval(seconds=2, return_from="north", hub_vision=hub)
        self.assertEqual("hub_arrived", result["status"])
        self.assertEqual("recent_floor_and_current_blue_pair", result["continuous_frames"][-1]["hub_evidence"]["kind"])
        self.assertGreaterEqual(result["continuous_frames"][-1]["arrival_hold_seconds"], 0.64)
        self.assertFalse(self.backend.events)

    def test_objective_stops_outbound_before_ground_or_central_gate_without_claiming_kill(self):
        objective = Mock()
        objective.observe.return_value = ObjectiveReading("summoner_platform", (400, 300, 537, 437), 0.96, ())
        hub = Mock()
        hub.observe.return_value = None
        result = self.run_interval(objective_vision=objective, hub_vision=hub, plan=Mock(side_effect=RuntimeError("ground_unverified")))
        self.assertEqual("summoner_platform_observed", result["status"])
        self.assertEqual("summoner_platform_hold", result["continuous_frames"][-1]["action"])
        self.assertFalse(self.backend.events)
        self.assertNotIn("dead", result["objective"])

    def test_objective_does_not_interrupt_return_to_hub(self):
        objective = Mock()
        objective.observe.return_value = ObjectiveReading("summoner_platform", (400, 300, 537, 437), 0.96, ())
        result = self.run_interval(objective_vision=objective, return_from="north")
        self.assertNotEqual("summoner_platform_observed", result["status"])
        objective.observe.assert_not_called()
        self.assertIn(("down", "f2"), self.backend.events)

    def test_next_wing_requires_fresh_settled_hub_and_ordered_outbound(self):
        previous = dict(
            status="hub_arrived",
            continuous_status="central_waypoint_and_player_settled",
            wing="north",
            return_from="north",
            continuous_frames=[dict(processed_at=100, action="central_arrival_hold")],
        )
        validate_hub_departure(previous, "east", None, 100.2)
        for changes, wing, return_from, now in (
            ({}, "south", None, 100.2),
            ({}, "east", "north", 100.2),
            ({}, "east", None, 100.5),
            ({}, "east", None, 99.9),
            (dict(status="field_paused"), "east", None, 100.2),
            (dict(return_from=None), "east", None, 100.2),
            (dict(wing="west", return_from="west"), "north", None, 100.2),
            (dict(continuous_frames=[dict(processed_at=100, action="hold_teleport")]), "east", None, 100.2),
        ):
            with self.assertRaises(ValueError):
                validate_hub_departure(dict(previous, **changes), wing, return_from, now)

    def test_terminal_return_requires_fresh_settled_trio_in_same_wing(self):
        previous = dict(
            status="wing_end_observed",
            continuous_status="current_chest_trio_and_player_settled",
            wing="north",
            return_from=None,
            terminal=dict(kind="nonboss_chest_trio"),
            continuous_frames=[dict(processed_at=100, action="terminal_arrival_hold")],
        )
        validate_terminal_return(previous, "north", "north", 100.2)
        alternate = dict(
            previous,
            terminal=dict(kind="nonboss_chest_platform"),
            continuous_status="current_terminal_landmarks_and_player_settled",
        )
        validate_terminal_return(alternate, "north", "north", 100.2)
        with self.assertRaises(ValueError):
            validate_terminal_return(dict(alternate, continuous_status="current_chest_trio_and_player_settled"), "north", "north", 100.2)
        for changes, wing, return_from, now in (
            ({}, "north", "north", 100.5),
            ({}, "north", "north", 99.9),
            ({}, "east", "east", 100.2),
            ({}, "north", "south", 100.2),
            (dict(status="field_paused"), "north", "north", 100.2),
            (dict(terminal=dict(kind="single_chest")), "north", "north", 100.2),
            (dict(continuous_frames=[dict(processed_at=100, action="hold_teleport")]), "north", "north", 100.2),
        ):
            with self.assertRaises(ValueError):
                validate_terminal_return(dict(previous, **changes), wing, return_from, now)

    def test_live_continuation_requires_immediate_success_in_same_route(self):
        progress = dict(
            continuous_status="interval_complete", wing="north", return_from="south", continuous_frames=[dict(processed_at=100)]
        )
        validate_live_continuation(progress, "north", "south", 100.2)
        for changes, wing, return_from, now in (
            ({}, "north", "south", 100.5),
            ({}, "north", "south", 99.9),
            ({}, "east", "south", 100.2),
            ({}, "north", "north", 100.2),
            (dict(continuous_status="buff_renewal_due"), "north", "south", 100.2),
            (dict(continuous_frames=[]), "north", "south", 100.2),
        ):
            with self.assertRaises(ValueError):
                validate_live_continuation(dict(progress, **changes), wing, return_from, now)

    def setUp(self):
        self.profile = load_config(Path(__file__).resolve().parents[1] / "config").characters["flash"]
        self.at, self.count = 0.0, 0
        self.frame = np.zeros((1140, 1922, 3), np.uint8)
        self.capture = Mock(target={"left": 0, "top": 0})

        def grab():
            self.at += 0.1
            self.count += 1
            self.frame[180:890, 120:1720] = self.count * 10 % 255
            return SimpleNamespace(frame=self.frame)

        self.capture.grab.side_effect = grab
        self.backend, self.evidence = DryFieldInput(), []
        self.path = Mock(teleport={"maximum_distance": 600, "preferred_distance": 240, "clearance_pixels": 32})
        self.path.choose.return_value = PathChoice((480, 300), "observed_local_floor")
        self.proof = dict(
            character_id="flash", field=True, monsters_clear=False, belt_closed=True, potions_remaining=3, observed_at=time.time()
        )
        self.hazards, self.hover = Mock(), Mock()
        self.hazards.observe.return_value = ()
        self.hover.observe.return_value = ()
        self.planner = (Mock(reset_events=(), center=None), self.path)

    def plan(self, *args, **kwargs):
        view = PathView(1, 0, self.at, (1922, 1140), (0, 0, 1920, 896), (960, 580), 8, np.ones((112, 240), bool))
        return PathChoice((1450, 300), "teleport_floor"), GroundReading((960, 580), "current"), view

    def run_interval(self, seconds=1, **kwargs):
        hud = Mock()
        hud.observe.return_value = SimpleNamespace(
            life=SimpleNamespace(ratio=1.0), mana=SimpleNamespace(ratio=1.0), belt_visibility="closed"
        )
        with patch("diablo2.runs.summoner.continuous_navigation.resource_guard", return_value=None), patch(
            "diablo2.runs.summoner.continuous_navigation.plan_arcane_step", side_effect=kwargs.pop("plan", self.plan)
        ), patch("diablo2.runs.summoner.continuous_navigation.BackgroundMotionObserver") as motion:
            motion.return_value.observe.side_effect = kwargs.pop("motion", None)
            motion.return_value.observe.return_value = CameraMotion((0, 0), 20, "verified")
            motion.return_value.features = None
            return run_continuous_route(
                self.capture,
                hud,
                self.backend,
                kwargs.pop("focused", lambda: True),
                self.profile,
                threading.Event(),
                self.planner,
                self.proof,
                seconds,
                evidence=self.evidence,
                hazards=self.hazards,
                hover_vision=self.hover,
                reference_seconds=0.32,
                clock=lambda: self.at,
                emit=lambda message: None,
                **kwargs,
            )

    def test_wide_steering_keeps_f2_down_across_fresh_frames(self):
        def plan(*args, **kwargs):
            _, ground, view = self.plan(*args, **kwargs)
            return PathChoice((1450 - (self.count // 3) * 8, 300), "wide_current_floor"), ground, view

        result = self.run_interval(plan=plan)
        events = list(self.backend.events)
        self.assertEqual("north_segment_observed", result["status"])
        self.assertEqual(600, self.path.teleport["preferred_distance"])
        self.assertGreaterEqual(events.count(("down", "f2")), 2)
        self.assertEqual(1, events.count(("up", "f2")))
        self.assertGreaterEqual(sum(event[0] == "move" for event in events), 2)
        self.assertLess(sum(event[0] == "move" for event in events), len(result["continuous_frames"]))
        self.assertEqual(("up", "f2"), events[-1])
        self.assertFalse(any(event[0] == "press" for event in events))

    def test_transient_pose_gap_consumes_existing_lease_without_renewing_it(self):
        states = []

        def plan(*args, **kwargs):
            if 3 <= self.count <= 6:
                states.append((self.count, list(self.backend.events)))
                raise RuntimeError("ground_unverified")
            return self.plan(*args, **kwargs)

        self.run_interval(plan=plan)
        # 0.2초의 확인 화면이 준 0.45초 시한: 0.3/0.4에는 유지, 0.5에는 해제.
        self.assertNotIn(("up", "f2"), states[1][1])
        self.assertIn(("up", "f2"), states[3][1])
        self.assertEqual(1, states[3][1].count(("down", "f2")))

    def test_graph_model_controls_held_input_and_keeps_memory_on_unknown_motion(self):
        graph = ArcaneRouteGraph()
        calls = [0]

        def measured(*args, **kwargs):
            calls[0] += 1
            return CameraMotion((0, 0) if calls[0] < 3 else None, 24, "test")

        result = self.run_interval(route_memory=graph, motion=measured)
        self.assertEqual(2, result["route_graph"]["places"])
        self.assertEqual([0, 1], graph.place_charts)
        self.assertEqual(0, result["route_graph"]["edges"])
        self.assertEqual((0.0, 0.0), graph.position)
        self.assertEqual(0, result.get("position_generation", 0))
        self.assertTrue(all(row["choice"]["reason"].startswith("graph_") for row in result["continuous_frames"]))
        self.assertIn(("down", "f2"), self.backend.events)
        self.assertEqual(("up", "f2"), self.backend.events[-1])

    def test_near_enemy_releases_f2_before_f4_and_resumes_without_overlap(self):
        self.hazards.observe.side_effect = lambda *args, **kwargs: (Enemy((1000, 600)),) if 3 <= self.count <= 5 else ()
        result = self.run_interval(seconds=3)
        held = set()
        for event in self.backend.events:
            if event[0] == "down":
                self.assertTrue(not held or held == {event[1]})
                held.add(event[1])
            elif event[0] == "up":
                held.remove(event[1])
        self.assertFalse(held)
        self.assertGreaterEqual(list(self.backend.events).count(("down", "f4")), 1)
        self.assertEqual(2, list(self.backend.events).count(("up", "f2")))
        self.assertIn("hold_nova", [row["action"] for row in result["continuous_frames"]])

    def test_separate_pose_gaps_during_combat_do_not_count_as_sustained_loss(self):
        self.hazards.observe.side_effect = lambda *args, **kwargs: (Enemy((1000, 600)),) if self.count >= 3 else ()

        def plan(*args, **kwargs):
            if self.count in (4, 22):
                raise RuntimeError("camera_actor_displaced")
            return self.plan(*args, **kwargs)

        result = self.run_interval(seconds=3, plan=plan)
        self.assertEqual("interval_complete", result["continuous_status"])
        self.assertIn("hold_nova", [row["action"] for row in result["continuous_frames"]])

    def test_sustained_pose_loss_during_combat_still_stops_input(self):
        self.hazards.observe.return_value = (Enemy((1000, 600)),)

        def plan(*args, **kwargs):
            if self.count >= 3:
                raise RuntimeError("camera_actor_displaced")
            return self.plan(*args, **kwargs)

        result = self.run_interval(seconds=3, plan=plan)
        self.assertEqual("camera_actor_displaced", result["continuous_status"])
        self.assertEqual(("up", "f4"), self.backend.events[-1])

    def test_far_enemy_does_not_interrupt_teleport(self):
        self.hazards.observe.return_value = (Enemy((1500, 300)),)
        self.run_interval()
        self.assertNotIn(("down", "f4"), self.backend.events)

    def test_positive_terminal_waits_for_pending_cast_and_current_settling(self):
        terminal = Mock()
        terminal.observe.return_value = TerminalReading("nonboss_chest_trio", (1100, 600), (), ())
        motion = lambda *args, **kwargs: CameraMotion((300, -150) if self.count == 1 else (0, 0), 24, "test")
        result = self.run_interval(terminal_vision=terminal, motion=motion)
        self.assertEqual("wing_end_observed", result["status"])
        self.assertGreaterEqual(result["continuous_frames"][-1]["arrival_hold_seconds"], 0.64)
        self.assertNotIn(("down", "f2"), self.backend.events)
        self.assertEqual("nonboss_chest_trio", result["terminal"]["kind"])

    def test_distant_terminal_and_return_route_do_not_claim_wing_end(self):
        terminal = Mock()
        terminal.observe.return_value = TerminalReading("nonboss_chest_trio", (1700, 600), (), ())
        self.path.choose.return_value = PathChoice((1700, 600), "observed_terminal_approach")
        result = self.run_interval(terminal_vision=terminal)
        self.assertNotEqual("wing_end_observed", result["status"])
        self.assertTrue(result["terminal_acquired"])
        self.assertEqual(1, list(self.backend.events).count(("press", "f2")))
        self.assertNotIn(("down", "f2"), self.backend.events)
        terminal.reset_mock()
        self.path.choose.return_value = PathChoice((480, 300), "observed_local_floor")
        result = self.run_interval(terminal_vision=terminal, return_from="north")
        terminal.observe.assert_not_called()
        self.assertEqual("north_segment_observed", result["status"])

    def test_terminal_occlusion_holds_input_until_fresh_settled_reacquisition(self):
        terminal = Mock()
        terminal.observe.side_effect = lambda frame: (
            None if 4 <= self.count <= 6 else TerminalReading("nonboss_chest_platform", (1100, 600), (), ())
        )
        result = self.run_interval(seconds=2, terminal_vision=terminal)
        self.assertEqual("wing_end_observed", result["status"])
        self.assertEqual("current_terminal_landmarks_and_player_settled", result["continuous_status"])
        self.assertIn("terminal_reacquire_hold", [row["action"] for row in result["continuous_frames"]])
        self.assertFalse(self.backend.events)

    def test_terminal_preempts_held_travel_before_ground_can_be_read(self):
        terminal = Mock()
        terminal.observe.side_effect = lambda frame: (
            None if self.count < 3 else TerminalReading("nonboss_chest_platform", (1400, 600) if self.count == 3 else (1100, 600), (), ())
        )

        def temporarily_obscured_ground(*args, **kwargs):
            if self.count in (3, 4):
                raise RuntimeError("ground_unverified")
            return self.plan(*args, **kwargs)

        result = self.run_interval(seconds=3, terminal_vision=terminal, plan=temporarily_obscured_ground)
        self.assertEqual("wing_end_observed", result["status"])
        events = list(self.backend.events)
        released = events.index(("up", "f2"))
        self.assertNotIn(("down", "f2"), events[released + 1 :])
        self.assertNotIn(("press", "f2"), events[released + 1 :])

    def test_lost_terminal_never_restarts_general_exploration(self):
        terminal = Mock()
        terminal.observe.side_effect = lambda frame: TerminalReading("nonboss_chest_trio", (1100, 600), (), ()) if self.count <= 2 else None
        result = self.run_interval(seconds=3, terminal_vision=terminal)
        self.assertEqual("terminal_reacquisition_required", result["continuous_status"])
        self.assertNotEqual("wing_end_observed", result["status"])
        self.assertFalse(self.backend.events)

    def test_motion_correspondence_allows_closing_approach_without_terminal_approval(self):
        terminal = Mock()
        full = TerminalReading(
            "nonboss_chest_platform", (1100, 600), (), (0.99, 0.99, 0.99), ("floor", "large", "small"), current_parts=(True, True, True)
        )
        terminal.observe.side_effect = lambda frame: full if self.count == 1 else None
        terminal.confirm_approach_parts.return_value = replace(
            full,
            point=(1700, 600),
            scores=(0.99, None, None),
            current_parts=(True, False, False),
            provenance="current_motion_approach_only",
        )
        self.path.choose.return_value = PathChoice((1500, 600), "observed_closing_floor")
        measured = lambda *args, **kwargs: CameraMotion((-600, 0) if self.count == 2 else (0, 0), 24, "test")
        result = self.run_interval(seconds=1.5, terminal_vision=terminal, motion=measured)
        self.assertNotEqual("wing_end_observed", result["status"])
        self.assertIn(("press", "f2"), self.backend.events)
        self.assertNotIn(("down", "f2"), self.backend.events)
        self.assertNotIn("terminal", result)
        self.assertTrue(any(row["terminal_approach"] is not None for row in result["continuous_frames"]))
        with self.assertRaises(ValueError):
            validate_terminal_return(result, "north", "north", self.at)

    def test_relocalized_current_pair_requires_settling_and_preserves_reference_age(self):
        terminal = Mock()
        full = TerminalReading(
            "nonboss_chest_platform",
            (1100, 600),
            ((700, 400, 750, 450), (1200, 500, 1250, 550), (1000, 600, 1050, 650)),
            (0.99, 0.99, 0.99),
            ("left", "right", "floor"),
            current_parts=(True, True, True),
        )
        terminal.observe.side_effect = lambda frame: full if self.count == 1 else None
        terminal.confirm_relocalized_parts.return_value = replace(
            full, provenance="current_landmark_approach_only", current_parts=(True, True, False), scores=(0.99, 0.99, None)
        )
        measured = lambda *args, **kwargs: CameraMotion(None if self.count == 2 else (0, 0), 24, "test")
        result = self.run_interval(seconds=3, terminal_vision=terminal, motion=measured)
        self.assertEqual("wing_end_observed", result["status"])
        self.assertEqual("current_relocalized_two_parts", result["terminal"]["provenance"])
        self.assertIsNone(result["terminal"]["scores"][2])
        self.assertGreaterEqual(result["continuous_frames"][-1]["arrival_hold_seconds"], 0.64)
        self.assertEqual(result["continuous_frames"][0]["acquired_at"], result["terminal"]["reference_observed_at"])
        self.assertFalse(self.backend.events)
        validate_terminal_return(result, "north", "north", result["continuous_frames"][-1]["processed_at"] + 0.1)

    def test_terminal_approach_rejects_floor_that_does_not_close_the_distance(self):
        terminal = Mock()
        terminal.observe.return_value = TerminalReading("nonboss_chest_platform", (1850, 600), (), ())
        self.path.choose.return_value = PathChoice((480, 300), "observed_wrong_way_floor")
        result = self.run_interval(seconds=2, terminal_vision=terminal)
        self.assertEqual("terminal_approach_floor_unverified", result["continuous_status"])
        self.assertFalse(self.backend.events)

    def test_static_terminal_tracking_finishes_with_current_parts_and_original_reference_time(self):
        terminal = Mock()
        reading = TerminalReading(
            "nonboss_chest_platform", (1100, 600), (), (0.99, 0.99, 0.99), ("floor", "large", "small"), current_parts=(True, True, True)
        )
        terminal.observe.side_effect = lambda frame: reading if self.count == 1 else None
        terminal.confirm_static_parts.return_value = replace(
            reading, provenance="current_static_two_parts", current_parts=(False, True, True), scores=(None, 0.99, 0.99)
        )
        result = self.run_interval(seconds=3, terminal_vision=terminal)
        self.assertEqual("wing_end_observed", result["status"])
        self.assertEqual("recent_terminal_layout_and_player_settled", result["continuous_status"])
        self.assertEqual(result["continuous_frames"][0]["acquired_at"], result["terminal"]["reference_observed_at"])
        self.assertFalse(self.backend.events)
        last = result["continuous_frames"][-1]
        validate_terminal_return(result, "north", "north", last["processed_at"] + 0.1)
        motion_settled = dict(result, terminal=dict(result["terminal"], provenance="current_motion_settled_two_parts"))
        validate_terminal_return(motion_settled, "north", "north", last["processed_at"] + 0.1)
        for changes in (
            dict(reference_observed_at=-3),
            dict(current_parts=[False, False, True]),
            dict(provenance="historical_three_parts"),
        ):
            for source in (result, motion_settled):
                with self.assertRaises(ValueError):
                    validate_terminal_return(
                        dict(source, terminal=dict(source["terminal"], **changes)), "north", "north", last["processed_at"] + 0.1
                    )

    def test_combat_off_detaches_enemy_observers_and_keeps_f2_down(self):
        self.hazards.observe.return_value = (Enemy((1000, 600)),)
        target = Mock()
        result = self.run_interval(engage_monsters=False, target_vision=target)
        self.hazards.observe.assert_not_called()
        target.observe.assert_not_called()
        self.assertNotIn(("down", "f4"), self.backend.events)
        self.assertEqual(1, list(self.backend.events).count(("up", "f2")))
        self.assertEqual("teleport_aim_settling", result["continuous_frames"][0]["action"])
        self.assertTrue(all(row["action"] == "hold_teleport" for row in result["continuous_frames"][1:-1]))

    def test_local_turn_is_not_reused_as_global_route_heading(self):
        headings = []

        def plan(*args, **kwargs):
            headings.append(kwargs.get("heading"))
            return self.plan(*args, **kwargs)

        self.run_interval(plan=plan, progress=dict(steps=[], position=[0, 0], breadcrumbs=[], steering_heading=(-1, 0.5)))
        self.assertTrue(headings)
        self.assertTrue(all(heading is None for heading in headings))

    def test_closed_forward_floor_turns_before_cast_reference_without_releasing_f2(self):
        def plan(*args, **kwargs):
            choice, ground, view = self.plan(*args, **kwargs)
            if self.count >= 2:
                view.floor[30:45, 175:190] = False
                choice = PathChoice((480, 300), "observed_floor_frontier")
            return choice, ground, view

        result = self.run_interval(plan=plan)
        first, turn = result["continuous_frames"][:2]
        self.assertLess(turn["processed_at"] - first["processed_at"], 0.32)
        self.assertTrue(turn["steering_changed"])
        self.assertEqual("previous_floor_closed", turn["steering_reason"])
        self.assertIn(("move", 480, 300), self.backend.events)
        self.assertEqual(1, list(self.backend.events).count(("up", "f2")))

    def test_stuck_aim_is_excluded_and_alternate_floor_is_reaimed_without_restarting_room(self):
        grab = self.capture.grab.side_effect

        def stable():
            captured = grab()
            captured.frame[:] = 20
            return captured

        self.capture.grab.side_effect = stable
        self.path.choose.return_value = PathChoice((1300, 300), "observed_alternate_floor")
        result = self.run_interval(seconds=4)
        retries = [row for row in result["continuous_frames"] if row.get("recovery")]
        self.assertTrue(retries)
        self.assertEqual("blocked_aim_excluded", retries[0]["recovery"])
        self.assertEqual([[1386, 236, 1514, 364]], [list(box) for box in retries[0]["blocked_aims"]])
        self.assertIn(("move", 1300, 300), self.backend.events)
        self.assertEqual(1, list(self.backend.events).count(("up", "f2")))

    def test_missing_path_pauses_in_same_room_after_transient_retry(self):
        def plan(*args, **kwargs):
            if self.count > 2:
                raise RuntimeError("ground_unverified")
            return self.plan(*args, **kwargs)

        result = self.run_interval(seconds=3, plan=plan)
        self.assertEqual("field_paused", result["status"])
        self.assertEqual("ground_unverified", result["continuous_status"])
        self.assertEqual(("up", "f2"), list(self.backend.events)[-1])

    def test_animated_effects_without_measured_travel_still_trigger_escape(self):
        result = self.run_interval(seconds=4)
        self.assertTrue(any(row.get("recovery") for row in result["continuous_frames"]))

    def test_stationary_frames_without_dispatched_teleport_do_not_poison_route_exits(self):
        self.backend.pointer_matches = lambda point: False
        graph = ArcaneRouteGraph()
        result = self.run_interval(seconds=4, route_memory=graph)
        self.assertEqual(0, result["input_dispatches"]["f2"])
        self.assertEqual("no_valid_steering", result["continuous_status"])
        self.assertFalse(any(row.get("recovery") for row in result["continuous_frames"]))
        self.assertEqual(0, result["route_graph"]["failed_exits"])

    def test_changing_scenes_without_feature_overlap_are_not_stationary_evidence(self):
        result = self.run_interval(seconds=4, motion=lambda *args, **kwargs: CameraMotion(None, 0, "correspondence_insufficient"))
        self.assertFalse(any(row.get("recovery") for row in result["continuous_frames"]))
        self.assertEqual("interval_complete", result["continuous_status"])

    def test_stationary_samples_separated_by_missing_correspondence_do_not_accumulate(self):
        def measured(*args, **kwargs):
            return CameraMotion((0, 0) if self.count % 2 else None, 20, "test")

        result = self.run_interval(seconds=4, motion=measured)
        self.assertFalse(any(row.get("recovery") for row in result["continuous_frames"]))

    def test_transient_motion_gap_reacquires_from_last_verified_frame(self):
        samples = iter(
            [CameraMotion((100, -50), 20, "verified"), CameraMotion(None, 0, "missing"), CameraMotion((120, -60), 20, "verified")]
            + [CameraMotion((0, 0), 20, "verified")] * 20
        )
        result = self.run_interval(seconds=1, motion=lambda *args, **kwargs: next(samples))
        rows = result["continuous_frames"]
        self.assertEqual((100, -50), rows[1]["position"])
        self.assertFalse(rows[1]["position_current"])
        self.assertEqual((220, -110), rows[2]["position"])
        self.assertEqual(0, rows[2]["position_generation"])
        self.assertTrue(rows[2]["position_current"])

    def test_sustained_motion_gap_starts_new_local_history_and_resumes_visit_avoidance(self):
        samples = iter(
            [CameraMotion((100, -50), 20, "verified")]
            + [CameraMotion(None, 0, "missing")] * 9
            + [CameraMotion((120, -60), 20, "verified")] * 20
        )
        result = self.run_interval(seconds=1.3, motion=lambda *args, **kwargs: next(samples))
        rows = result["continuous_frames"]
        restarted = next(i for i, row in enumerate(rows) if row["position_generation"] == 1)
        self.assertEqual((0, 0), rows[restarted]["position"])
        self.assertEqual((120, -60), rows[restarted + 1]["position"])
        self.assertEqual("local_scene", result["position_reference"])
        self.assertTrue(result["position_current"])
        visited = [call[1].get("visited", ()) for call in self.path.choose.call_args_list]
        self.assertTrue(any((120, -60) in points for points in visited))
        self.assertFalse(any((100, -50) in points and (120, -60) in points for points in visited))

    def test_focus_loss_releases_held_input_on_exception(self):
        with self.assertRaisesRegex(RuntimeError, "unfocused"):
            self.run_interval(focused=lambda: self.count < 3)
        self.assertEqual(("up", "f2"), list(self.backend.events)[-1])

    def test_late_observation_never_starts_f2(self):
        def plan(*args, **kwargs):
            self.at += 0.3
            return self.plan(*args, **kwargs)

        result = self.run_interval(plan=plan)
        self.assertEqual("field_paused", result["status"])
        self.assertFalse(self.backend.events)

    def test_original_buff_deadline_stops_repeat_and_reports_renewal_due(self):
        result = self.run_interval(seconds=3, buff_expires_at=0.5)
        self.assertEqual("field_paused", result["status"])
        self.assertEqual("buff_renewal_due", result["continuous_status"])
        self.assertEqual(("up", "f2"), list(self.backend.events)[-1])

    def test_travel_permit_allows_observed_enemy_but_does_not_change_buff_safety(self):
        validate_travel_permit(self.proof, self.profile.survival, time.time())
        self.assertFalse(self.proof["monsters_clear"])
        with self.assertRaises(ValueError):
            validate_travel_permit(dict(self.proof, monsters_clear=None), self.profile.survival, time.time())

    def test_pointer_delay_cannot_start_expired_held_input(self):
        backend = DryFieldInput()
        backend.move = lambda *args: setattr(self, "at", 2)
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: self.at)
        held.apply(FieldDirective("travel", "fresh", aim=(100, 100), deadline=1), 0)
        self.assertFalse(held.held)
        self.assertFalse(backend.events)

    def test_analysis_near_lease_deadline_reuses_verified_settled_aim_on_the_next_fresh_frame(self):
        backend, at = DryFieldInput(), [0.21]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0], aim_settle_seconds=0.05)
        held.apply(FieldDirective("travel", "fresh", aim=(1200, 300), deadline=0.25), at[0])
        self.assertEqual("f2", held.snapshot()["pending"])
        self.assertEqual(0, held.snapshot()["dispatches"]["f2"])
        at[0] = 0.26
        held.service(at[0])
        self.assertNotIn(("down", "f2"), backend.events)
        at[0] = 0.43
        held.apply(FieldDirective("travel", "new_fresh_frame", aim=(1200, 300), deadline=0.47), at[0])
        self.assertEqual(1, held.snapshot()["dispatches"]["f2"])
        self.assertEqual([("move", 1200, 300), ("down", "f2")], list(backend.events))
        at[0] = 0.48
        held.service(at[0])
        self.assertEqual(("up", "f2"), backend.events[-1])
        self.assertFalse(held.held)

    def test_expired_aim_is_rechecked_and_explicit_release_forgets_it(self):
        backend, at = DryFieldInput(), [0.21]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0], aim_settle_seconds=0.05)
        held.apply(FieldDirective("travel", "fresh", aim=(1200, 300), deadline=0.25), at[0])
        at[0] = 0.26
        held.service(at[0])
        backend.pointer = (800, 600)
        at[0] = 0.43
        held.apply(FieldDirective("travel", "new_frame", aim=(1200, 300), deadline=0.47), at[0])
        self.assertEqual(2, list(backend.events).count(("move", 1200, 300)))
        self.assertEqual(0, held.snapshot()["dispatches"]["f2"])
        held.release()
        self.assertIsNone(held.pointer)

    def test_pointer_interference_during_settle_cannot_cast_at_an_unapproved_point(self):
        backend, at = DryFieldInput(), [0.0]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0], aim_settle_seconds=0.05)
        held.apply(FieldDirective("travel", "fresh", aim=(1200, 300), deadline=0.25), at[0])
        backend.pointer = (800, 600)
        at[0] = 0.06
        held.service(at[0])
        self.assertEqual(0, held.snapshot()["dispatches"]["f2"])
        self.assertFalse(held.held)

    def test_recording_wrapper_preserves_actual_pointer_validation_without_recording_a_cast(self):
        driver = DryFieldInput()
        backend = RecordingInput(driver)
        backend.move(1200, 300)
        self.assertTrue(backend.pointer_matches((1200, 300)))
        driver.pointer = (800, 600)
        self.assertFalse(backend.pointer_matches((1200, 300)))
        self.assertEqual(["move"], [row["action"] for row in backend.events])

    def test_pointer_validation_delay_cannot_dispatch_after_screen_expiry(self):
        backend, at = DryFieldInput(), [0.0]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0], aim_settle_seconds=0.05)
        held.apply(FieldDirective("travel", "fresh", aim=(1200, 300), deadline=0.25), at[0])

        def matches(point):
            at[0] = 0.26
            return True

        backend.pointer_matches = matches
        at[0] = 0.24
        held.service(at[0])
        self.assertFalse(held.held)
        self.assertEqual(0, held.snapshot()["dispatches"]["f2"])

    def test_held_repeat_has_no_key_up_between_cast_intervals_and_expiry_stops_it(self):
        backend = DryFieldInput()
        at = [0.0]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0], repeat_seconds=0.32)
        for current in (0, 0.1, 0.32):
            at[0] = current
            held.apply(FieldDirective("travel", "current_floor", deadline=current + 0.25), current)
        self.assertEqual([("down", "f2"), ("down", "f2")], list(backend.events))
        at[0] = 0.7
        held.apply(FieldDirective("travel", "expired", deadline=0.69), at[0])
        self.assertEqual(("up", "f2"), list(backend.events)[-1])
        self.assertFalse(held.held)

    def test_pointer_turn_defers_repeat_until_settled_without_releasing_held_f2(self):
        backend, at = DryFieldInput(), [0.0]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0], repeat_seconds=0.32, aim_settle_seconds=0.05)
        for current, aim in ((0, (1200, 300)), (0.06, None), (0.4, (500, 300))):
            at[0] = current
            held.apply(FieldDirective("travel", "current_floor", aim=aim, deadline=current + 0.25), current)
        self.assertEqual(1, list(backend.events).count(("down", "f2")))
        self.assertIn("f2", held.held)
        at[0] = 0.46
        held.apply(FieldDirective("travel", "fresh_settled_aim", deadline=0.7), at[0])
        self.assertEqual(2, list(backend.events).count(("down", "f2")))
        self.assertNotIn(("up", "f2"), backend.events)

    def test_outbound_requires_player_at_observed_central_waypoint(self):
        hub = Mock()
        hub.observe.return_value = None
        self.planner[0].observe.return_value = GroundReading((960, 580), "current")
        with self.assertRaisesRegex(RuntimeError, "central_waypoint"):
            self.run_interval(hub_vision=hub)
        self.assertFalse(self.backend.events)

    def test_fresh_supervised_outbound_wing_can_resume_after_central_waypoint(self):
        hub = Mock()
        hub.observe.return_value = None
        self.proof.update(current_wing="north", wing_phase="outbound")
        result = self.run_interval(hub_vision=hub)
        self.assertEqual("north_segment_observed", result["status"])
        self.assertIn(("down", "f2"), self.backend.events)
        self.proof.update(current_wing="south")
        with self.assertRaisesRegex(RuntimeError, "central_waypoint"):
            self.run_interval(hub_vision=hub)

    def test_rapid_reaims_do_not_starve_repeats_between_analysis_frames(self):
        backend, at = DryFieldInput(), [0.0]
        held = HeldFieldInput(backend, "f2", "f4", clock=lambda: at[0], repeat_seconds=0.32, aim_settle_seconds=0.05)
        for current in (0, 0.1, 0.2, 0.3, 0.4):
            at[0] = current
            aim = (1200 - int(current * 200), 300)
            held.apply(FieldDirective("travel", "current_floor", aim=aim, deadline=current + 0.25), current)
            at[0] = current + 0.06
            held.service(at[0])
        self.assertEqual(2, list(backend.events).count(("down", "f2")))
        self.assertNotIn(("up", "f2"), backend.events)
        self.assertEqual((1120, 300), held.cast_pointer)
        at[0] = 0.8
        held.service(at[0])
        held.service(1.5)
        self.assertEqual(("up", "f2"), list(backend.events)[-1])
        self.assertEqual(2, list(backend.events).count(("down", "f2")))

    def test_return_from_south_uses_opposite_heading_and_stops_at_settled_hub(self):
        hub = Mock()
        hub.observe.return_value = (960, 580)
        wings = []

        def plan(frame, wing, *args, **kwargs):
            wings.append(wing)
            return self.plan(frame, wing, *args, **kwargs)

        result = self.run_interval(return_from="south", hub_vision=hub, plan=plan)
        self.assertTrue(wings and all(wing == "north" for wing in wings))
        self.assertGreaterEqual(result["continuous_frames"][-1]["arrival_hold_seconds"], 0.64)
        self.assertEqual("hub_arrived", result["status"])
        self.assertFalse(self.backend.events)

    def test_visible_distant_hub_does_not_claim_arrival(self):
        hub = Mock()
        hub.observe.return_value = (1400, 300)
        self.path.choose.return_value = PathChoice((1400, 300), "observed_hub_approach")
        result = self.run_interval(return_from="south", hub_vision=hub)
        self.assertNotEqual("hub_arrived", result["status"])
        self.assertIn(("press", "f2"), self.backend.events)
        self.assertNotIn(("down", "f2"), self.backend.events)

    def test_hub_approach_allows_grid_center_rounding_but_keeps_actual_arrival_radius(self):
        hub = Mock()
        hub.observe.side_effect = lambda frame: (959, 475) if self.count < 12 else (960, 580)
        self.path.choose.return_value = PathChoice((1044, 531), "current_grid_floor")
        result = self.run_interval(seconds=2.5, return_from="north", hub_vision=hub)
        self.assertEqual("hub_arrived", result["status"])
        self.assertEqual(1, list(self.backend.events).count(("press", "f2")))
        self.assertTrue(all(row["index"] >= 11 for row in result["continuous_frames"] if row["action"] == "central_arrival_hold"))
        self.setUp()
        hub.observe.side_effect = None
        hub.observe.return_value = (959, 475)
        self.path.choose.return_value = PathChoice((1048, 531), "outside_grid_rounding")
        result = self.run_interval(seconds=2, return_from="north", hub_vision=hub)
        self.assertEqual("central_approach_floor_unverified", result["continuous_status"])
        self.assertFalse(self.backend.events)

    def test_pending_cast_leaves_hub_after_two_quiet_frames_without_claiming_arrival(self):
        hub = Mock()
        hub.observe.side_effect = lambda frame: (960, 580) if self.count <= 3 else (1400, 300)
        self.path.choose.return_value = PathChoice((1400, 300), "observed_hub_approach")
        motion = lambda *args, **kwargs: CameraMotion((440, -280) if self.count == 4 else (0, 0), 24, "test")
        result = self.run_interval(seconds=2, return_from="north", hub_vision=hub, motion=motion)
        self.assertNotEqual("hub_arrived", result["status"])
        self.assertEqual(3, sum(row["action"] == "central_arrival_hold" for row in result["continuous_frames"]))
        self.assertIn(("press", "f2"), self.backend.events)

    def test_detected_hub_preempts_held_aim_then_approaches_once_after_settling(self):
        hub = Mock()
        hub.observe.side_effect = lambda frame: None if self.count < 3 else ((1200, 600) if self.count < 12 else (960, 580))
        self.path.choose.return_value = PathChoice((1200, 600), "observed_hub_approach")

        def south_plan(*args, **kwargs):
            _, ground, view = self.plan(*args, **kwargs)
            return PathChoice((450, 850), "observed_south_floor"), ground, view

        result = self.run_interval(seconds=3, return_from="north", hub_vision=hub, plan=south_plan)
        self.assertEqual("hub_arrived", result["status"])
        actions = [row["action"] for row in result["continuous_frames"]]
        self.assertEqual(1, actions.count("central_approach_cast"))
        events = list(self.backend.events)
        released = events.index(("up", "f2"))
        self.assertNotIn(("down", "f2"), events[released + 1 :])
        self.assertIn(("move", 1200, 600), events[released + 1 :])
        self.assertEqual(1, events.count(("press", "f2")))
        first_seen = next(row["processed_at"] for row in result["continuous_frames"] if row["hub_point"] is not None)
        cast = next(row["processed_at"] for row in result["continuous_frames"] if row["action"] == "central_approach_cast")
        self.assertGreaterEqual(cast - first_seen, 0.64)

    def test_temporarily_lost_hub_never_resumes_opposite_wing_search(self):
        hub = Mock()
        hub.observe.side_effect = lambda frame: (1200, 600) if self.count == 3 else None

        def south_plan(*args, **kwargs):
            _, ground, view = self.plan(*args, **kwargs)
            return PathChoice((450, 850), "observed_south_floor"), ground, view

        result = self.run_interval(seconds=3, return_from="north", hub_vision=hub, plan=south_plan)
        self.assertEqual("central_reacquisition_required", result["continuous_status"])
        self.assertTrue(result["hub_acquired"])
        rows = result["continuous_frames"]
        self.assertTrue(all(row["action"] == "central_reacquire_hold" for row in rows[3:]))
        events = list(self.backend.events)
        released = events.index(("up", "f2"))
        self.assertNotIn(("down", "f2"), events[released + 1 :])
        self.assertNotIn(("press", "f2"), events)

    def test_obvious_opposite_wing_aim_is_rejected_before_f2(self):
        self.path.choose.return_value = PathChoice(None, "no_side_exit")

        def plan(*args, **kwargs):
            _, ground, view = self.plan(*args, **kwargs)
            return PathChoice((450, 850), "observed_floor"), ground, view

        result = self.run_interval(plan=plan)
        self.assertEqual("opposite_wing_aim_rejected", result["continuous_status"])
        self.assertFalse(self.backend.events)
        self.assertIsNone(result["route_detour"])


if __name__ == "__main__":
    unittest.main()
