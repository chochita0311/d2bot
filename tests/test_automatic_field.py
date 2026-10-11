import math
import threading
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np

from diablo2.common.automatic_field import AutomaticBuffDriver, CooperativeStop, validate_runtime_permit
from diablo2.common.survival_vision import HudReading, ResourceReading
from diablo2.runs.summoner.camera_ground import CalibratedCameraGroundObserver
from diablo2.runs.summoner.scene_vision import GroundReading
from diablo2.runs.summoner.main_run import maintain_paused_field
from diablo2.common.field_control import DryFieldInput


class AutomaticFieldTests(unittest.TestCase):
    def test_paused_guard_renews_from_callback_without_supervisor_wait(self):
        now, stop = [0.0], Mock(wraps=threading.Event())
        capture, hud = Mock(), Mock()
        capture.grab.return_value = SimpleNamespace(frame=object())
        hud.observe.return_value = SimpleNamespace(life=ResourceReading(9, 10), mana=ResourceReading(8, 10), belt_visibility="closed")
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))

        def wait(seconds):
            now[0] += 0.1
            stop.set()

        stop.wait.side_effect = wait
        renew = Mock(return_value=140)
        maintain_paused_field(
            capture,
            hud,
            lambda: True,
            profile,
            stop,
            report={},
            evidence=[],
            clock=lambda: now[0],
            engage_monsters=False,
            buff_expires_at=0,
            renew_buffs=renew,
        )
        renew.assert_called_once_with()
        capture.grab.assert_called_once_with()

    def test_paused_loot_requires_actual_attack_dispatch_and_releases_first(self):
        for dispatched in (False, True):
            with self.subTest(dispatched=dispatched):
                now, loops, stop = [0.0], [0], Mock(wraps=threading.Event())
                capture, hud, hazards, loot = Mock(), Mock(), Mock(), Mock()
                capture.grab.return_value = SimpleNamespace(frame=object())
                hud.observe.return_value = SimpleNamespace(
                    life=ResourceReading(9, 10), mana=ResourceReading(8, 10), belt_visibility="closed"
                )
                hazards.observe.side_effect = lambda *args: (SimpleNamespace(ground=(980, 580)),) if loops[0] == 0 else ()
                ground = SimpleNamespace(observe=lambda *args, **kwargs: GroundReading((960, 580), "test"))
                backend = DryFieldInput()
                profile = SimpleNamespace(
                    survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
                    actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
                )

                def wait(seconds):
                    now[0] += 0.1
                    loops[0] += 1
                    if loops[0] == 2:
                        stop.set()

                def scan(**kwargs):
                    self.assertEqual(("up", "f4"), backend.events[-1])

                stop.wait.side_effect = wait
                loot.after_combat.side_effect = scan
                replacement = {} if dispatched else {"return_value": False}
                target = "diablo2.common.field_control.HeldFieldInput.apply"
                if dispatched:
                    from diablo2.common.field_control import HeldFieldInput

                    replacement = {"autospec": True, "side_effect": HeldFieldInput.apply}
                with patch(target, **replacement):
                    maintain_paused_field(
                        capture,
                        hud,
                        lambda: True,
                        profile,
                        stop,
                        report={},
                        evidence=[],
                        clock=lambda: now[0],
                        backend=backend,
                        ground=ground,
                        hazards=hazards,
                        buff_expires_at=100,
                        loot_controller=loot,
                    )
                self.assertEqual(int(dispatched), loot.after_combat.call_count)

    def driver(self, *, unsafe=False, status="dispatched_unconfirmed", balanced=True):
        self.now = 10.0
        self.events = []
        self.guards = []
        profile = SimpleNamespace(
            actions=SimpleNamespace(pre_run_buff_order=("w", "a", "a", "s", "d", "f1", "w")),
            survival=SimpleNamespace(buffs=[SimpleNamespace(name="battle_orders", key="s", duration_seconds=150, renew_before_seconds=10)]),
        )

        def guard_factory():
            guard = Mock()
            guard.prepare.return_value = {"window_seconds": 1.0}
            if unsafe and not self.guards:
                guard.prepare.side_effect = RuntimeError("buff_nearby_hostile")
            self.guards.append(guard)
            return guard

        def dispatch(check):
            self.events.append("dispatch")
            self.now += 3
            return dict(
                status=status,
                equipment_dispatch_balanced=balanced,
                buff_dispatches=[dict(key="s", cast_request_monotonic=self.now)],
                effect_verified=False,
            )

        return AutomaticBuffDriver(
            profile,
            guard_factory,
            dispatch,
            lambda: {"source": "runtime_screen"},
            lambda: self.events.append("clear"),
            threading.Event(),
            clock=lambda: self.now,
        )

    def test_140_seconds_uses_actual_s_request_not_review_time(self):
        driver = self.driver()
        result = driver.execute()
        self.assertEqual([153], result["deadlines"])
        self.assertEqual("not_independently_verified", result["effect_status"])
        self.assertFalse(result["buff"]["effect_verified"])
        self.now = 152.999
        self.assertFalse(driver.due())
        self.now = 153
        self.assertTrue(driver.due())
        renewed = driver.execute()
        self.assertEqual([296], renewed["deadlines"])
        self.assertEqual(["dispatch", "dispatch"], self.events)

    def test_nearby_hostile_clear_then_new_stability_check(self):
        driver = self.driver(unsafe=True)
        driver.execute()
        self.assertEqual(["clear", "dispatch"], self.events)
        self.assertEqual(2, len(self.guards))

    def test_partial_sequence_never_grants_new_deadline(self):
        for status, balanced in (("interrupted", True), ("dispatched_unconfirmed", False)):
            driver = self.driver(status=status, balanced=balanced)
            with self.assertRaises(RuntimeError):
                driver.execute()
            self.assertEqual([], driver.deadlines)

    def test_invalid_sequence_and_cancel_never_dispatch(self):
        driver = self.driver()
        driver.profile.actions.pre_run_buff_order = ("w", "s")
        with self.assertRaises(ValueError):
            driver.execute()
        self.assertEqual([], self.events)
        driver = self.driver()
        driver.stop.set()
        with self.assertRaises(RuntimeError):
            driver.execute()
        self.assertEqual([], self.events)

    def test_stop_local_cleanup_does_not_cancel_next_room(self):
        external = threading.Event()
        room = CooperativeStop(external)
        room.set()
        self.assertTrue(room.is_set())
        self.assertFalse(external.is_set())
        next_room = CooperativeStop(external)
        self.assertFalse(next_room.is_set())
        next_room.request_stop()
        self.assertTrue(external.is_set())
        self.assertTrue(CooperativeStop(external).wait(1))

    def test_runtime_permit_requires_current_explicit_scope(self):
        proof = dict(
            source="runtime_screen",
            area="arcane_sanctuary",
            panels_closed=True,
            belt_closed=True,
            nearby_candidates=0,
            safety_scope="current_nearby_candidates_and_stable_ratios",
            observed_at=10,
        )
        validate_runtime_permit(proof, 10.499)
        for changes in (
            {"observed_at": 9.5},
            {"observed_at": 11},
            {"observed_at": math.nan},
            {"panels_closed": False},
            {"nearby_candidates": 1},
            {"nearby_candidates": False},
            {"source": "supervised_visual"},
        ):
            with self.assertRaises(ValueError):
                validate_runtime_permit(dict(proof, **changes), 10)

    def test_shared_camera_runtime_needs_fresh_closed_viewport(self):
        from pathlib import Path

        frame = np.zeros((1140, 1922, 3), np.uint8)
        hud = HudReading("test", "1", ResourceReading(8, 10), ResourceReading(8, 10), "read", belt_visibility="closed")
        ground = Mock()
        ground.observe.return_value = GroundReading(None, "ground_unverified")
        layout = Path(__file__).resolve().parents[1] / "assets/ui/layouts/remastered-1922x1140/camera-ground.json"
        camera = CalibratedCameraGroundObserver(ground, lambda _: False, persist_reviewed_layout=True)
        for at, focused in ((9, True), (10, False)):
            with self.assertRaises(ValueError):
                camera.confirm_runtime_viewport(frame, hud, focused, at, 10, reviewed_layout=layout)
        camera.confirm_runtime_viewport(frame, hud, True, 10, 10.1, reviewed_layout=layout)
        self.assertIsNotNone(camera.observe(frame, captured_at=10).point)


if __name__ == "__main__":
    unittest.main()
