import threading
import time
import unittest
from dataclasses import dataclass
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np

from diablo2.actions.loot_pickup import LootPickupSession, LootScanHit, MatchResult
from diablo2.common.conditional_loot import ConditionalLootController
from diablo2.common.config import load_config
from diablo2.common.field_control import DryFieldInput, HeldFieldInput
from diablo2.common.scene_motion import CameraMotion
from diablo2.runs.summoner.continuous_navigation import run_continuous_route, validate_travel_permit
from diablo2.runs.summoner.encounter_execution import SummonerAppearance, run_platform_encounter
from diablo2.runs.summoner.navigation import PathChoice, PathView
from diablo2.runs.summoner.scene_vision import GroundReading


class FakeStop:
    def __init__(self, now):
        self.now, self.stopped = now, False

    def is_set(self):
        return self.stopped

    def set(self):
        self.stopped = True

    def wait(self, seconds):
        self.now[0] += seconds
        return self.stopped


class LootBackend(DryFieldInput):
    def click(self):
        self.events.append(("click",))


class ConditionalLootTests(unittest.TestCase):
    def test_low_resources_preempt_scan_and_off_restoration_with_one_recovery_or_immediate_exit(self):
        for phase in ("scan", "restore"):
            for recovered in (True, False):
                with self.subTest(phase=phase, recovered=recovered):
                    self.setUp()
                    calls, restored = [], [False]
                    valid = self.hud.observe.return_value
                    low = SimpleNamespace(life=SimpleNamespace(ratio=0.2), mana=SimpleNamespace(ratio=None), belt_visibility="closed")

                    def recover(frame, reading, acquired):
                        calls.append((reading, acquired))
                        self.assertIs(low, reading)
                        self.assertLess(self.now[0] - acquired, 0.25)
                        restored[0] = recovered
                        return recovered

                    controller = self.controller(recover_resources=recover)

                    def observe(frame):
                        if controller.labels_state == "on" and not restored[0]:
                            if phase == "scan" or self.scanner.scan_frame.call_count >= 2:
                                return low
                        return valid

                    self.hud.observe.side_effect = observe
                    if recovered:
                        result = controller.after_combat()
                        self.assertEqual("resource_recovered" if phase == "scan" else "inspection_complete", result["status"])
                        self.assertTrue(result["labels_off_sent"])
                    else:
                        with self.assertRaisesRegex(RuntimeError, "^resource_low$"):
                            controller.after_combat()
                        self.assertTrue(controller.resource_exit_required)
                    self.assertEqual(1, len(calls))
                    self.assertNotIn(("press", "f2"), self.backend.events)
                    self.assertNotIn(("click",), self.backend.events)

    def test_hud_reobservation_rejects_fresh_positive_at_exact_read_deadline(self):
        for elapsed, permitted in ((0.749, True), (0.75, False)):
            with self.subTest(elapsed=elapsed):
                self.setUp()
                controller = self.controller()
                started, calls = self.now[0], [0]
                valid = self.hud.observe.return_value
                unknown = SimpleNamespace(life=SimpleNamespace(ratio=None), mana=valid.mana, belt_visibility="closed")

                def grab():
                    calls[0] += 1
                    self.now[0] = started + (0.01 if calls[0] == 1 else elapsed)
                    return SimpleNamespace(frame=self.frame)

                self.capture.grab.side_effect = grab
                self.hud.observe.side_effect = [unknown, valid]
                self.stop.wait = lambda seconds: self.now.__setitem__(0, started + elapsed - 0.01)
                if permitted:
                    frame, acquired = controller._read()
                    self.assertIs(frame, self.frame)
                    self.assertLess(self.now[0] - acquired, 0.25)
                else:
                    with self.assertRaisesRegex(RuntimeError, "observation_unverified"):
                        controller._read()
                self.assertFalse(self.backend.events)

    def test_transient_hud_loss_waits_without_input_before_scan_or_off_restoration(self):
        for phase in ("scan", "restore"):
            with self.subTest(phase=phase):
                self.setUp()
                controller = self.controller()
                valid = self.hud.observe.return_value
                unknown = SimpleNamespace(life=SimpleNamespace(ratio=None), mana=valid.mana, belt_visibility="closed")
                missing, observed_inputs = [0], []

                def observe(frame):
                    toggled = list(self.backend.events).count(("press", "alt")) == 1
                    due = self.scanner.scan_frame.call_count >= 2 if phase == "restore" else True
                    if toggled and due and missing[0] < 2:
                        missing[0] += 1
                        observed_inputs.append(tuple(self.backend.events))
                        return unknown
                    return valid

                self.hud.observe.side_effect = observe
                result = controller.after_combat()
                self.assertEqual("inspection_complete", result["status"])
                self.assertTrue(result["movement_allowed"])
                self.assertEqual(2, missing[0])
                self.assertEqual(observed_inputs[0], observed_inputs[1])
                self.assertIsNotNone(controller.last_hud_failure[1]["recovered_at"])
                self.assertEqual(phase == "restore", controller.last_hud_failure[1]["restoring"])
                self.assertNotIn(("press", "f2"), self.backend.events)
                self.assertFalse(result["clicks"])

    def test_hud_reobservation_preserves_failure_deadlines_stop_focus_and_resource_guards(self):
        for condition in ("persistent", "low", "stale", "focus", "stop", "buff", "scan"):
            with self.subTest(condition=condition):
                self.setUp()
                controller = self.controller(max_scan_seconds=0.1 if condition == "scan" else 3)
                valid, reads = self.hud.observe.return_value, [0]
                unknown = SimpleNamespace(life=SimpleNamespace(ratio=None), mana=valid.mana, belt_visibility="closed")

                def observe(frame):
                    if list(self.backend.events).count(("press", "alt")) != 1:
                        return valid
                    reads[0] += 1
                    if condition == "stale":
                        self.now[0] += 0.25
                    if condition == "persistent" or reads[0] == 1:
                        return unknown
                    if condition == "low":
                        return SimpleNamespace(life=SimpleNamespace(ratio=0.1), mana=valid.mana, belt_visibility="closed")
                    if condition == "focus":
                        self.focus = False
                    if condition == "stop":
                        self.stop.set()
                    if condition == "buff":
                        controller.deadline_supplier = lambda: self.now[0]
                    return valid

                self.hud.observe.side_effect = observe
                result = controller.after_combat()
                expected = dict(
                    persistent="observation_unverified",
                    low="resource_low",
                    stale="screen_stale",
                    focus="focus_lost",
                    stop="stopped",
                    buff="buff_renewal_due",
                    scan="scan_deadline",
                )
                self.assertEqual(expected[condition], result["status"])
                self.assertNotIn(("press", "f2"), self.backend.events)
                self.assertFalse(result["clicks"])
                if condition == "persistent":
                    self.assertLess(self.now[0], 1.7)
                    self.assertFalse(result["movement_allowed"])

    def test_native_parallel_scan_preserves_configured_priority_and_exact_coordinates(self):
        scanner = LootPickupSession(load_config(Path("config")))
        candidates = scanner._build_candidates()
        frame = np.zeros((1140, 1922, 3), np.uint8)
        key, rune = candidates[:2]
        for candidate, (x, y) in ((key, (700, 400)), (rune, (1000, 600))):
            height, width = candidate.template.shape[:2]
            frame[y : y + height, x : x + width] = candidate.template
        hit = scanner.scan_frame(frame, candidates)
        self.assertEqual("key of hate", hit.label)
        self.assertEqual((700, 400), hit.match.top_left)
        self.assertGreaterEqual(hit.match.score, key.threshold)
        self.assertEqual("rune", scanner.scan_frame(frame, [rune]).label)
        self.assertIsNone(scanner.scan_frame(np.zeros_like(frame), candidates))

    def setUp(self):
        self.now, self.focus = [0.0], True
        self.stop, self.backend = FakeStop(self.now), LootBackend()
        self.frame = np.zeros((1140, 1922, 3), np.uint8)
        self.capture = Mock(target=dict(left=-1922, top=0))

        def grab():
            self.now[0] += 0.01
            return SimpleNamespace(frame=self.frame)

        self.capture.grab.side_effect = grab
        self.hud = Mock()
        self.hud.observe.return_value = SimpleNamespace(
            life=SimpleNamespace(ratio=1), mana=SimpleNamespace(ratio=1), belt_visibility="closed"
        )
        self.profile = SimpleNamespace(
            actions=SimpleNamespace(movement_skill_key="f2"),
            survival=SimpleNamespace(
                life_potion_below=0.5,
                mana_potion_below=0.1,
                potion_pickup=SimpleNamespace(
                    preferred_kind="full_rejuvenation_potion", fallback_kind="rejuvenation_potion", fallback_at_or_below=6
                ),
            ),
        )
        self.config = SimpleNamespace(shared_loot=SimpleNamespace(ignore_labels=[]))
        self.scanner = Mock()
        self.scanner._build_candidates.return_value = [SimpleNamespace(label="key of hate")]
        self.scanner.scan_frame.return_value = None

    def controller(self, **kwargs):
        controller = ConditionalLootController(
            self.config,
            self.capture,
            self.hud,
            self.backend,
            lambda: self.focus,
            self.profile,
            self.stop,
            lambda frame: False,
            clock=lambda: self.now[0],
            scanner=self.scanner,
            ground_supplier=lambda: SimpleNamespace(observe=lambda frame, captured_at: SimpleNamespace(point=(750, 450))),
            **kwargs,
        )
        controller.prepare(fresh_room=True)
        return controller

    def hit(self, label="key of hate", x=700):
        return LootScanHit(label, MatchResult((x, 400), 100, 20, 0.99))

    def test_fresh_room_and_repeated_preparation_keep_labels_off_without_alt(self):
        controller = self.controller()
        prepared = controller.prepare(fresh_room=True, labels_on=False)
        self.assertEqual("off", prepared["labels_state"])
        self.assertTrue(controller.movement_allowed)
        self.assertFalse(self.backend.events)
        self.scanner._build_candidates.assert_not_called()

    def test_unknown_resume_state_requires_evidence_without_input(self):
        controller = self.controller()
        controller._prepared = False
        controller.labels_state = "unknown"
        with self.assertRaisesRegex(RuntimeError, "item_labels_state_unknown"):
            controller.prepare(fresh_room=False)
        self.assertFalse(controller.movement_allowed)
        self.assertFalse(self.backend.events)

    def test_no_candidate_scan_toggles_on_then_off_without_claiming_no_drops_or_kill(self):
        controller = self.controller()
        result = controller.after_combat(combat_id="one")
        self.assertEqual("inspection_complete", result["status"])
        self.assertEqual("combat_ended_scan", result["reason"])
        self.assertEqual(2, result["scan_frames"])
        self.assertEqual([("press", "alt"), ("press", "alt")], list(self.backend.events))
        self.assertFalse(result["kill_confirmed"])
        self.assertEqual(0, result["pickups_confirmed"])
        self.assertNotIn("no_drops", result)
        controller.after_combat(combat_id="one")
        self.assertEqual(2, len(self.backend.events))

    def test_fresh_aim_click_and_disappeared_label_stay_unconfirmed(self):
        controller = self.controller()
        self.scanner.scan_frame.side_effect = [self.hit(), self.hit(), self.hit(x=850), self.hit(x=850), None]
        result = controller.after_combat()
        self.assertEqual("click_unconfirmed", result["status"])
        self.assertEqual(1, len(result["clicks"]))
        self.assertFalse(result["clicks"][0]["label_visible_after"])
        self.assertFalse(result["clicks"][0]["pickup_confirmed"])
        self.assertEqual(0, result["pickups_confirmed"])
        self.assertEqual(
            [
                ("press", "alt"),
                ("move", -1172, 410),
                ("press", "alt"),
                ("press", "f2"),
                ("press", "alt"),
                ("move", -1022, 410),
                ("click",),
                ("press", "alt"),
            ],
            list(self.backend.events),
        )
        self.assertEqual(1, len(result["teleports"]))
        self.assertTrue(result["teleports"][0]["dispatch_returned"])
        self.assertTrue(result["movement_allowed"])
        self.assertGreaterEqual(self.capture.grab.call_count, 7)

    def test_scanner_error_restores_off_once(self):
        controller = self.controller()
        self.scanner.scan_frame.side_effect = ValueError("broken scanner")
        result = controller.after_combat()
        self.assertEqual("scan_error", result["status"])
        self.assertEqual("broken scanner", result["error"])
        self.assertEqual([("press", "alt"), ("press", "alt")], list(self.backend.events))
        self.assertTrue(controller.movement_allowed)

    def test_label_reconfirmation_is_reused_for_off_without_extending_teleport_frame_age(self):
        for capture_seconds, scan_seconds, off_seconds, permitted in (
            (0.12, 0.05, 0, True),
            (0.15, 0.06, 0, True),
            (0.15, 0.06, 0.08, False),
        ):
            with self.subTest(permitted=permitted):
                self.setUp()
                controller = self.controller()
                acquired, reconfirmed, request_ages = [None], [None], []
                scans = [0]
                press = self.backend.press

                def grab():
                    acquired[0] = self.now[0]
                    self.now[0] += capture_seconds
                    return SimpleNamespace(frame=self.frame)

                def scan(*args):
                    scans[0] += 1
                    self.now[0] += scan_seconds
                    if scans[0] == 2:
                        reconfirmed[0] = acquired[0]
                    return self.hit(x=700 if scans[0] <= 2 else 850) if scans[0] <= 4 else None

                def record(key):
                    if key == "alt" and list(self.backend.events).count(("press", "alt")) == 1:
                        self.now[0] += off_seconds
                    if key == "f2":
                        request_ages.append(self.now[0] - reconfirmed[0])
                    press(key)

                self.capture.grab.side_effect = grab
                self.scanner.scan_frame.side_effect = scan
                self.backend.press = record
                result = controller.after_combat()
                self.assertEqual(int(permitted), list(self.backend.events).count(("press", "f2")))
                self.assertEqual(int(permitted), list(self.backend.events).count(("click",)))
                self.assertEqual("click_unconfirmed" if permitted else "screen_stale", result["status"])
                self.assertEqual("off", result["labels_state"])
                if permitted:
                    self.assertAlmostEqual(capture_seconds + scan_seconds, request_ages[0])
                else:
                    self.assertIsNone(result["teleports"][0]["requested_at"])

    def test_remaining_item_pointer_settle_preserves_stop_buff_and_actual_fifty_ms(self):
        for condition in ("normal", "stop", "buff_due"):
            with self.subTest(condition=condition):
                self.setUp()
                controller = self.controller()
                self.scanner.scan_frame.side_effect = [self.hit(), self.hit(), self.hit(x=850), self.hit(x=850), None]
                move, press, wait = self.backend.move, self.backend.press, self.stop.wait
                prepared_at, request_at = [None], []

                def prepare(x, y):
                    move(x, y)
                    if prepared_at[0] is None:
                        prepared_at[0] = self.now[0]

                def record(key):
                    if key == "f2":
                        request_at.append(self.now[0])
                    press(key)

                def settling(seconds):
                    if prepared_at[0] is not None and not request_at:
                        if condition == "stop":
                            self.stop.set()
                        elif condition == "buff_due":
                            controller.deadline_supplier = lambda: self.now[0]
                    return wait(seconds)

                self.backend.move, self.backend.press, self.stop.wait = prepare, record, settling
                result = controller.after_combat()
                if condition == "normal":
                    self.assertAlmostEqual(0.05, request_at[0] - prepared_at[0])
                    self.assertEqual(1, len(request_at))
                    self.assertEqual("click_unconfirmed", result["status"])
                else:
                    self.assertFalse(request_at)
                    self.assertFalse(result["clicks"])
                    self.assertEqual("stopped" if condition == "stop" else "buff_renewal_due", result["status"])

    def test_focus_loss_during_reused_off_toggle_never_sends_teleport_or_repeats_toggle(self):
        controller = self.controller()
        self.scanner.scan_frame.return_value = self.hit()
        press = self.backend.press
        toggles = [0]

        def lose_focus(key):
            press(key)
            if key == "alt":
                toggles[0] += 1
                if toggles[0] == 2:
                    self.focus = False

        self.backend.press = lose_focus
        result = controller.after_combat()
        self.assertEqual("focus_lost", result["status"])
        self.assertEqual("unknown", result["labels_state"])
        self.assertNotIn(("press", "f2"), self.backend.events)
        self.assertEqual(2, toggles[0])

    def test_teleport_never_clicks_when_new_label_or_landing_is_unverified(self):
        for condition in ("label", "ground_observer", "player", "far"):
            with self.subTest(condition=condition):
                self.setUp()
                controller = self.controller()
                current = None if condition == "label" else self.hit(x=1300 if condition == "far" else 700)
                if condition == "ground_observer":
                    controller.ground_supplier = lambda: None
                elif condition == "player":
                    controller.ground_supplier = lambda: SimpleNamespace(observe=lambda *args, **kwargs: GroundReading(None, "pose_lost"))
                self.scanner.scan_frame.side_effect = [self.hit(), self.hit(), current]
                result = controller.after_combat()
                self.assertEqual(1, list(self.backend.events).count(("press", "f2")))
                self.assertNotIn(("click",), self.backend.events)
                self.assertFalse(result["clicks"])
                self.assertEqual("off", result["labels_state"])
                frame, observation = controller.last_landing_observation
                self.assertIs(self.frame, frame)
                self.assertIs(observation, result["teleports"][0]["landing_observation"])
                self.assertEqual([] if condition == "far" else [condition], observation["missing"])
                self.assertEqual("not_near_item" if condition == "far" else "unverified", observation["status"])
                self.assertEqual("pose_lost" if condition == "player" else None, observation["ground_reason"])
                self.assertGreaterEqual(observation["acquired_at"], result["teleports"][0]["requested_at"] + 0.32)

    def test_buff_expiring_during_teleport_preempts_click(self):
        controller = self.controller(deadline_supplier=lambda: 0.3)
        self.scanner.scan_frame.return_value = self.hit()
        result = controller.after_combat()
        self.assertEqual("buff_renewal_due", result["status"])
        self.assertEqual(1, list(self.backend.events).count(("press", "f2")))
        self.assertNotIn(("click",), self.backend.events)
        self.assertEqual("off", result["labels_state"])

    def test_configured_key_template_uses_read_only_scanner_and_same_backend(self):
        self.config = load_config(Path(__file__).absolute().parents[1] / "config")
        self.profile = self.config.characters["flash"]
        self.scanner = LootPickupSession(self.config)
        key = next(candidate for candidate in self.scanner._build_candidates() if candidate.label == "key of hate")
        height, width = key.template.shape[:2]
        self.frame[400 : 400 + height, 700 : 700 + width] = key.template
        controller = self.controller()
        result = controller.after_combat()
        self.assertEqual("key of hate", result["clicks"][0]["label"])
        self.assertEqual("click_unconfirmed", result["status"])
        self.assertFalse(self.scanner.is_running)
        self.assertEqual(1, list(self.backend.events).count(("click",)))

    def test_focus_loss_or_stop_does_not_restore_or_repeat_alt_and_blocks_movement(self):
        for interruption in ("focus", "stop"):
            with self.subTest(interruption=interruption):
                self.setUp()
                controller = self.controller()

                def interrupt(*args):
                    if interruption == "focus":
                        self.focus = False
                    else:
                        self.stop.set()
                    return None

                self.scanner.scan_frame.side_effect = interrupt
                result = controller.after_combat()
                self.assertEqual("unknown", result["labels_state"])
                self.assertFalse(controller.movement_allowed)
                self.assertEqual([("press", "alt")], list(self.backend.events))
                controller.after_combat()
                self.assertEqual([("press", "alt")], list(self.backend.events))

    def test_buff_deadline_preempts_scan_and_safe_cleanup_restores_off(self):
        controller = self.controller(deadline_supplier=lambda: 0.12)

        def expire(*args):
            self.now[0] = 0.13
            return self.hit()

        self.scanner.scan_frame.side_effect = expire
        result = controller.after_combat()
        self.assertEqual("buff_renewal_due", result["status"])
        self.assertEqual([("press", "alt"), ("press", "alt")], list(self.backend.events))
        self.assertTrue(controller.movement_allowed)
        self.assertFalse(result["clicks"])

    def test_frame_and_click_limits_are_bounded_and_same_label_is_not_reclicked(self):
        controller = self.controller(max_clicks=1)
        self.scanner._build_candidates.return_value = [SimpleNamespace(label="key of hate"), SimpleNamespace(label="rune")]

        def scan(frame, candidates):
            return self.hit(candidates[0].label) if candidates else None

        self.scanner.scan_frame.side_effect = scan
        result = controller.after_combat()
        self.assertEqual("click_limit", result["bounded_reason"])
        self.assertEqual(1, list(self.backend.events).count(("click",)))
        self.assertEqual("click_unconfirmed", result["status"])
        self.setUp()
        controller = self.controller(max_scan_frames=1)
        self.scanner.scan_frame.side_effect = [self.hit(), self.hit(x=950)]
        result = controller.after_combat()
        self.assertEqual(1, result["scan_frames"])
        self.assertEqual("frame_limit", result["bounded_reason"])
        self.assertFalse(result["clicks"])

    def test_stale_frame_or_unverified_aim_never_clicks(self):
        for condition in ("stale", "pointer", "target_moved"):
            with self.subTest(condition=condition):
                self.setUp()
                controller = self.controller(max_scan_frames=1)
                if condition == "stale":

                    def scan(*args):
                        self.now[0] += 0.3
                        return self.hit()

                    self.scanner.scan_frame.side_effect = scan
                elif condition == "pointer":
                    self.scanner.scan_frame.return_value = self.hit()
                    self.backend.pointer_matches = lambda aim: False
                else:
                    self.scanner.scan_frame.side_effect = [self.hit(), self.hit(x=950)]
                result = controller.after_combat()
                self.assertFalse(result["clicks"])
                self.assertNotIn(("click",), self.backend.events)
                self.assertTrue(controller.movement_allowed)

    def test_gold_is_ignored_and_small_potion_requires_current_confirmed_count_at_most_six(self):
        for label, count, age, confirmed, permitted in (
            ("gold", 6, 0, True, False),
            ("rejuvenation potion", None, 0, True, False),
            ("rejuvenation potion", 7, 0, True, False),
            ("rejuvenation potion", 6, 1, True, False),
            ("rejuvenation potion", 6, 0, False, False),
            ("rejuvenation potion", 6, 0, True, True),
            ("full rejuvenation potion", None, 0, False, True),
        ):
            with self.subTest(label=label, count=count, age=age, confirmed=confirmed):
                self.setUp()
                self.scanner._build_candidates.return_value = [SimpleNamespace(label=label)]
                controller = self.controller(
                    potion_count_supplier=lambda: dict(count=count, observed_at=self.now[0] - age, confirmed=confirmed)
                )
                self.scanner.scan_frame.return_value = self.hit(label)
                result = controller.after_combat()
                self.assertEqual(permitted, bool(result["clicks"]))

    def test_failed_toggle_or_blocked_restoration_never_assumes_labels_off(self):
        controller = self.controller()

        def press(key):
            self.backend.events.append(("press", key))
            if len(self.backend.events) == 2:
                raise RuntimeError("dispatch failed")

        self.backend.press = press
        result = controller.after_combat()
        self.assertEqual("unknown", result["labels_state"])
        self.assertFalse(controller.movement_allowed)
        self.assertIn("dispatch failed", result["restore_error"])
        self.setUp()
        controller = self.controller()

        def low_resources(*args):
            self.hud.observe.return_value.life.ratio = 0.1
            return None

        self.scanner.scan_frame.side_effect = low_resources
        result = controller.after_combat()
        self.assertEqual("resource_low", result["restore_error"])
        self.assertEqual("on", result["labels_state"])
        self.assertFalse(controller.movement_allowed)
        self.assertEqual([("press", "alt")], list(self.backend.events))


