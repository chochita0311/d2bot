import threading
import unittest
from types import SimpleNamespace
from unittest.mock import Mock

from diablo2.common.config import load_config
from diablo2.common.emergency_recovery import EmergencyResourceRecovery
from diablo2.common.field_control import DryFieldInput
from diablo2.common.survival_vision import HudReading, ResourceReading, SlotReading


class EmergencyRecoveryTests(unittest.TestCase):
    def exercise(self, condition="success", maximum=1000):
        policy = load_config("config").characters["flash"].survival
        now, focused, stop = [0.0], [True], threading.Event()
        backend, capture, hud = DryFieldInput(), Mock(), Mock()
        frame = SimpleNamespace(shape=(1140, 1922, 3))
        low = ResourceReading(maximum // 20, maximum)
        high = ResourceReading(maximum * 9 // 10, maximum)
        initial = HudReading("test", "test", high, low, "not_verified_expanded", belt_visibility="closed")
        slots = tuple(
            SlotReading(
                row,
                column,
                "empty"
                if (column == 1 and row == 1) or (column != 4 and condition == "empty")
                else "town_portal_scroll"
                if column == 4
                else "purple_potion",
                0,
            )
            for row in range(1, 5)
            for column in range(1, 5)
        )

        def grab():
            now[0] += 0.1
            return SimpleNamespace(frame=frame)

        def observe(seen):
            toggles = backend.events.count(("press", policy.belt_toggle_key))
            expanded = toggles % 2 or (condition == "closure" and toggles > 0)
            return HudReading(
                "test",
                "test",
                high,
                low if condition == "no_effect" else high,
                "slot_unverified" if condition == "unreadable_belt" else "read",
                slots if expanded else (),
                "expanded" if expanded else "closed",
            )

        def press(key):
            backend.events.append(("press", key))
            if condition == "stop_after_press":
                stop.set()
            if condition == "focus_after_press":
                focused[0] = False

        backend.press = press
        capture.grab.side_effect = grab
        hud.observe.side_effect = observe
        hud.closed_belt_slots.return_value = tuple(
            SlotReading(4, column, "town_portal_scroll" if condition == "wrong_kind" or column == 4 else "purple_potion", 0)
            for column in range(1, 5)
        )
        if condition == "stale":
            now[0] = 0.3
        if condition == "boundary":
            initial = HudReading(
                "test",
                "test",
                ResourceReading(maximum // 2, maximum),
                ResourceReading(maximum // 10, maximum),
                "read",
                belt_visibility="closed",
            )
        records, evidence = [], []
        consumer = EmergencyResourceRecovery(
            capture, hud, backend, lambda: focused[0], policy, stop, records=records, evidence=evidence, clock=lambda: now[0]
        )
        result = consumer(frame, initial, 0)
        return result, list(backend.events), records, evidence

    def test_recovery_uses_current_ratio_and_observed_counts_then_closes_belt(self):
        for maximum in (1000, 5000):
            with self.subTest(maximum=maximum):
                result, events, records, evidence = self.exercise(maximum=maximum)
                self.assertTrue(result)
                self.assertEqual([("press", "1"), ("press", "`"), ("press", "`")], events)
                self.assertEqual(11, records[0]["potions_remaining"])
                self.assertEqual({1: 3, 2: 4, 3: 4, 4: 4}, records[0]["belt_counts"])
                self.assertEqual(1, records[0]["potion_requests"])
                self.assertTrue(records[0]["belt_closed_verified"])
                self.assertEqual(3, len(evidence))

    def test_stale_boundary_or_nonpotion_slots_never_dispatch(self):
        for condition in ("stale", "boundary", "wrong_kind"):
            with self.subTest(condition=condition):
                result, events, records, evidence = self.exercise(condition)
                self.assertFalse(result)
                self.assertFalse(events)
                self.assertNotIn("potions_remaining", records[0])

    def test_unconfirmed_outcomes_never_retry_or_invent_counts(self):
        for condition in ("no_effect", "unreadable_belt", "closure", "stop_after_press", "focus_after_press"):
            with self.subTest(condition=condition):
                result, events, records, evidence = self.exercise(condition)
                self.assertFalse(result)
                self.assertEqual(1, events.count(("press", "1")))
                self.assertNotIn(("press", "4"), events)
                self.assertLessEqual(len(evidence), 3)
                if condition != "closure":
                    self.assertNotIn("potions_remaining", records[0])
                if condition in ("stop_after_press", "focus_after_press"):
                    self.assertEqual([("press", "1")], events)

    def test_actual_empty_belt_stops_after_reconciliation_even_if_resources_rose(self):
        result, events, records, evidence = self.exercise("empty")
        self.assertFalse(result)
        self.assertEqual("potions_empty", records[0]["status"])
        self.assertEqual(0, records[0]["potions_remaining"])
        self.assertTrue(records[0]["belt_closed_verified"])
        self.assertEqual(1, events.count(("press", "1")))


if __name__ == "__main__":
    unittest.main()
