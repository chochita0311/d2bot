import unittest
from dataclasses import replace

import cv2 as cv
import numpy as np

from diablo2.common.buffing import BeltInspection, BuffConfirmation, BuffSequence, BuffSequenceExecutor
from diablo2.common.buffing_vision import WeaponSetObserver, resource_increase_candidate
from diablo2.common.config import load_config
from diablo2.common.survival import CharacterSurvivalState, ObservationStamp, SafetyObservation
from diablo2.common.survival_vision import SurvivalHudObserver
from diablo2.common.field_control import FieldObservation
from tests.test_survival_vision import ROOT


class BuffingTests(unittest.TestCase):
    def setUp(self):
        profile = load_config(ROOT / "config").characters["flash"]
        self.state = CharacterSurvivalState("profile", "room", profile.survival)
        self.order = profile.actions.pre_run_buff_order
        self.observer = SurvivalHudObserver()
        self.state.observe_belt(
            self.stamp(0, 0), {1: ("purple_potion", 4), 2: ("purple_potion", 4), 3: ("purple_potion", 4), 4: ("town_portal_scroll", 4)}
        )

    @staticmethod
    def stamp(sequence, time):
        return ObservationStamp("profile", "room", sequence, time)

    def observation(self, sequence, time, **changes):
        return replace(SafetyObservation(self.stamp(sequence, time), 1, 1, "field", True), **changes)

    def reference(self, name):
        reference = next(ref for ref in self.observer.manifest["references"] if ref["path"] == f"references/{name}.png")
        frame = np.zeros((1140, 1922, 3), np.uint8)
        l, t, r, b = reference["canvas_box"]
        frame[t:b, l:r] = cv.imread(str(self.observer.asset_directory / reference["path"]))
        return self.observer.observe(frame)

    def sent_sequence(self):
        transaction = BuffSequence(self.state, self.order)
        tokens = []
        for index in range(len(self.order)):
            observation = self.observation(index * 2 + 1, index + 1)
            directive = transaction.next_step(observation, index + 1, screen_age=1, belt_age=100)
            self.assertEqual(directive.action, "input_request")
            tokens.append(directive.token)
            self.assertTrue(transaction.acknowledge_step(index, self.stamp(index * 2 + 2, index + 1.1), True))
        self.assertEqual(tokens, ["w", "a", "a", "s", "f1", "d", "w"])
        return transaction

    def test_dispatch_alone_cannot_confirm_effects(self):
        transaction = self.sent_sequence()
        self.assertEqual(transaction.phase, "await_effect")
        self.assertEqual(transaction.next_step(self.observation(15, 8), 8, screen_age=1, belt_age=100).reason, "effect_unconfirmed")
        self.assertEqual(
            self.state.decide(self.observation(15, 8), 8, max_observation_age_seconds=1, max_belt_age_seconds=100).action, "buff"
        )

    def executor_case(self, *, unsafe_step=None, stop_on_wait=False, timeout=10):
        transaction = BuffSequence(self.state, self.order)
        executor = BuffSequenceExecutor(
            transaction, {"a": 0.1, "s": 0.2, "f1": 0.3, "d": 0.4}, default_pause=0.5, screen_age=0.5, belt_age=100, timeout=timeout
        )
        at, serial, sent, gaps = [1.0], [0], [], []

        def stamp():
            serial[0] += 1
            return self.stamp(serial[0], at[0])

        def observe():
            clear = unsafe_step is None or len(sent) != unsafe_step
            return FieldObservation(SafetyObservation(stamp(), 1, 1, "field", clear), True, "clear" if clear else "near")

        def wait(seconds):
            gaps.append(seconds)
            at[0] += seconds
            return stop_on_wait

        result = executor.run(
            observe=observe, send_token=sent.append, next_stamp=stamp, clock=lambda: at[0], wait=wait, interrupted=lambda: False
        )
        return result, transaction, sent, gaps

    def test_profile_buff_executor_runs_continuously_and_keeps_effect_confirmation_separate(self):
        result, transaction, sent, gaps = self.executor_case()
        self.assertEqual(sent, ["w", "a", "a", "s", "f1", "d", "w"])
        self.assertEqual(gaps, [0.5, 0.1, 0.1, 0.2, 0.3, 0.4, 0.5])
        self.assertEqual(result.action, "await_confirmation")
        self.assertEqual(transaction.phase, "await_effect")
        self.assertIsNone(self.state.next_buff_renewal_at)

    def test_continuous_buff_executor_stops_when_enemy_appears_or_wait_interrupted(self):
        result, transaction, sent, _ = self.executor_case(unsafe_step=2)
        self.assertEqual(sent, ["w", "a"])
        self.assertEqual(result.reason, "field_safety_unverified")
        self.assertEqual(transaction.phase, "cancelled")
        result, transaction, sent, _ = self.executor_case(stop_on_wait=True)
        self.assertEqual(sent, ["w"])
        self.assertEqual(result.reason, "sequence_interrupted")
        self.assertEqual(transaction.phase, "cancelled")

    def test_timeout_during_buff_gap_cancels_remaining_keys_and_timer(self):
        result, transaction, sent, gaps = self.executor_case(timeout=0.25)
        self.assertEqual(result.reason, "sequence_interrupted")
        self.assertEqual(sent, ["w"])
        self.assertEqual(gaps, [0.25])
        self.assertEqual(transaction.phase, "cancelled")
        self.assertIsNone(self.state.next_buff_renewal_at)

    def test_confirmed_timer_uses_cast_request_not_final_review_time(self):
        transaction = self.sent_sequence()
        observation = self.observation(15, 8)
        proof = BuffConfirmation(observation.stamp, "supervised_visual", True, True, True)
        self.assertTrue(transaction.confirm_effect("battle_orders", proof, observation, 8, screen_age=1, belt_age=100))
        self.assertEqual(transaction.phase, "complete")
        for time, action in ((143.9, "continue"), (144, "buff"), (154, "buff")):
            self.assertEqual(
                self.state.decide(self.observation(20, time), time, max_observation_age_seconds=1, max_belt_age_seconds=1000).action, action
            )
        self.assertFalse(transaction.confirm_effect("battle_orders", proof, observation, 8, screen_age=1, belt_age=100))
        self.assertEqual(transaction.next_step(self.observation(21, 144), 144, screen_age=1, belt_age=1000).reason, "buff_renewal_required")

    def test_town_unknown_enemies_old_screen_and_context_block(self):
        for changes, now in (
            ({"location": "town"}, 1),
            ({"location": "unknown"}, 1),
            ({"monsters_clear": None}, 1),
            ({"monsters_clear": False}, 1),
            ({}, 3),
            ({"stamp": ObservationStamp("another", "room", 1, 1)}, 1),
        ):
            transaction = BuffSequence(self.state, self.order)
            directive = transaction.next_step(self.observation(1, 1, **changes), now, screen_age=1, belt_age=100)
            self.assertNotEqual(directive.action, "input_request")

    def test_pending_input_is_not_resent_and_failure_cannot_resume(self):
        transaction = BuffSequence(self.state, self.order)
        transaction.next_step(self.observation(1, 1), 1, screen_age=1, belt_age=100)
        self.assertEqual(transaction.next_step(self.observation(2, 1.1), 1.1, screen_age=1, belt_age=100).reason, "input_outcome_pending")
        self.assertFalse(transaction.acknowledge_step(1, self.stamp(2, 1.1), True))
        self.assertFalse(transaction.acknowledge_step(0, self.stamp(2, 1.1), False))
        self.assertEqual(transaction.next_step(self.observation(3, 2), 2, screen_age=1, belt_age=100).reason, "sequence_cancelled")

    def test_danger_and_potion_priority_cancel_active_sequence(self):
        for life, clear, action in ((0.49, True, "use_potion"), (1, False, "hold")):
            transaction = BuffSequence(self.state, self.order)
            transaction.next_step(self.observation(1, 1), 1, screen_age=1, belt_age=100)
            transaction.acknowledge_step(0, self.stamp(2, 1.1), True)
            self.assertEqual(
                transaction.next_step(self.observation(3, 2, life_ratio=life, monsters_clear=clear), 2, screen_age=1, belt_age=100).action,
                action,
            )
            self.assertEqual(transaction.phase, "cancelled")
        self.state.observe_belt(self.stamp(10, 3), {1: (None, 0), 2: (None, 0), 3: (None, 0), 4: ("town_portal_scroll", 4)})
        self.assertEqual(
            BuffSequence(self.state, self.order).next_step(self.observation(11, 4), 4, screen_age=1, belt_age=100).action, "exit"
        )

    def test_incomplete_or_late_visual_proof_never_starts_timer(self):
        for field in ("effect_verified", "equipment_restored", "sequence_effects_verified"):
            transaction = self.sent_sequence()
            observation = self.observation(15, 8)
            proof = replace(BuffConfirmation(observation.stamp, "supervised_visual", True, True, True), **{field: False})
            self.assertFalse(transaction.confirm_effect("battle_orders", proof, observation, 8, screen_age=1, belt_age=100))
        transaction = self.sent_sequence()
        observation = self.observation(15, 144)
        self.assertFalse(
            transaction.confirm_effect(
                "battle_orders",
                BuffConfirmation(observation.stamp, "supervised_visual", True, True, True),
                observation,
                144,
                screen_age=1,
                belt_age=1000,
            )
        )
        self.assertEqual(transaction.phase, "cancelled")
        with self.assertRaises(ValueError):
            BuffConfirmation(observation.stamp, "key_dispatch", True, True, True)

    def test_invalid_sequence_and_missing_timed_key_fail(self):
        for order in ([], [""], ["w", 1], ["w", "a", "w"]):
            with self.assertRaises(ValueError):
                BuffSequence(self.state, order)

    def test_belt_inspection_requires_positive_closed_evidence(self):
        self.state.confirm_buff(self.stamp(0, 0), "battle_orders", "field", True)
        inspection = BeltInspection(self.state)
        self.assertEqual(inspection.request_toggle(), "`")
        self.assertTrue(inspection.observe(self.stamp(1, 1), self.reference("expanded")))
        self.assertFalse(inspection.permits_movement(self.observation(2, 2), 2, screen_age=1, belt_age=100))
        inspection.request_toggle()
        unknown = replace(self.reference("closed"), belt_visibility="unknown")
        self.assertFalse(inspection.observe(self.stamp(3, 3), unknown))
        self.assertFalse(inspection.permits_movement(self.observation(4, 4), 4, screen_age=1, belt_age=100))
        self.assertTrue(inspection.observe(self.stamp(5, 5), self.reference("closed")))
        self.assertTrue(inspection.permits_movement(self.observation(6, 5.1), 5.1, screen_age=1, belt_age=100))
        self.assertFalse(inspection.permits_movement(self.observation(7, 7), 7, screen_age=1, belt_age=100))
        inspection.request_toggle()
        self.assertFalse(inspection.permits_movement(self.observation(8, 7), 7, screen_age=1, belt_age=100))

    def test_bad_expanded_belt_invalidates_counts_and_wrong_context_cannot_close(self):
        inspection = BeltInspection(self.state)
        reading = replace(self.reference("expanded"), belt_status="slot_unverified")
        self.assertFalse(inspection.observe(self.stamp(1, 1), reading))
        self.assertIsNone(self.state.potions_remaining)
        self.assertFalse(inspection.observe(ObservationStamp("profile", "other-room", 2, 2), self.reference("closed")))
        self.assertEqual(inspection.visibility, "expanded")

    def test_actual_field_numbers_and_changed_label_positions(self):
        for name, values in (
            ("field-before", (985, 1075)),
            ("field-swapped", (895, 1075)),
            ("before-orders", (895, 1075)),
            ("after-orders", (1490, 1543)),
            ("field-restored", (1580, 1543)),
        ):
            reading = self.reference(name)
            self.assertEqual(
                (reading.life.current, reading.life.maximum, reading.mana.current, reading.mana.maximum),
                (values[0], values[0], values[1], values[1]),
            )
            self.assertEqual(reading.belt_visibility, "closed")
        self.assertTrue(resource_increase_candidate(self.reference("before-orders"), self.reference("after-orders")))
        self.assertFalse(resource_increase_candidate(self.reference("field-before"), self.reference("field-swapped")))

    def test_closed_anchor_occlusion_is_unknown_and_keeps_hidden_counts_unknown(self):
        reference = self.reference("closed")
        self.assertIsNone(reference.belt_contents(self.state.policy))
        frame = np.zeros((1140, 1922, 3), np.uint8)
        ref = next(ref for ref in self.observer.manifest["references"] if ref["path"] == "references/closed.png")
        l, t, r, b = ref["canvas_box"]
        frame[t:b, l:r] = cv.imread(str(self.observer.asset_directory / ref["path"]))
        l, t, r, b = self.observer.manifest["belt"]["closed_anchor_box"]
        frame[t:b, l:r] = 255
        reading = self.observer.observe(frame)
        self.assertEqual(reading.belt_visibility, "unknown")

    def test_weapon_set_tabs_require_both_regions_and_do_not_guess_equipment(self):
        observer = WeaponSetObserver()
        for number, template in observer.templates.items():
            frame = np.zeros((1140, 1922, 3), np.uint8)
            start = 0
            for l, t, r, b in observer.manifest["regions"]:
                frame[t:b, l:r] = template[:, start : start + r - l]
                start += r - l
            self.assertEqual(observer.observe(frame), number)
            l, t, r, b = observer.manifest["regions"][0]
            frame[t:b, l:r] = 0
            self.assertIsNone(observer.observe(frame))
        self.assertIsNone(observer.observe(np.zeros((1140, 1922, 3), np.uint8)))
        self.assertIsNone(observer.observe(np.zeros((753, 1267, 3), np.uint8)))


if __name__ == "__main__":
    unittest.main()
