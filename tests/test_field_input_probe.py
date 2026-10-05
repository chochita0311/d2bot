import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

from diablo2.common.config import load_config
from diablo2.common.field_control import DryFieldInput, HeldFieldInput
from diablo2.common.survival_vision import ResourceReading
from diablo2.tools.field_input_probe import RecordingInput, hold_probe, validate_permit, wait_readable_resources


class FieldProbeTests(unittest.TestCase):
    def setUp(self):
        self.policy = load_config(Path(__file__).resolve().parents[1] / "config").characters["flash"].survival
        self.permit = dict(
            character_id="flash",
            observed_at=100,
            field=True,
            monsters_clear=True,
            belt_closed=True,
            buff_effect_verified=True,
            equipment_restored=True,
            potions_remaining=11,
            buff_casts={"battle_orders": 90},
        )
        self.reading = SimpleNamespace(life=ResourceReading(1580, 1580), mana=ResourceReading(1543, 1543), belt_visibility="closed")

    def test_recent_permit_cannot_extend_expired_buff_or_accept_missing_time(self):
        validate_permit(self.permit, "flash", self.policy, 110)
        for casts in ({}, {"battle_orders": -30}, {"battle_orders": 111}):
            with self.assertRaises(ValueError):
                validate_permit(dict(self.permit, buff_casts=casts), "flash", self.policy, 110)
        with self.assertRaises(ValueError):
            validate_permit(self.permit, "flash", self.policy, 116)

    def probe(self, readings, stop=None, focused=lambda: True, deadline=None):
        driver = DryFieldInput()
        backend = RecordingInput(driver)
        inputs = HeldFieldInput(backend, "f2", "f4")
        hud = Mock()
        hud.observe.side_effect = readings
        capture = Mock()
        capture.grab.return_value = SimpleNamespace(frame=None)
        result = hold_probe(
            capture,
            hud,
            inputs,
            focused,
            self.policy,
            "travel",
            0.06,
            (100, 100),
            stop or threading.Event(),
            buff_deadline=deadline or time.monotonic() + 10,
        )
        return result, backend.events

    def test_live_adapter_releases_after_hud_becomes_unknown(self):
        unknown = SimpleNamespace(life=ResourceReading(), mana=self.reading.mana, belt_visibility="closed")
        result, events = self.probe([self.reading, unknown])
        self.assertEqual("observation_unverified", result["status"])
        self.assertEqual([], result["held_after"])
        self.assertEqual(["key_down", "key_up"], [e["action"] for e in events if e["action"].startswith("key_")])

    def test_watchdog_releases_while_capture_stalls(self):
        def reading(frame):
            if len(calls):
                time.sleep(0.12)
            calls.append(1)
            return self.reading

        calls = []
        result, events = self.probe(reading)
        self.assertEqual("duration_complete", result["status"])
        down, up = [e for e in events if e["action"].startswith("key_")]
        self.assertLess(up["at"] - down["at"], 0.1)
        self.assertEqual([], result["held_after"])

    def test_pause_before_start_sends_no_key_and_reports_interruption(self):
        stop = threading.Event()
        stop.set()
        result, events = self.probe([], stop=stop)
        self.assertEqual("interrupted", result["status"])
        self.assertEqual([], events)

    def test_buff_waits_without_input_for_new_readable_same_frame_resources(self):
        hud, capture, stop = Mock(), Mock(), threading.Event()
        capture.grab.return_value = SimpleNamespace(frame=None)
        unknown = SimpleNamespace(life=ResourceReading(), mana=ResourceReading(), belt_visibility="closed")
        hud.observe.side_effect = [unknown, self.reading]
        self.assertIsNone(wait_readable_resources(capture, hud, lambda: True, self.policy, stop, time.monotonic() + 0.1))
        hud.observe.side_effect = None
        hud.observe.return_value = unknown
        self.assertEqual(
            wait_readable_resources(capture, hud, lambda: True, self.policy, stop, time.monotonic() + 0.03), "observation_unverified"
        )

    def test_focus_loss_and_due_buff_have_distinct_outcomes(self):
        result, events = self.probe([self.reading], focused=lambda: False)
        self.assertEqual("focus_lost", result["status"])
        self.assertEqual([], events)
        result, events = self.probe(lambda frame: self.reading, deadline=time.monotonic() + 0.035)
        self.assertEqual("buff_due", result["status"])
        self.assertEqual([], result["held_after"])


if __name__ == "__main__":
    unittest.main()
