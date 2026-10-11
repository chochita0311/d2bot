import unittest
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import PropertyMock, patch

import numpy as np

from diablo2.common.config import load_config
from diablo2.common.field_control import FieldLimits, FieldObservation
from diablo2.common.field_runtime import FieldControlRuntime, create_field_control
from diablo2.common.survival import ObservationStamp, SafetyObservation
from diablo2.common.survival_vision import HudReading, ResourceReading
from diablo2.runs.summoner.navigation import ArcanePathObserver, CameraMotion, PathSettings
from diablo2.runs.summoner.search import ArcaneSearch, SearchLimits
from diablo2.runs.summoner.search_runtime import ArcaneScene, ArcaneSearchRuntime


ROOT = Path(__file__).resolve().parents[1]


class Capture:
    def __init__(self):
        self.target = {"left": 300, "top": 200, "width": 200, "height": 200}
        self.current = self.target.copy()
        self.on_grab = lambda: None
        self.closed = False

    def current_target(self):
        return self.current.copy()

    def grab(self):
        self.on_grab()
        return SimpleNamespace(frame=np.full((200, 200, 3), (111, 109, 109), np.uint8), timestamp=999999)

    def close(self):
        self.closed = True


class SearchRuntimeTests(unittest.TestCase):
    def setUp(self):
        config = load_config(ROOT / "config")
        self.at, self.serial, self.focus = 1.0, 1, True
        self.loop = create_field_control(config, "flash", "room", FieldLimits(0.5, 1000, 3, 2, 1))
        self.loop.state.observe_belt(
            self.stamp(), {1: ("purple_potion", 4), 2: ("purple_potion", 4), 3: ("purple_potion", 4), 4: ("town_portal_scroll", 4)}
        )
        self.loop.state.confirm_buff(self.stamp(), "battle_orders", "field", True)
        self.field = FieldControlRuntime(self.loop, clock=lambda: self.at, focused=lambda: self.focus)
        self.backend = self.loop.inputs.backend
        self.capture = Capture()
        self.search = ArcaneSearch(
            ArcanePathObserver(PathSettings(4, 0.5, 20, 32, 40, 8, 8)),
            SearchLimits(0.5, 2, 20, 20, 100, 4, 8, 100, 2, settled_frames=2, settled_motion_pixels=2),
            reference_seconds=0.32,
            started_at=1,
        )
        self.enemy, self.terminal = "clear", None
        self.delta = (0.0, 0.0)
        self.reading = HudReading("test", "1", ResourceReading(100, 100), ResourceReading(100, 100), "unknown", belt_visibility="closed")
        self.on_hud = lambda: None
        self.driver = ArcaneSearchRuntime(
            self.search,
            self.field,
            self.capture,
            motion=SimpleNamespace(observe=lambda frame, view: CameraMotion(self.delta, 20, "test")),
            hud=SimpleNamespace(observe=self.hud),
            scene_reader=self.scene,
            next_stamp=self.stamp,
            viewport=(0, 0, 200, 200),
            anchor=(100, 100),
        )

    def stamp(self):
        self.serial += 1
        return ObservationStamp("flash", "room", self.serial, self.at)

    def scene(self, frame, stamp):
        return ArcaneScene(
            FieldObservation(SafetyObservation(stamp, None, None, "field", self.enemy == "clear"), True, self.enemy), terminal=self.terminal
        )

    def hud(self, frame):
        self.on_hud()
        return self.reading

    def tearDown(self):
        self.driver.close()

    def test_native_local_aim_is_translated_and_one_cast_waits_for_landing(self):
        action = self.driver.step()
        self.assertEqual(action.kind, "aim")
        self.assertEqual(self.backend.events[-1], ("move", action.aim[0] + 300, action.aim[1] + 200))
        self.at += 0.05
        self.assertEqual(self.driver.step().kind, "cast")
        self.assertEqual(self.backend.events[-1], ("press", "f2"))
        self.assertFalse(self.loop.inputs.held)
        self.at += 0.4
        self.assertEqual(self.driver.step().reason, "landing_pending")
        self.assertEqual(sum(event == ("press", "f2") for event in self.backend.events), 1)
        self.delta = (20, -10)
        self.at += 0.05
        self.assertEqual(self.driver.step().reason, "landing_pending")
        self.delta = (0, 0)
        self.at += 0.05
        self.assertEqual(self.driver.step().reason, "landing_pending")
        self.at += 0.05
        self.assertEqual(self.driver.step().kind, "aim")

    def test_unknown_enemy_and_belt_do_not_authorize_movement(self):
        self.enemy = "unknown"
        self.assertEqual(self.driver.step().kind, "hold")
        self.assertFalse(self.backend.events)
        self.enemy = "clear"
        self.reading = replace(self.reading, belt_visibility="unknown")
        self.at += 0.1
        self.assertEqual(self.driver.step().kind, "hold")
        self.assertFalse(self.backend.events)

    def test_enemy_combat_and_potion_preempt_route_and_boss_can_handoff_during_combat(self):
        self.enemy = "near"
        self.assertEqual(self.driver.step().kind, "hold")
        self.assertEqual(self.loop.inputs.held, {"f4"})
        self.terminal = "summoner_present"
        self.at += 0.1
        self.assertEqual(self.driver.step().kind, "summoner")
        self.assertFalse(self.loop.inputs.held)
        self.assertFalse(any(event == ("press", "f2") for event in self.backend.events))
        self.terminal = None
        self.search.phase = "outbound"
        self.reading = replace(self.reading, life=ResourceReading(1, 100))
        self.at += 0.1
        self.assertEqual(self.driver.step().kind, "hold")
        self.assertEqual(self.backend.events[-1], ("press", "1"))

    def test_capture_start_age_includes_hud_and_vision_time(self):
        self.on_hud = lambda: setattr(self, "at", self.at + 0.6)
        self.assertEqual(self.driver.step().reason, "shared_observation_rejected")
        self.assertFalse(self.backend.events)

    def test_window_move_during_capture_fails_without_input_and_closes(self):
        self.capture.on_grab = lambda: self.capture.current.update(left=301)
        with self.assertRaisesRegex(RuntimeError, "rectangle changed"):
            self.driver.step()
        self.assertTrue(self.capture.closed)
        self.assertTrue(self.loop.inputs.closed)
        self.assertFalse(self.backend.events)

    def test_window_move_after_observation_is_rechecked_before_dispatch(self):
        self.on_hud = lambda: self.capture.current.update(top=201)
        with self.assertRaisesRegex(RuntimeError, "rectangle changed"):
            self.driver.step()
        self.assertFalse(self.backend.events)

    def test_shared_gate_rejects_older_frame_due_timer_and_pause_after_move(self):
        self.driver.step()
        stamp = self.loop.last_stamp
        self.assertTrue(self.field.navigation_allowed(stamp))
        self.assertFalse(self.field.navigation_allowed(replace(stamp, sequence=0)))
        due = self.loop.state.next_buff_renewal_at
        self.at = due
        self.assertFalse(self.field.dispatch_navigation(stamp, aim=(400, 300), cast=True))
        self.at = stamp.observed_at
        original = self.backend.move

        def move(x, y):
            original(x, y)
            self.field.pause(True)

        self.backend.move = move
        self.assertFalse(self.field.dispatch_navigation(stamp, aim=(400, 300), cast=True))
        self.assertFalse(any(event == ("press", "f2") for event in self.backend.events))

    def test_scene_cannot_inject_held_movement_or_reuse_another_stamp(self):
        with self.assertRaisesRegex(ValueError, "owns movement"):
            ArcaneScene(replace(self.scene(None, self.stamp()).field, travel_aim=(500, 300)))
        for values in ({"terminal": "boss_dead"}, {"hub_ready": None}, {"hover": ("npc", (0, 0, 10, 10))}):
            with self.assertRaises(ValueError):
                ArcaneScene(self.scene(None, self.stamp()).field, **values)
        original = self.driver.scene_reader
        self.driver.scene_reader = lambda frame, stamp: original(frame, replace(stamp, room_id="other"))
        with self.assertRaisesRegex(ValueError, "exact stamp"):
            self.driver.step()
        self.assertFalse(self.backend.events)

    def test_non_dry_input_requires_independent_watchdog_before_observing(self):
        self.loop.inputs.backend = SimpleNamespace()
        with self.assertRaisesRegex(RuntimeError, "independent field watchdog"):
            self.driver.step()
        self.assertTrue(self.capture.closed)
        self.assertFalse(self.field.running)

    def test_live_route_cannot_fall_back_to_a_fixed_ground_anchor(self):
        self.loop.inputs.backend = SimpleNamespace(move=self.backend.move, press=self.backend.press, key_up=self.backend.key_up)
        with patch.object(FieldControlRuntime, "running", new_callable=PropertyMock, return_value=True):
            self.assertEqual("player_ground_unverified", self.driver.step().reason)
        self.assertFalse(self.backend.events)

    def test_new_scene_ground_is_used_instead_of_constructor_anchor(self):
        original = self.driver.scene_reader
        self.driver.scene_reader = lambda frame, stamp: replace(original(frame, stamp), player_ground=(120, 100))
        with patch.object(self.search.observer, "observe", wraps=self.search.observer.observe) as observe:
            self.driver.step()
        self.assertEqual((120, 100), observe.call_args[1]["anchor"])


if __name__ == "__main__":
    unittest.main()
