import queue
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

from diablo2.common.config import load_config
from diablo2.common.field_control import DryFieldInput, HeldFieldInput
from diablo2.common.survival_vision import HudReading, ResourceReading
from diablo2.common.supervised_field import StaleSupervision
from diablo2.runs.summoner.navigation import CameraMotion, PathSettings
from diablo2.runs.summoner.scene_vision import GroundReading
from diablo2.tools.field_input_probe import plan_arcane_step
from diablo2.tools.field_input_probe import (
    RecordingInput,
    confirm_initial_probe,
    dispatch_buffs,
    dispatch_planned_step,
    hold_probe,
    observe_step_landing,
    read_probe_signals,
    validate_permit,
    wait_probe_signal,
    wait_readable_resources,
    wait_renewal_deadline,
)


class FieldProbeTests(unittest.TestCase):
    def test_signal_reader_supports_five_rooms_and_bounded_refreshes(self):
        messages = ['{"phase":"field_safety"}', '{"phase":"initial_effect"}'] * 6
        with patch("builtins.input", side_effect=messages):
            signals = read_probe_signals(max_signals=12)
            self.assertEqual(12, len([signals.get(timeout=1) for _ in range(12)]))
        for count in (0, 33, True):
            with self.assertRaises(ValueError):
                read_probe_signals(max_signals=count)

    def test_confirmation_refresh_consumes_stale_proof_without_renewing_its_time(self):
        signals = queue.Queue()
        stale = dict(self.permit, phase="field_safety", observed_at=100)
        fresh = dict(self.permit, phase="field_safety", observed_at=115)
        for signal in (stale, fresh):
            signals.put(signal)
        emitted = []
        with patch("diablo2.common.supervised_field.wait_readable_resources", return_value=None) as read:
            received = wait_probe_signal(
                signals,
                capture=Mock(),
                hud=Mock(),
                focused=lambda: True,
                policy=self.policy,
                stop=threading.Event(),
                deadline=200,
                phase="field_safety",
                accept=lambda value: validate_permit(value, "flash", self.policy, 116, buff=True),
                clock=lambda: 150,
                retry_stale=True,
                emit=emitted.append,
            )
        self.assertIs(received, fresh)
        self.assertEqual(100, stale["observed_at"])
        self.assertEqual(2, read.call_count)
        self.assertIn("age 16.000s", emitted[0])

    def test_confirmation_refresh_does_not_swallow_invalid_character_or_future_time(self):
        for changes in (dict(character_id="other"), dict(observed_at=117), dict(potions_remaining=0)):
            signals = queue.Queue()
            signals.put(dict(self.permit, phase="field_safety", **changes))
            with patch("diablo2.common.supervised_field.wait_readable_resources", return_value=None), self.assertRaises(
                ValueError
            ) as error:
                wait_probe_signal(
                    signals,
                    capture=Mock(),
                    hud=Mock(),
                    focused=lambda: True,
                    policy=self.policy,
                    stop=threading.Event(),
                    deadline=200,
                    phase="field_safety",
                    accept=lambda value: validate_permit(value, "flash", self.policy, 116, buff=True),
                    clock=lambda: 150,
                    retry_stale=True,
                )
            self.assertNotIsInstance(error.exception, StaleSupervision)

    def test_main_signal_reader_accepts_safety_effect_and_three_continuations(self):
        messages = ['{"phase":"field_safety"}', '{"phase":"initial_effect"}'] + ['{"phase":"navigation_continue"}'] * 3
        with patch("builtins.input", side_effect=messages):
            signals = read_probe_signals(max_signals=5)
            self.assertEqual(
                ["field_safety", "initial_effect", "navigation_continue", "navigation_continue", "navigation_continue"],
                [signals.get(timeout=1)["phase"] for _ in range(5)],
            )

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

    def buff_trace(self, order, trace, *, hud=None, clock=None):
        driver = RecordingInput(DryFieldInput())
        capture = Mock()
        capture.grab.return_value = SimpleNamespace(frame=object())
        observer = hud or Mock()
        if hud is None:
            observer.observe.return_value = HudReading(
                "test", "test", self.reading.life, self.reading.mana, "closed", belt_visibility="closed"
            )
        profile = SimpleNamespace(
            actions=SimpleNamespace(pre_run_buff_order=order, buff_action_pause_seconds=dict.fromkeys(order, 0)), survival=self.policy
        )
        with patch("diablo2.common.supervised_field.wait_readable_resources", return_value=None), patch(
            "diablo2.tools.field_input_probe.time.monotonic", side_effect=clock or time.monotonic
        ):
            report = dispatch_buffs(capture, observer, driver, lambda: True, profile, threading.Event(), trace_frames=trace)
        return report, driver.events, capture

    def test_buff_trace_preserves_duplicate_steps_without_claiming_effect(self):
        trace = []
        order = ("w", "a", "a", "s", "d", "f1", "w")
        report, events, capture = self.buff_trace(order, trace)
        self.assertEqual(list(order), [row["key"] for _, row in trace])
        self.assertEqual(list(range(7)), [row["index"] for _, row in trace])
        self.assertEqual(7, capture.grab.call_count)
        self.assertEqual(7, len(events))
        self.assertEqual(1580, trace[0][1]["hud"]["life"]["maximum"])
        self.assertFalse(report["effect_verified"])

    def test_slow_buff_trace_stops_before_the_next_cast(self):
        trace = []
        # began, deadline guards, acquired, processed: the first trace takes 300ms.
        report, events, _ = self.buff_trace(("a", "s"), trace, clock=iter((0, 0, 0, 0, 0, 0, 0.3)).__next__)
        self.assertEqual("trace_screen_stale", report["status"])
        self.assertEqual(["a"], [row["key"] for row in report["buff_dispatches"]])
        self.assertEqual(1, len(events))
        self.assertEqual(1, len(trace))

    def test_buff_trace_rejects_overflow_before_sending_input(self):
        for order, trace in ((("a",) * 17, []), (("a",), [(None, {})])):
            with self.assertRaises(ValueError):
                self.buff_trace(order, trace)

    def test_partial_weapon_swap_does_not_send_an_unverified_restore_key(self):
        trace = []
        report, events, _ = self.buff_trace(("w", "a"), trace, clock=iter((0, 0, 0, 0, 0, 0, 0.3)).__next__)
        self.assertEqual("trace_screen_stale", report["status"])
        self.assertFalse(report["equipment_dispatch_balanced"])
        self.assertEqual([("w",)], [row["args"] for row in events])

    def test_initial_effect_proof_requires_fresh_post_sequence_full_confirmation(self):
        proof = dict(
            character_id="flash",
            source="supervised_visual",
            observed_at=105,
            effect_verified=True,
            equipment_restored=True,
            sequence_effects_verified=True,
        )
        confirm_initial_probe(proof, "flash", 104, 110)
        for changes in (
            dict(observed_at=103),
            dict(observed_at=111),
            dict(observed_at=94),
            dict(sequence_effects_verified=False),
            dict(source="key_dispatch"),
        ):
            with self.assertRaises(ValueError):
                confirm_initial_probe(dict(proof, **changes), "flash", 104, 110)

    def test_timer_monitors_until_cast_plus_140_without_sending_input(self):
        now = [10.0]
        stop = Mock()
        stop.is_set.return_value = False
        stop.wait.side_effect = lambda seconds: now.__setitem__(0, now[0] + seconds)
        progress = []
        with patch("diablo2.tools.field_input_probe.wait_readable_resources", return_value=None) as read:
            wait_renewal_deadline(
                150,
                capture=Mock(),
                hud=Mock(),
                focused=lambda: True,
                policy=self.policy,
                stop=stop,
                clock=lambda: now[0],
                progress=progress.append,
            )
        self.assertEqual(150, now[0])
        self.assertGreater(read.call_count, 1)
        self.assertLessEqual(progress[-1], 10)

    def test_prepared_renewal_still_rejects_stale_safety(self):
        signals = queue.Queue()
        signals.put(dict(self.permit, phase="renewal_safety", observed_at=100))
        with patch("diablo2.common.supervised_field.wait_readable_resources", return_value=None), self.assertRaises(ValueError):
            wait_probe_signal(
                signals,
                capture=Mock(),
                hud=Mock(),
                focused=lambda: True,
                policy=self.policy,
                stop=threading.Event(),
                deadline=200,
                phase="renewal_safety",
                accept=lambda value: validate_permit(value, "flash", self.policy, 116, buff=True),
                clock=lambda: 150,
            )

    def test_confirmation_wait_retries_only_unverified_hud_without_consuming_proof(self):
        for blocked, retry, succeeds in (
            ("observation_unverified", True, True),
            ("resource_low", True, False),
            ("focus_lost", True, False),
            ("screen_stale", True, False),
            ("observation_unverified", False, False),
        ):
            signals = queue.Queue()
            signals.put(dict(phase="field_safety"))
            accepted = Mock()
            with patch("diablo2.common.supervised_field.wait_readable_resources", side_effect=[blocked, None]) as read:

                def run():
                    return wait_probe_signal(
                        signals,
                        capture=Mock(),
                        hud=Mock(),
                        focused=lambda: True,
                        policy=self.policy,
                        stop=threading.Event(),
                        deadline=200,
                        phase="field_safety",
                        accept=accepted,
                        clock=lambda: 150,
                        retry_unverified=retry,
                    )

                if succeeds:
                    self.assertEqual("field_safety", run()["phase"])
                    self.assertEqual(2, read.call_count)
                    accepted.assert_called_once()
                else:
                    with self.assertRaisesRegex(RuntimeError, blocked):
                        run()
                    accepted.assert_not_called()
                    self.assertEqual(1, signals.qsize())

    def test_focus_loss_cancels_timer_monitor_before_any_signal_is_used(self):
        with patch("diablo2.tools.field_input_probe.wait_readable_resources", return_value="focus_lost"), self.assertRaisesRegex(
            RuntimeError, "focus_lost"
        ):
            wait_renewal_deadline(
                150, capture=Mock(), hud=Mock(), focused=lambda: False, policy=self.policy, stop=threading.Event(), clock=lambda: 10
            )

    def test_step_plan_requires_new_ground_floor_and_fresh_processing(self):
        ground, path = Mock(), Mock()
        ground.observe.return_value = SimpleNamespace(point=(960, 578), reason="verified")
        path.choose.return_value = SimpleNamespace(aim=(1060, 528), reason="connected_floor")
        choice, reading, _ = plan_arcane_step(None, "north", ground, path, 100, clock=lambda: 100.2)
        self.assertEqual((1060, 528), choice.aim)
        self.assertEqual((960, 578), reading.point)
        with self.assertRaisesRegex(RuntimeError, "stale"):
            plan_arcane_step(None, "north", ground, path, 100, clock=lambda: 100.25)
        ground.observe.return_value = SimpleNamespace(point=None, reason="ground_ambiguous")
        with self.assertRaisesRegex(RuntimeError, "ground_ambiguous"):
            plan_arcane_step(None, "north", ground, path, 100, clock=lambda: 100.2)
        ground.observe.return_value = SimpleNamespace(point=(960, 578), reason="verified")
        path.choose.return_value = SimpleNamespace(aim=None, reason="no_observed_progress")
        with self.assertRaisesRegex(RuntimeError, "no_observed_progress"):
            plan_arcane_step(None, "north", ground, path, 100, clock=lambda: 100.2)

    def test_pointer_latency_cannot_dispatch_a_stale_step(self):
        backend, stop = Mock(), threading.Event()
        diagnostics = {}
        self.assertFalse(
            dispatch_planned_step(
                backend, lambda: True, stop, (100, 100), "f2", 100, clock=iter((100.1, 100.25)).__next__, diagnostics=diagnostics
            )
        )
        self.assertEqual("frame_expired", diagnostics["reason"])
        self.assertEqual("before_cast", diagnostics["stage"])
        self.assertEqual(0.25, diagnostics["frame_age_seconds"])
        backend.move.assert_called_once_with(100, 100)
        backend.press.assert_not_called()
        self.assertTrue(dispatch_planned_step(backend, lambda: True, stop, (100, 100), "f2", 100, clock=lambda: 100.2))
        backend.press.assert_called_once_with("f2")

    def test_new_hover_check_can_deny_cast_and_cannot_extend_original_frame_deadline(self):
        for accepted, times in ((False, (100.1, 100.2)), (True, (100.1, 100.2, 100.25))):
            backend, check = Mock(), Mock(return_value=accepted)
            diagnostics = {}
            self.assertFalse(
                dispatch_planned_step(
                    backend,
                    lambda: True,
                    threading.Event(),
                    (100, 100),
                    "f2",
                    100,
                    clock=iter(times).__next__,
                    verify_aim=check,
                    diagnostics=diagnostics,
                )
            )
            check.assert_called_once_with()
            backend.move.assert_called_once_with(100, 100)
            backend.press.assert_not_called()
            self.assertEqual("frame_expired" if accepted else "pointer_mismatch", diagnostics["reason"])

    def test_large_hub_departure_requires_current_waypoint_near_current_ground(self):
        settings = PathSettings(12, 0.5, 40, 120, 120, 30, 4)
        observed = []

        class Planner:
            teleport = None

            def __init__(self):
                self.settings = settings

            def observe(self, *args, **kwargs):
                observed.append(self.settings)
                return object()

            def choose(self, *args, **kwargs):
                return SimpleNamespace(aim=(1060, 528))

        path, ground = Planner(), Mock()
        ground.observe.return_value = GroundReading((960, 578), "verified")
        for hub in (None, (1181, 578), (1180, 578)):
            plan_arcane_step(None, "north", ground, path, 100, hub_point=hub, clock=lambda: 100.1)
        self.assertEqual([120, 120, 240], [item.maximum_step for item in observed])
        self.assertEqual(120, path.settings.maximum_step)

    def test_segment_expiry_denies_even_a_fresh_frame_after_hover_check(self):
        backend, check = Mock(), Mock(return_value=True)
        diagnostics = {}
        self.assertFalse(
            dispatch_planned_step(
                backend,
                lambda: True,
                threading.Event(),
                (100, 100),
                "f2",
                111.9,
                clock=iter((111.9, 111.95, 112.0)).__next__,
                verify_aim=check,
                expires_at=112,
                diagnostics=diagnostics,
            )
        )
        check.assert_called_once_with()
        backend.move.assert_called_once_with(100, 100)
        backend.press.assert_not_called()
        self.assertEqual("deadline_expired", diagnostics["reason"])
        self.assertEqual("after_pointer_verification", diagnostics["stage"])
        self.assertEqual(112, diagnostics["checked_at"])

    def test_stop_during_game_pointer_settle_cannot_send_cast(self):
        backend, stop = Mock(), Mock()
        stop.is_set.side_effect = [False, True]
        diagnostics = {}
        self.assertFalse(
            dispatch_planned_step(backend, lambda: True, stop, (100, 100), "f2", 100, clock=lambda: 100.1, diagnostics=diagnostics)
        )
        backend.move.assert_called_once_with(100, 100)
        stop.wait.assert_called_once_with(0.05)
        backend.press.assert_not_called()
        self.assertEqual("stop_requested", diagnostics["reason"])
        self.assertIsNone(diagnostics["checked_at"])

    def test_dispatch_diagnostics_preserve_focus_and_invalid_time_refusals(self):
        for focused, captured_at, expires_at, reason in (
            (False, 100, None, "focus_lost"),
            (True, 101, None, "frame_time_reversed"),
            (True, float("nan"), None, "frame_expired"),
            (True, 100, float("nan"), "deadline_expired"),
        ):
            backend, diagnostics = Mock(), {}
            self.assertFalse(
                dispatch_planned_step(
                    backend,
                    lambda: focused,
                    threading.Event(),
                    (100, 100),
                    "f2",
                    captured_at,
                    clock=lambda: 100.1,
                    expires_at=expires_at,
                    diagnostics=diagnostics,
                )
            )
            self.assertEqual(reason, diagnostics["reason"])
            self.assertEqual("before_pointer", diagnostics["stage"])
            backend.move.assert_not_called()
            backend.press.assert_not_called()

    def landing_probe(self, delta, *, position=(960, 578), slow=False, transient_ground=False, expires_at=None):
        now = [100.0]
        capture, hud, ground, path, motion, stop = (Mock() for _ in range(6))
        capture.grab.return_value = SimpleNamespace(frame=None)
        hud.observe.return_value = self.reading
        if slow:
            hud.observe.side_effect = lambda frame: (now.__setitem__(0, now[0] + 0.26), self.reading)[1]
        ground.observe.return_value = GroundReading(position, "test")
        if transient_ground:
            calls = [0]

            def locate(frame, *, captured_at):
                calls[0] += 1
                return GroundReading(position if calls[0] == 8 else None, "test")

            ground.observe.side_effect = locate
        stop.is_set.return_value = False
        stop.wait.side_effect = lambda seconds: now.__setitem__(0, now[0] + 0.05)
        count = [0]

        def measure(frame, view, *, overlap_hint=None):
            count[0] += 1
            return CameraMotion(None if count[0] == 1 else delta if count[0] == 2 else (0, 0), 20, "test_translation")

        motion.observe.side_effect = measure
        view = SimpleNamespace(anchor=(960, 578), viewport=(600, 358, 1320, 798), generation=0)
        with patch("diablo2.runs.summoner.navigation.BackgroundMotionObserver", return_value=motion):
            return observe_step_landing(
                capture,
                hud,
                lambda: True,
                self.policy,
                stop,
                None,
                view,
                SimpleNamespace(aim=(1060, 528)),
                ground,
                path,
                reference_seconds=0.32,
                clock=lambda: now[0],
                expires_at=expires_at,
            )

    def test_landing_needs_aligned_motion_then_settled_new_ground_after_reference(self):
        result = self.landing_probe((80, -40))
        self.assertEqual("arrival_observed", result["status"])
        self.assertGreaterEqual(result["observations"][-1]["elapsed"], 0.32)
        self.assertEqual("arrival_unverified", self.landing_probe((0, 0))["status"])
        self.assertEqual("arrival_unverified", self.landing_probe((-80, 40))["status"])
        self.assertEqual("arrival_unverified", self.landing_probe((80, -40), position=None)["status"])

    def test_landing_processing_overrun_cannot_be_confirmed_by_motion(self):
        self.assertEqual("screen_stale", self.landing_probe((80, -40), slow=True)["status"])

    def test_landing_cannot_report_arrival_after_the_whole_segment_expires(self):
        self.assertEqual("segment_time_limit", self.landing_probe((80, -40), expires_at=100.34)["status"])

    def test_current_ground_can_confirm_a_camera_already_observed_stable(self):
        result = self.landing_probe((80, -40), transient_ground=True)
        self.assertEqual("arrival_observed", result["status"])
        self.assertIsNotNone(result["observations"][-1]["ground"])
        self.assertIsNone(result["observations"][-2]["ground"])


if __name__ == "__main__":
    unittest.main()
