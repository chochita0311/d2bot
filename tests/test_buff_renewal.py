import unittest
from dataclasses import replace
from pathlib import Path

from diablo2.common.buffing import BuffConfirmation, BuffRenewalLimits
from diablo2.common.config import load_config
from diablo2.common.field_control import FieldLimits, FieldObservation
from diablo2.common.field_runtime import create_field_runtime
from diablo2.common.survival import ObservationStamp, SafetyObservation


ROOT = Path(__file__).resolve().parents[1]


class BuffRenewalTests(unittest.TestCase):
    def setUp(self):
        self.config = load_config(ROOT / "config")
        self.at, self.serial, self.focused = 1.0, 0, True
        self.runtime = create_field_runtime(
            self.config,
            "flash",
            "room",
            FieldLimits(0.5, 1000, 3, 2, 1),
            buff_limits=BuffRenewalLimits(8, 2, 3),
            next_stamp=self.stamp,
            clock=lambda: self.at,
            focused=lambda: self.focused,
        )
        self.loop = self.runtime.loop
        self.state = self.loop.state
        self.backend = self.loop.inputs.backend
        self.backend.events = []
        self.state.observe_belt(self.stamp(), self.contents(12))

    def tearDown(self):
        self.runtime.stop()

    @staticmethod
    def contents(count):
        return {
            1: ("purple_potion", min(count, 4)),
            2: ("purple_potion", min(max(count - 4, 0), 4)),
            3: ("purple_potion", max(count - 8, 0)),
            4: ("town_portal_scroll", 4),
        }

    def stamp(self):
        self.serial += 1
        return ObservationStamp("flash", "room", self.serial, self.at)

    def scene(self, **changes):
        enemy = changes.pop("enemy", "clear")
        safety = changes.pop("safety", SafetyObservation(self.stamp(), 1, 1, "field", enemy == "clear"))
        return replace(FieldObservation(safety, True, enemy, (600, 400), True, True, True), **changes)

    def pump_scene(self, *, proof=False, **changes):
        scene = self.scene(**changes)
        evidence = BuffConfirmation(scene.safety.stamp, "validated_vision", True, True, True) if proof else None
        self.assertTrue(self.runtime.publish(scene, buff_confirmation=evidence))
        return self.runtime.pump()

    def advance(self, target, *, proof=False, **changes):
        # 큰 시간 점프 전에 새 관찰을 계속 제공하여 watchdog 중단과 구분한다.
        while self.at + 0.2 < target:
            self.at += 0.2
            self.pump_scene(**changes)
        self.at = target
        return self.pump_scene(proof=proof, **changes)

    def finish_sequence(self):
        while self.runtime.buffs.sequence.phase != "await_effect":
            self.advance(self.runtime.buffs.next_step_at)
        self.assertEqual(self.advance(self.runtime.buffs.next_step_at, proof=True).reason, "buff_confirmed")

    def pressed(self):
        return [event[1] for event in self.backend.events if event[0] == "press"]

    def test_initial_and_two_timer_renewals_use_profile_and_cast_request_time(self):
        self.assertEqual(self.pump_scene().reason, "buff_sequence_running")
        self.assertIsNone(self.state.next_buff_renewal_at)
        self.finish_sequence()
        order = ["w", "a", "a", "s", "d", "f1", "w"]
        self.assertEqual(self.pressed(), order)
        first_due = self.state.next_buff_renewal_at
        self.assertLess(first_due, self.at + 140)
        for repeat in (2, 3):
            self.advance(first_due - 0.001)
            self.assertEqual(len(self.pressed()), 7 * (repeat - 1))
            self.assertEqual(self.loop.inputs.held, {"f2"})
            self.at = first_due
            self.assertEqual(self.pump_scene().reason, "buff_sequence_running")
            self.assertFalse(self.loop.inputs.held)
            self.finish_sequence()
            self.assertEqual(self.pressed(), order * repeat)
            self.assertGreater(self.state.next_buff_renewal_at, first_due)
            first_due = self.state.next_buff_renewal_at

    def test_due_timer_releases_without_new_screen_and_never_dispatches_from_cache(self):
        self.state.confirm_buff(self.stamp(), "battle_orders", "field", True)
        due = self.state.next_buff_renewal_at
        self.at = due - 0.1
        self.pump_scene()
        self.assertEqual(self.loop.inputs.held, {"f2"})
        self.at = due
        self.assertEqual(self.runtime.pump().reason, "buff_observation_required")
        self.assertFalse(self.loop.inputs.held)
        self.assertEqual(self.pressed(), [])
        self.at += 0.5
        self.assertEqual(self.runtime.pump().reason, "buff_observation_stale")
        self.pump_scene()
        self.assertEqual(self.pressed(), [])

    def test_safe_site_wait_has_finite_budget_and_cannot_restart_after_timeout(self):
        self.assertEqual(self.pump_scene(enemy="near").reason, "buff_waiting_for_safe_field")
        self.assertEqual(self.advance(4, enemy="near").reason, "buff_safety_timeout")
        self.pump_scene()
        self.assertEqual(self.pressed(), [])

    def test_wait_for_positive_safe_site_then_dispatch(self):
        self.pump_scene(enemy="unknown")
        self.at += 0.1
        self.assertEqual(self.pump_scene().reason, "buff_sequence_running")
        self.assertEqual(self.pressed(), ["w"])

    def test_danger_or_belt_change_during_gap_cancels_before_next_token(self):
        for changes in ({"enemy": "near"}, {"enemy": "unknown"}, {"belt_closed": False}, {"belt_closed": None}):
            with self.subTest(changes=changes):
                self.runtime.stop()
                self.setUp()
                self.pump_scene()
                self.at += 0.05
                self.assertEqual(self.pump_scene(**changes).reason, "buff_safety_lost")
                self.at += 0.1
                self.pump_scene()
                self.assertEqual(self.pressed(), ["w"])

    def test_recovery_preempts_buff_once_and_empty_belt_keeps_exit_priority(self):
        self.pump_scene()
        self.at += 0.1
        low = SafetyObservation(self.stamp(), 0.49, 0.09, "field", True)
        self.assertEqual(self.pump_scene(safety=low).action, "use_potion")
        self.assertEqual(self.pressed(), ["w", "1"])
        self.at += 0.1
        self.pump_scene()
        self.assertEqual(self.pressed(), ["w", "1"])
        self.assertTrue(self.loop.reconcile_belt(self.stamp(), self.contents(0)))
        self.assertEqual(self.runtime.pump().action, "exit")
        self.assertFalse(self.loop.inputs.held)

    def test_pause_focus_loss_and_stop_cancel_partial_sequence(self):
        for action in ("pause", "focus", "stop"):
            with self.subTest(action=action):
                self.runtime.stop()
                self.setUp()
                self.pump_scene()
                if action == "pause":
                    self.runtime.pause()
                elif action == "focus":
                    self.focused = False
                    self.runtime.pump()
                    self.focused = True
                else:
                    self.runtime.stop()
                self.runtime.pause(False)
                self.at += 0.1
                if action != "stop":
                    self.pump_scene()
                self.assertIsNotNone(self.runtime.buffs.review_reason)
                self.assertEqual(self.pressed(), ["w"])
                self.assertFalse(self.loop.inputs.held)

    def test_confirmation_uses_same_new_frame_and_incomplete_proof_does_not_extend_timer(self):
        self.pump_scene()
        while self.runtime.buffs.sequence.phase != "await_effect":
            self.advance(self.runtime.buffs.next_step_at)
        self.advance(self.runtime.buffs.next_step_at)
        scene = self.scene()
        wrong = BuffConfirmation(ObservationStamp("flash", "other", 999, self.at), "validated_vision", True, True, True)
        self.assertFalse(self.runtime.publish(scene, buff_confirmation=wrong))
        bad = BuffConfirmation(scene.safety.stamp, "validated_vision", True, False, True)
        self.assertTrue(self.runtime.publish(scene, buff_confirmation=bad))
        self.assertEqual(self.runtime.pump().reason, "buff_confirmation_rejected")
        self.assertIsNone(self.state.next_buff_renewal_at)

    def test_confirmation_and_sequence_timeouts_block_further_input(self):
        self.pump_scene()
        self.runtime.buffs.limits = BuffRenewalLimits(0.3, 2, 3)
        self.assertEqual(self.advance(1.3).reason, "buff_sequence_timeout")
        self.assertEqual(self.pressed(), ["w"])
        self.runtime.stop()
        self.setUp()
        self.pump_scene()
        while self.runtime.buffs.sequence.phase != "await_effect":
            self.advance(self.runtime.buffs.next_step_at)
        deadline = self.runtime.buffs.confirmation_at + 2
        self.assertEqual(self.advance(deadline).reason, "buff_confirmation_timeout")
        self.assertIsNone(self.state.next_buff_renewal_at)
        self.at += 0.1
        self.pump_scene(proof=True)
        self.assertIsNone(self.state.next_buff_renewal_at)

    def test_input_exception_latches_review_and_releases_actor(self):
        def fail(key):
            raise RuntimeError("input failed")

        self.backend.press = fail
        with self.assertRaisesRegex(RuntimeError, "input failed"):
            self.pump_scene()
        self.assertEqual(self.runtime.buffs.review_reason, "buff_input_error")
        self.assertTrue(self.loop.stopped)
        self.assertFalse(self.loop.inputs.held)

    def test_duplicate_future_wrong_context_and_regressed_clock_never_send_next_token(self):
        scene = self.scene()
        self.runtime.publish(scene)
        self.runtime.pump()
        self.assertFalse(self.runtime.publish(scene))
        future = replace(scene, safety=replace(scene.safety, stamp=ObservationStamp("flash", "room", 999, 2)))
        self.assertFalse(self.runtime.publish(future))
        wrong = replace(scene, safety=replace(scene.safety, stamp=ObservationStamp("other", "room", 999, 1)))
        self.assertFalse(self.runtime.publish(wrong))
        self.at = 0.9
        self.runtime.pump()
        self.assertEqual(self.runtime.buffs.review_reason, "buff_clock_regressed")
        self.assertEqual(self.pressed(), ["w"])

    def test_dispatch_gap_starts_after_backend_completion(self):
        def slow_press(key):
            self.backend.events.append(("press", key))
            self.at += 0.1

        self.backend.press = slow_press
        self.pump_scene()
        due = self.runtime.buffs.next_step_at
        self.assertEqual(due, self.at + self.config.characters["flash"].actions.buff_action_pause_seconds["w"])
        self.advance(due - 0.001)
        self.assertEqual(self.pressed(), ["w"])
        self.at = due
        self.pump_scene()
        self.assertEqual(self.pressed(), ["w", "a"])

    def test_capture_queued_during_dispatch_is_dropped_without_restarting_sequence(self):
        def press_with_capture(key):
            self.backend.events.append(("press", key))
            self.runtime.publish(self.scene())

        self.backend.press = press_with_capture
        self.pump_scene()
        self.assertEqual(self.runtime.pump().reason, "buff_frame_precedes_dispatch")
        self.assertIsNone(self.runtime.buffs.review_reason)
        self.advance(self.runtime.buffs.next_step_at)
        self.assertEqual(self.pressed(), ["w", "a"])

    def test_invalid_limits_are_rejected(self):
        for value in (0, -1, float("inf"), True):
            with self.assertRaises(ValueError):
                BuffRenewalLimits(value, 1, 1)


if __name__ == "__main__":
    unittest.main()
