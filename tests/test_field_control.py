import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import Mock, patch

import cv2 as cv
import numpy as np

from diablo2.common.config import load_config
from diablo2.common.field_control import (
    AdaptiveTeleportCadence,
    DryFieldInput,
    FieldControlLoop,
    FieldLimits,
    FieldObservation,
    HeldFieldInput,
)
from diablo2.common.field_runtime import FieldControlRuntime, create_field_control
from diablo2.common.controller import BotController, ControllerFieldInput
from diablo2.common.movement import MOVEMENT_INTENT_TRAVEL, MovementExecutionState, apply_movement_intent, release_movement_intent
from diablo2.common.survival import CharacterSurvivalState, ObservationStamp, SafetyObservation
from diablo2.common.survival_vision import HudReading, ResourceReading, SurvivalHudObserver
from diablo2.tools.field_replay import replay


ROOT = Path(__file__).resolve().parents[1]


class FieldControlTests(unittest.TestCase):
    def setUp(self):
        config = load_config(ROOT / "config")
        self.profile = config.characters["flash"]
        self.state = CharacterSurvivalState("flash", "room", self.profile.survival)
        self.backend = DryFieldInput()
        self.inputs = HeldFieldInput(self.backend, "f2", "f4")
        self.estimate = config.casting_rules.estimate("resurrection", "sorceress", self.profile.casting)
        self.cadence = AdaptiveTeleportCadence(self.estimate, 2)
        self.loop = FieldControlLoop(self.state, self.inputs, FieldLimits(0.5, 1000, 3, 2, 1), self.cadence)
        self.state.observe_belt(self.stamp(0, 0), self.contents(12))
        self.state.confirm_buff(self.stamp(1, 0), "battle_orders", "field", True)

    @staticmethod
    def stamp(sequence, at):
        return ObservationStamp("flash", "room", sequence, at)

    @staticmethod
    def contents(count):
        return {
            1: ("purple_potion", min(count, 4)),
            2: ("purple_potion", min(max(count - 4, 0), 4)),
            3: ("purple_potion", max(count - 8, 0)),
            4: ("town_portal_scroll", 4),
        }

    def observation(self, sequence, at, **changes):
        enemy = changes.pop("enemy", "clear")
        safety = changes.pop("safety", SafetyObservation(self.stamp(sequence, at), 1, 1, "field", enemy == "clear"))
        return replace(FieldObservation(safety, True, enemy, (600, 400), True, False), **changes)

    def test_hold_travel_combat_clear_loot_resume_without_overlap(self):
        for sequence, at, changes, action in (
            (2, 1, {}, "travel"),
            (3, 1.2, {}, "travel"),
            (4, 1.3, {"enemy": "near"}, "combat"),
            (5, 1.4, {"enemy": "near"}, "combat"),
            (6, 1.5, {}, "inspect_loot"),
            (7, 1.6, {}, "inspect_loot"),
            (8, 1.7, {"loot_checked": True}, "travel"),
        ):
            self.assertEqual(self.loop.observe(self.observation(sequence, at, **changes), at).action, action)
            self.assertLessEqual(len(self.inputs.held), 1)
        keys = [event for event in self.backend.events if event[0] in ("down", "up")]
        self.assertEqual(keys, [("down", "f2"), ("up", "f2"), ("down", "f4"), ("up", "f4"), ("down", "f2")])
        self.loop.stop()
        self.assertEqual(self.backend.events[-1], ("up", "f2"))

    def test_missing_enemy_is_unknown_and_cannot_skip_loot(self):
        self.loop.observe(self.observation(2, 1, enemy="near"), 1)
        self.assertEqual(self.loop.observe(self.observation(3, 1.1, enemy="unknown"), 1.1).action, "hold")
        self.assertFalse(self.inputs.held)
        self.assertEqual(
            self.loop.observe(self.observation(4, 1.2, enemy="far", loot_checked=True), 1.2).reason, "combat_clear_unconfirmed"
        )
        self.assertEqual(self.loop.phase, "combat")

    def test_duplicate_future_old_wrong_context_and_open_belt_release(self):
        for changes, now in (
            ({}, 2),
            ({"safety": SafetyObservation(self.stamp(3, 4), 1, 1, "field", True)}, 3),
            ({"safety": SafetyObservation(ObservationStamp("other", "room", 3, 1.1), 1, 1, "field", True)}, 1.1),
            ({"belt_closed": False}, 1.1),
            ({"belt_closed": None}, 1.1),
            ({"reachable": None}, 1.1),
        ):
            self.setUp()
            self.loop.observe(self.observation(2, 1), 1)
            self.assertEqual(self.loop.observe(self.observation(3, 1.1, **changes), now).action, "hold")
            self.assertFalse(self.inputs.held)
        self.setUp()
        frame = self.observation(2, 1)
        self.loop.observe(frame, 1)
        self.assertEqual(self.loop.observe(frame, 1.1).reason, "screen_not_newer")
        self.assertFalse(self.inputs.held)

    def test_buff_timer_releases_without_new_vision_and_requires_safe_field(self):
        self.loop.observe(self.observation(2, 139.9), 139.9)
        self.assertEqual(self.inputs.held, {"f2"})
        self.assertEqual(self.loop.tick(140).reason, "buff_renewal_required")
        self.assertFalse(self.inputs.held)
        unsafe = SafetyObservation(self.stamp(3, 140.1), 1, 1, "field", False)
        self.assertEqual(self.loop.observe(self.observation(3, 140.1, safety=unsafe, enemy="near"), 140.1).reason, "buff_unsafe")
        self.assertEqual(self.loop.observe(self.observation(4, 140.2), 140.2).action, "buff")
        self.assertFalse(self.inputs.held)

    def test_potion_single_dispatch_waits_for_consumption_then_fresh_recovery(self):
        self.loop.observe(self.observation(2, 1, enemy="near"), 1)
        low = SafetyObservation(self.stamp(3, 1.1), 0.49, 0.09, "field", False)
        self.assertEqual(self.loop.observe(self.observation(3, 1.1, safety=low, enemy="near"), 1.1).action, "use_potion")
        self.assertEqual(list(self.backend.events)[-2:], [("up", "f4"), ("press", "1")])
        self.assertEqual(self.state.potions_remaining, 12)
        self.assertEqual(self.loop.observe(self.observation(4, 1.2, enemy="near"), 1.2).reason, "potion_outcome_pending")
        self.assertFalse(self.loop.acknowledge_potion(self.stamp(4, 1.2), 2))
        self.assertTrue(self.loop.acknowledge_potion(self.stamp(4, 1.2), 1))
        self.assertEqual(self.state.potions_remaining, 11)
        self.assertEqual(self.loop.observe(self.observation(5, 1.3, enemy="near"), 1.3).action, "combat")
        self.assertEqual(sum(event[0] == "press" for event in self.backend.events), 1)

    def test_unconfirmed_potion_timeout_never_retries_automatically(self):
        low = SafetyObservation(self.stamp(2, 1), 1, 0.01, "field", True)
        self.loop.observe(self.observation(2, 1, safety=low), 1)
        self.assertEqual(self.loop.observe(self.observation(3, 3), 3).reason, "potion_confirmation_timeout")
        self.assertEqual(list(self.backend.events), [("press", "1")])

    def test_zero_potions_preempts_unknown_screen_and_buff_deadline(self):
        self.loop.observe(self.observation(2, 1), 1)
        self.state.observe_belt(self.stamp(3, 1.1), self.contents(0))
        self.assertEqual(self.loop.tick(200).action, "exit")
        self.assertFalse(self.inputs.held)
        self.assertEqual(self.loop.observe(self.observation(2, 0), 200).action, "exit")
        self.assertFalse(any(event[0] == "press" for event in self.backend.events))

    def test_replenishment_latches_to_full_and_requires_far_enemy_destination(self):
        self.state.observe_belt(self.stamp(2, 1), self.contents(6))
        self.assertEqual(self.loop.observe(self.observation(3, 1.1, enemy="far"), 1.1).reason, "destination_unconfirmed")
        result = self.loop.observe(self.observation(4, 1.2, enemy="far", hunt_aim=(650, 450)), 1.2)
        self.assertTrue(result.hunt_visible)
        self.assertEqual(result.aim, (650, 450))
        self.state.observe_belt(self.stamp(5, 1.3), self.contents(7))
        self.assertTrue(self.loop.observe(self.observation(6, 1.4, enemy="near"), 1.4).hunt_visible)
        self.state.observe_belt(self.stamp(7, 1.5), self.contents(12))
        self.assertFalse(self.loop.observe(self.observation(8, 1.6, enemy="near"), 1.6).hunt_visible)

    def test_combat_budget_survives_unknown_observations(self):
        self.loop.observe(self.observation(2, 1, enemy="near"), 1)
        self.loop.observe(self.observation(3, 1.1, enemy="unknown"), 1.1)
        self.assertEqual(self.loop.observe(self.observation(4, 4.1, enemy="near"), 4.1).reason, "combat_timeout")
        self.assertFalse(self.inputs.held)

    def test_arrival_delay_adapts_and_misses_cannot_extend_timeout(self):
        self.loop.observe(self.observation(2, 1), 1)
        self.loop.observe(self.observation(3, 1.8, arrived=True, travel_aim=(700, 500)), 1.8)
        self.assertGreater(self.cadence.interval, self.cadence.reference_seconds)
        self.assertEqual(sum(event == ("down", "f2") for event in self.backend.events), 1)
        self.loop.observe(self.observation(4, 2.5, travel_aim=(800, 500)), 2.5)
        self.assertEqual(self.loop.observe(self.observation(5, 3.9), 3.9).reason, "arrival_timeout")
        self.assertFalse(self.inputs.held)

    def test_low_fcr_reference_slows_minimum_cadence(self):
        config = load_config(ROOT / "config")
        low = config.casting_rules.estimate("resurrection", "sorceress", replace(self.profile.casting, fcr=0))
        cadence = AdaptiveTeleportCadence(low, 2)
        self.assertEqual(cadence.reference_seconds, 13 / 25)
        self.assertEqual(self.cadence.reference_seconds, 8 / 25)

    def test_expanded_belt_reconciles_uncertain_potion_without_double_subtraction(self):
        low = SafetyObservation(self.stamp(2, 1), 0.4, 1, "field", True)
        self.assertEqual(self.loop.observe(self.observation(2, 1, safety=low), 1).action, "use_potion")
        self.assertFalse(self.loop.reconcile_belt(self.stamp(2, 1), self.contents(11)))
        self.assertTrue(self.loop.reconcile_belt(self.stamp(3, 1.1), self.contents(11)))
        self.assertEqual(self.state.potions_remaining, 11)
        self.assertFalse(self.loop.acknowledge_potion(self.stamp(4, 1.2), 1))
        self.assertEqual(self.state.potions_remaining, 11)
        self.assertEqual(self.loop.observe(self.observation(4, 1.2, belt_closed=False), 1.2).reason, "belt_not_confirmed_closed")
        self.assertEqual(self.loop.observe(self.observation(5, 1.3), 1.3).action, "travel")

    def test_invalid_belt_reconcile_preserves_potion_wait_and_empty_preempts(self):
        low = SafetyObservation(self.stamp(2, 1), 0.4, 1, "field", True)
        self.loop.observe(self.observation(2, 1, safety=low), 1)
        self.assertFalse(self.loop.reconcile_belt(self.stamp(3, 1.1), {}))
        self.assertIsNotNone(self.loop.potion_pending)
        self.assertTrue(self.loop.reconcile_belt(self.stamp(4, 1.2), self.contents(0)))
        self.assertEqual(self.loop.tick(1.3).action, "exit")

    def test_partial_key_down_failure_releases_and_latches_closed(self):
        def fail(key):
            self.backend.events.append(("down", key))
            raise RuntimeError("partial input")

        self.backend.key_down = fail
        with self.assertRaises(RuntimeError):
            self.loop.observe(self.observation(2, 1), 1)
        self.assertEqual(list(self.backend.events)[-2:], [("down", "f2"), ("up", "f2")])
        self.assertTrue(self.inputs.closed)
        self.assertFalse(self.inputs.held)

    def test_key_up_failure_is_retained_and_retry_releases(self):
        self.loop.observe(self.observation(2, 1), 1)
        original = self.backend.key_up
        self.backend.key_up = lambda key: (_ for _ in ()).throw(RuntimeError("up failed"))
        with self.assertRaises(RuntimeError):
            self.loop.stop()
        self.assertEqual(self.inputs.held, {"f2"})
        self.backend.key_up = original
        self.loop.stop()
        self.assertFalse(self.inputs.held)

    def test_mailbox_discards_older_and_watchdog_releases_capture_stall(self):
        clock = [1]
        runtime = FieldControlRuntime(self.loop, clock=lambda: clock[0], focused=lambda: True)
        self.assertTrue(runtime.publish(self.observation(2, 1)))
        clock[0] = 1.1
        self.assertTrue(runtime.publish(self.observation(3, 1.1)))
        self.assertFalse(runtime.publish(self.observation(2, 1)))
        clock[0] = 1.1
        self.assertEqual(runtime.pump().action, "travel")
        clock[0] = 1.7
        self.assertEqual(runtime.pump().reason, "input_lease_expired")
        self.assertFalse(self.inputs.held)
        runtime.stop()

    def test_actual_hud_belt_bridge_reconciles_in_actor_and_blocks_expanded_movement(self):
        low = SafetyObservation(self.stamp(2, 1), 0.4, 1, "field", True)
        self.loop.observe(self.observation(2, 1, safety=low), 1)
        observer = SurvivalHudObserver()
        ref = next(r for r in observer.manifest["references"] if r["path"] == "references/eleven.png")
        frame = np.zeros((1140, 1922, 3), np.uint8)
        l, t, r, b = ref["canvas_box"]
        frame[t:b, l:r] = cv.imread(str(observer.asset_directory / ref["path"]))
        reading = observer.observe(frame)
        runtime = FieldControlRuntime(self.loop, clock=lambda: 1.1, focused=lambda: True)
        self.assertTrue(runtime.publish_hud(reading, self.observation(3, 1.1)))
        self.assertEqual(self.state.potions_remaining, 12)
        self.assertEqual(runtime.pump().reason, "belt_not_confirmed_closed")
        self.assertEqual(self.state.potions_remaining, 11)
        self.assertIsNone(self.loop.potion_pending)
        self.assertFalse(self.inputs.held)
        runtime.stop()

    def test_hud_bridge_uses_same_frame_maximum_and_unknown_preempts_travel(self):
        at = [1]
        runtime = FieldControlRuntime(self.loop, clock=lambda: at[0], focused=lambda: True)
        healthy = HudReading("layout", "rev", ResourceReading(1580, 1580), ResourceReading(1543, 1543), "unused", belt_visibility="closed")
        runtime.publish_hud(healthy, self.observation(2, 1))
        self.assertEqual(runtime.pump().action, "travel")
        at[0] = 1.1
        low_after_buff = replace(healthy, life=ResourceReading(790, 1600))
        runtime.publish_hud(low_after_buff, self.observation(3, 1.1))
        self.assertEqual(runtime.pump().action, "use_potion")
        self.assertFalse(self.inputs.held)
        self.assertTrue(self.loop.acknowledge_potion(self.stamp(4, 1.2), 1))
        at[0] = 1.3
        runtime.publish_hud(replace(healthy, mana=ResourceReading()), self.observation(5, 1.3))
        self.assertEqual(runtime.pump().reason, "hud_unknown")
        self.assertFalse(self.inputs.held)
        runtime.stop()

    def test_queued_hud_belt_cannot_mutate_counts_after_its_screen_deadline(self):
        at = [1]
        runtime = FieldControlRuntime(self.loop, clock=lambda: at[0], focused=lambda: True)
        observer = SurvivalHudObserver()
        ref = next(r for r in observer.manifest["references"] if r["path"] == "references/eleven.png")
        frame = np.zeros((1140, 1922, 3), np.uint8)
        l, t, r, b = ref["canvas_box"]
        frame[t:b, l:r] = cv.imread(str(observer.asset_directory / ref["path"]))
        self.assertTrue(runtime.publish_hud(observer.observe(frame), self.observation(2, 1)))
        at[0] = 2
        self.assertEqual(runtime.pump().reason, "screen_stale")
        self.assertEqual(self.state.potions_remaining, 12)
        runtime.stop()

    def test_focus_loss_latches_pause_until_explicit_resume_with_new_screen(self):
        focused = [True]
        clock = [1]
        runtime = FieldControlRuntime(self.loop, clock=lambda: clock[0], focused=lambda: focused[0])
        runtime.publish(self.observation(2, 1))
        runtime.pump()
        focused[0] = False
        self.assertEqual(runtime.pump().action, "hold")
        self.assertFalse(self.inputs.held)
        focused[0] = True
        self.assertEqual(runtime.pump().action, "hold")
        runtime.pause(False)
        clock[0] = 1.1
        runtime.publish(self.observation(3, 1.1))
        self.assertEqual(runtime.pump().action, "travel")
        runtime.stop()

    def test_legacy_movement_final_stop_prevents_late_worker_rehold(self):
        class Session:
            _hold_key_down = self.backend.key_down
            _hold_key_up = self.backend.key_up
            _press_key = self.backend.press

        session, state = Session(), MovementExecutionState()
        apply_movement_intent(session, self.profile.actions, state, MOVEMENT_INTENT_TRAVEL)
        release_movement_intent(session, self.profile.actions, state, stop=True)
        apply_movement_intent(session, self.profile.actions, state, MOVEMENT_INTENT_TRAVEL)
        self.assertEqual(list(self.backend.events), [("down", "f2"), ("up", "f2")])

    def test_worker_exception_finally_releases_held_key(self):
        self.loop.observe(self.observation(2, 1), 1)
        runtime = FieldControlRuntime(self.loop, focused=lambda: (_ for _ in ()).throw(RuntimeError("focus worker failed")))
        runtime.start()
        runtime._thread.join(1)
        self.assertFalse(runtime._thread.is_alive())
        self.assertIsInstance(runtime.error, RuntimeError)
        self.assertFalse(self.inputs.held)
        self.assertTrue(self.inputs.closed)

    def test_controller_backend_focus_bounds_stop_and_unconditional_release(self):
        controller = BotController(dry_run=False)
        focused = [True]
        backend = ControllerFieldInput(controller, focused=lambda: focused[0], bounds=lambda: (100, 100, 1000, 1000))
        with patch("diablo2.common.controller.pydirectinput", Mock()) as driver:
            driver.keyDown.return_value = driver.keyUp.return_value = True
            backend.key_down("f2")
            driver.keyDown.assert_called_once_with("f2", _pause=False)
            with self.assertRaises(ValueError):
                backend.move(1000, 300)
            driver.moveTo.assert_not_called()
            focused[0] = False
            with self.assertRaises(RuntimeError):
                backend.key_down("f4")
            controller.stop_requested = True
            backend.key_up("f2")
            driver.keyUp.assert_called_once_with("f2", _pause=False)

    def test_backend_failed_press_still_releases_and_failed_release_is_not_success(self):
        backend = ControllerFieldInput(BotController(dry_run=False), focused=lambda: True, bounds=lambda: (0, 0, 100, 100))
        with patch("diablo2.common.controller.pydirectinput", Mock()) as driver:
            driver.keyDown.return_value, driver.keyUp.return_value = False, True
            with self.assertRaises(RuntimeError):
                backend.press("s")
            driver.keyUp.assert_called_once_with("s", _pause=False)
            driver.keyDown.return_value, driver.keyUp.return_value = True, False
            with self.assertRaises(RuntimeError):
                backend.key_up("f2")

    def test_controller_hotkey_callbacks_release_and_resume_requires_new_frame(self):
        controller = BotController(dry_run=True)
        controller.attach_field_control(self.loop)
        frame = self.observation(2, 1)
        self.loop.observe(frame, 1)
        controller.toggle_pause()
        self.assertFalse(self.inputs.held)
        controller.toggle_pause()
        self.assertEqual(self.loop.observe(frame, 1.1).reason, "screen_not_newer")
        self.loop.observe(self.observation(3, 1.2), 1.2)
        controller.request_stop()
        self.assertFalse(self.inputs.held)
        self.assertTrue(self.inputs.closed)

    def test_factory_defaults_to_dry_input_and_uses_character_policy_and_class(self):
        config = load_config(ROOT / "config")
        config.dry_run = False
        loop = create_field_control(config, "flash", "another", self.loop.limits)
        self.assertIsInstance(loop.inputs.backend, DryFieldInput)
        self.assertEqual(loop.inputs.keys, ("f2", "f4"))
        self.assertEqual(loop.state.room_id, "another")
        self.assertEqual(loop.state.policy, config.characters["flash"].survival)
        self.assertEqual(loop.cadence.reference_seconds, 0.32)
        with self.assertRaises(ValueError):
            create_field_control(config, "constantin", "another", self.loop.limits)

    def test_bounded_replay_integrates_config_belt_buff_modes_and_watchdog(self):
        scenario = {
            "evidence": "synthetic",
            "limits": {"screen_age": 0.5, "belt_age": 1000, "combat_seconds": 3, "arrival_seconds": 2, "potion_confirmation_seconds": 1},
            "events": [
                {"at": 0, "sequence": 0, "type": "belt", "contents": self.contents(12)},
                {"at": 0, "sequence": 1, "type": "buff", "name": "battle_orders", "location": "field", "monsters_clear": True},
            ],
        }
        for sequence, at, enemy, loot in (
            (2, 1, "clear", False),
            (3, 1.1, "near", False),
            (4, 1.2, "clear", False),
            (5, 1.3, "clear", True),
        ):
            scenario["events"].append(
                {
                    "at": at,
                    "sequence": sequence,
                    "type": "observation",
                    "field": {
                        "safety": {"life_ratio": 1, "mana_ratio": 1, "location": "field", "monsters_clear": enemy == "clear"},
                        "belt_closed": True,
                        "enemy": enemy,
                        "travel_aim": [600, 400],
                        "reachable": True,
                        "loot_checked": loot,
                    },
                }
            )
        scenario["events"].append({"at": 2, "type": "tick"})
        result = replay(load_config(ROOT / "config"), "flash", "demo", scenario)
        self.assertEqual(result["inputs_sent"], 0)
        self.assertEqual([row["action"] for row in result["results"]][2:], ["travel", "combat", "inspect_loot", "travel", "hold"])
        self.assertEqual(result["dry_input_events"][-1], ("up", "f2"))
        scenario["evidence"] = "validated_vision"
        with self.assertRaises(ValueError):
            replay(load_config(ROOT / "config"), "flash", "demo", scenario)

    def test_control_clock_regression_releases_and_requires_new_controller(self):
        self.loop.observe(self.observation(2, 1), 1)
        self.assertEqual(self.loop.tick(0.5).reason, "clock_regressed")
        self.assertFalse(self.inputs.held)
        self.assertTrue(self.inputs.closed)
        self.assertEqual(self.loop.observe(self.observation(3, 1.1), 1.1).action, "hold")

    def test_visible_enemy_cannot_be_admitted_as_safe_buff_site(self):
        for enemy in ("near", "far", "unknown"):
            with self.assertRaises(ValueError):
                FieldObservation(SafetyObservation(self.stamp(2, 1), 1, 1, "field", True), True, enemy)


if __name__ == "__main__":
    unittest.main()