@dataclass
class Enemy:
    ground: tuple


class ConditionalLootIntegrationTests(unittest.TestCase):
    def test_runtime_travel_permit_uses_fresh_automatic_observation_without_supervised_claim(self):
        proof = dict(
            source="runtime_screen",
            area="arcane_sanctuary",
            panels_closed=True,
            belt_closed=True,
            nearby_candidates=0,
            safety_scope="current_nearby_candidates_and_stable_ratios",
            observed_at=10.0,
        )
        validate_travel_permit(proof, None, 10.2)
        for changes in ({"nearby_candidates": 1}, {"panels_closed": False}, {"observed_at": 9.0}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate_travel_permit(dict(proof, **changes), None, 10.2)

    def test_navigation_releases_combat_input_scans_once_and_recaptures_before_movement(self):
        for sent, labels_off in ((True, True), (True, False), (False, True)):
            with self.subTest(sent=sent, labels_off=labels_off):
                now, count, inspected_at = [0.0], [0], []
                backend, frame = DryFieldInput(), np.zeros((1140, 1922, 3), np.uint8)
                capture = Mock(target=dict(left=0, top=0))

                def grab():
                    now[0] += 0.1
                    count[0] += 1
                    backend.events.append(("capture", count[0]))
                    return SimpleNamespace(frame=frame)

                capture.grab.side_effect = grab
                profile = load_config(Path(__file__).absolute().parents[1] / "config").characters["flash"]
                path = Mock(teleport=dict(maximum_distance=600, preferred_distance=240, clearance_pixels=32))
                path.choose.return_value = PathChoice((480, 300), "observed_floor")
                hazards, hover, hud = Mock(), Mock(), Mock()
                hazards.observe.side_effect = lambda *args, **kwargs: (Enemy((1000, 600)),) if 3 <= count[0] <= 5 else ()
                hover.observe.return_value = ()
                hud.observe.return_value = SimpleNamespace(
                    life=SimpleNamespace(ratio=1), mana=SimpleNamespace(ratio=1), belt_visibility="closed"
                )
                loot = Mock(movement_allowed=True)

                def inspect(**kwargs):
                    self.assertEqual(("up", "f4"), backend.events[-1])
                    now[0] += 0.3
                    inspected_at.append(now[0])
                    backend.events.append(("scan", count[0]))
                    loot.movement_allowed = labels_off
                    return dict(status="inspection_complete", labels_state="off" if labels_off else "unknown")

                loot.after_combat.side_effect = inspect

                def plan(*args, **kwargs):
                    view = PathView(1, 0, now[0], (1922, 1140), (0, 0, 1920, 896), (960, 580), 8, np.ones((112, 240), bool))
                    return PathChoice((1450, 300), "observed_floor"), GroundReading((960, 580), "current"), view

                class UnsentInput(HeldFieldInput):
                    def apply(self, *args):
                        pass

                with patch("diablo2.runs.summoner.continuous_navigation.plan_arcane_step", side_effect=plan), patch(
                    "diablo2.runs.summoner.continuous_navigation.BackgroundMotionObserver"
                ) as motion, patch(
                    "diablo2.runs.summoner.continuous_navigation.HeldFieldInput", HeldFieldInput if sent else UnsentInput
                ), patch(
                    "diablo2.runs.summoner.continuous_navigation.threading", SimpleNamespace(Event=threading.Event, Thread=Mock())
                ):
                    motion.return_value.observe.return_value = CameraMotion((0, 0), 20, "verified")
                    motion.return_value.features = None
                    result = run_continuous_route(
                        capture,
                        hud,
                        backend,
                        lambda: True,
                        profile,
                        threading.Event(),
                        (Mock(reset_events=(), center=None, observe=Mock(return_value=GroundReading((960, 580), "current"))), path),
                        dict(
                            character_id="flash",
                            field=True,
                            monsters_clear=False,
                            belt_closed=True,
                            potions_remaining=3,
                            observed_at=time.time(),
                        ),
                        3,
                        evidence=[],
                        hazards=hazards,
                        hover_vision=hover,
                        reference_seconds=0.32,
                        clock=lambda: now[0],
                        emit=lambda *args: None,
                        loot_controller=loot,
                    )
                self.assertEqual(int(sent), loot.after_combat.call_count)
                if sent and labels_off:
                    rows = result["continuous_frames"]
                    scan = next(index for index, row in enumerate(rows) if row["action"] == "post_combat_loot_inspection")
                    self.assertGreaterEqual(rows[scan + 1]["acquired_at"], inspected_at[0])
                    events = list(backend.events)
                    scan_event = next(index for index, event in enumerate(events) if event[0] == "scan")
                    self.assertEqual("capture", events[scan_event + 1][0])
                elif sent:
                    self.assertEqual("item_labels_not_off", result["continuous_status"])
                    self.assertEqual("scan", backend.events[-1][0])

    def test_encounter_inspects_only_after_actual_nova_and_releases_input_first(self):
        for sent in (True, False):
            with self.subTest(sent=sent):
                now, count = [0.0], [0]
                backend, capture, hud, objective, appearance = DryFieldInput(), Mock(), Mock(), Mock(), Mock()
                capture.target = dict(left=0, top=0)
                frame = SimpleNamespace(shape=(1140, 1922, 3))

                def grab():
                    now[0] += 0.1
                    count[0] += 1
                    return SimpleNamespace(frame=frame)

                capture.grab.side_effect = grab
                hud.observe.return_value = SimpleNamespace(
                    life=SimpleNamespace(ratio=1), mana=SimpleNamespace(ratio=1), belt_visibility="closed"
                )
                objective.observe.return_value = SimpleNamespace(ground_point=(1000, 600))
                appearance.observe.side_effect = (
                    lambda *args: SummonerAppearance((980, 500, 1010, 560), (1000, 600), 0.99) if sent and count[0] < 5 else None
                )
                profile = SimpleNamespace(
                    survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
                    actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
                )
                loot = Mock(movement_allowed=True)

                def inspect(**kwargs):
                    self.assertEqual(("up", "f4"), backend.events[-1])
                    return dict(status="inspection_complete", kill_confirmed=False)

                loot.after_combat.side_effect = inspect
                path = Mock()
                path.choose.return_value = SimpleNamespace(aim=(1000, 600))
                with patch("diablo2.runs.summoner.encounter_execution.threading", SimpleNamespace(Event=threading.Event, Thread=Mock())):
                    result = run_platform_encounter(
                        capture,
                        hud,
                        backend,
                        lambda: True,
                        profile,
                        threading.Event(),
                        (SimpleNamespace(observe=lambda *args, **kwargs: GroundReading((962, 580), "current")), path),
                        objective,
                        appearance,
                        buff_expires_at=100,
                        reference_seconds=0.32,
                        evidence=[],
                        clock=lambda: now[0],
                        emit=lambda *args: None,
                        loot_controller=loot,
                    )
                self.assertEqual("kill_unconfirmed", result["status"])
                self.assertEqual(int(sent), loot.after_combat.call_count)


if __name__ == "__main__":
    unittest.main()
