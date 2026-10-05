import threading
import unittest
from unittest.mock import Mock, patch
from queue import Queue

import numpy as np

from diablo2.actions.run_lifecycle import RunLifecycleSession
from diablo2.common.capture import ScreenCapture, SessionRecorder
from diablo2.actions.town_loop_vision import TownLoopPlanner, TownLoopVision


class SceneVision:
    def __init__(self):
        self.names = set()
        self.templates = {}

    def normalize(self, frame):
        return frame

    def find(self, frame, name):
        return {"x": 100, "y": 100, "score": 1.0} if name in self.names else None


class TownLoopContractTests(unittest.TestCase):
    def setUp(self):
        self.vision = SceneVision()
        self.planner = TownLoopPlanner(self.vision)

    def step(self, now, *names):
        self.vision.names = set(names)
        return self.planner.step(None, now)

    def test_unknown_initial_screen_never_sends_input_and_times_out(self):
        self.assertIsNone(self.step(0, "hud"))
        self.assertIsNone(self.step(20, "hud"))
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            self.step(26, "hud")

    def test_wrong_character_and_difficulty_overlay_cannot_start_room(self):
        self.assertIsNone(self.step(0, "play"))
        self.assertIsNone(self.step(1, "play", "flash", "difficulty_header"))
        self.assertEqual(self.step(2, "play", "flash")["phase"], "difficulty")

    def test_quick_loading_can_be_skipped_but_wrong_arrival_cannot(self):
        self.step(0, "play", "flash")
        self.step(1, "difficulty_header", "hell")
        # 도착의 긍정 증거가 있으면 로딩 프레임을 놓쳐도 진행할 수 있다.
        action = self.step(5, "hud", "area_act1", "world_act1_waypoint")
        self.assertEqual(action["phase"], "wait_act1_panel")
        self.step(7, "waypoint_header", "list_act1")
        self.assertIsNone(self.step(8, "waypoint_header", "list_act1"))
        self.step(9, "waypoint_header", "list_act2")
        self.assertIsNone(self.step(11, "hud", "area_act1"))
        self.assertIsNone(self.step(12, "hud", "area_act2"))
        self.assertEqual(self.step(13, "hud", "area_act2", "world_act2_waypoint")["phase"], "return_panel")

    def test_exit_confirmation_counts_once_and_done_never_creates_extra_room(self):
        self.planner.data.update(phase="after_exit", started=0, deadline=30)
        self.assertIsNone(self.step(1))
        self.assertEqual(self.planner.data["completed"], 0)
        self.assertIsNone(self.step(2, "play", "flash"))
        self.assertEqual(self.planner.data["completed"], 1)
        self.assertEqual(self.planner.data["phase"], "done")
        self.assertIsNone(self.step(3, "play", "flash"))
        self.assertEqual(self.planner.data["completed"], 1)

    def test_return_waypoint_retries_require_fresh_town_evidence_and_are_bounded(self):
        self.planner.data.update(phase="destination_arrival", started=0, deadline=30)
        action = self.step(1, "hud", "area_act2", "world_act2_waypoint")
        self.assertEqual(action["phase"], "return_panel")
        self.assertIsNone(self.step(1.2, "hud", "area_act2", "world_act2_waypoint"))
        self.assertIsNone(self.step(2, "hud", "world_act2_waypoint"))
        self.assertIsNone(self.step(3, "hud", "area_act1", "world_act2_waypoint"))
        self.assertEqual(self.step(4, "hud", "area_act2", "world_act2_waypoint")["kind"], "click")
        self.assertEqual(self.step(5, "hud", "area_act2", "world_act2_waypoint")["kind"], "click")
        with self.assertRaisesRegex(RuntimeError, "three observed"):
            self.step(6, "hud", "area_act2", "world_act2_waypoint")

    def test_finite_repeat_and_uncalibrated_arcane_required(self):
        for count in (None, 0, 4, True):
            with self.assertRaises(ValueError):
                TownLoopPlanner(self.vision, count)
        with self.assertRaisesRegex(ValueError, "not been calibrated"):
            TownLoopPlanner(self.vision, destination="arcane")

    def test_town_probes_are_bounded(self):
        self.planner.data.update(phase="act1_waypoint", started=0, deadline=30)
        self.assertIsNotNone(self.step(0, "hud", "area_act1"))
        self.assertEqual(self.step(3, "hud", "area_act1")["kind"], "release")
        self.assertIsNotNone(self.step(4, "hud", "area_act1"))
        self.assertEqual(self.step(7, "hud", "area_act1")["kind"], "release")
        self.assertIsNotNone(self.step(8, "hud", "area_act1"))
        self.step(10, "hud", "area_act1")
        with self.assertRaisesRegex(RuntimeError, "three bounded"):
            self.step(11, "hud", "area_act1")

    def test_visible_waypoint_releases_hold_before_reacquired_click(self):
        self.planner.data.update(phase="act1_waypoint", started=0, deadline=30)
        self.assertEqual(self.step(0, "hud", "area_act1")["kind"], "hold")
        self.assertIsNone(self.step(0.1, "hud", "area_act1"))
        self.assertEqual(self.step(0.2, "hud", "area_act1", "world_act1_waypoint")["kind"], "release")
        self.assertEqual(self.step(0.3, "hud", "area_act1", "world_act1_waypoint")["kind"], "click")

    def test_npc_dialogue_releases_hold_then_closes_menu_without_purchase(self):
        self.planner.data.update(phase="act1_waypoint", started=0, deadline=30)
        self.step(0, "hud", "area_act1")
        self.assertEqual(self.step(0.2, "hud", "area_act1", "npc_cancel")["kind"], "release")
        action = self.step(0.3, "hud", "area_act1", "npc_cancel")
        self.assertEqual(action["key"], "esc")
        self.assertEqual(self.planner.data["npc_recoveries"], 1)

    def test_release_is_allowed_after_focus_loss_or_stop(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session._town_mouse_held = True
        session._ensure_input_allowed = Mock(side_effect=RuntimeError("lost focus"))
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as input_mock:
            session._dispatch_town_action(None, None, {"kind": "release"}, None)
            input_mock.mouseUp.assert_called_once_with(button="left", _pause=False)
            self.assertFalse(session._town_mouse_held)

    def test_bad_window_shape_is_rejected(self):
        vision = TownLoopVision.__new__(TownLoopVision)
        for shape in ((100, 100, 3), (900, 900, 3)):
            with self.assertRaisesRegex(RuntimeError, "Uncalibrated"):
                vision.normalize(np.zeros(shape, dtype=np.uint8))

    def test_stop_during_settle_cancels_the_pending_click(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session._stop_event = threading.Event()
        session._last_pointer = None
        session._town_guard = None
        session._relative_point = Mock(return_value=(100, 100))
        session._move_absolute = Mock()
        session._sleep_range = Mock(side_effect=lambda *args: session._stop_event.set())
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as input_mock:
            with self.assertRaisesRegex(RuntimeError, "stopped"):
                session._click_relative(Mock(), (100, 100))
            input_mock.click.assert_not_called()

    def test_template_larger_than_capture_is_not_an_opencv_crash(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        self.assertIsNone(session._locate_template(np.zeros((10, 10, 3), dtype=np.uint8), np.zeros((20, 20, 3), dtype=np.uint8), 0.8))

    def test_game_cursor_shift_requires_expected_displacement_and_menu(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session._last_pointer = (1000, 100)
        session.events = Mock()
        self.planner.data["phase"] = "wait_act1_panel"
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as input_mock:
            input_mock.position.return_value = (1360, 100)
            session._accept_waypoint_cursor_shift(None, self.planner, 1920)
            self.assertEqual(session._last_pointer, (1000, 100))
            self.vision.names = {"waypoint_header", "list_act1"}
            input_mock.position.return_value = (1400, 100)
            session._accept_waypoint_cursor_shift(None, self.planner, 1920)
            self.assertEqual(session._last_pointer, (1000, 100))
            input_mock.position.return_value = (1360, 100)
            session._accept_waypoint_cursor_shift(None, self.planner, 1920)
            self.assertEqual(session._last_pointer, (1360, 100))

    def test_cleanup_error_does_not_leave_session_running(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session.capture_config = Mock()
        session._lock = threading.Lock()
        session.events = Queue()
        session._is_running = True
        session._town_loop = True
        session._bind_hotkey = Mock()
        session._focus_game_window = Mock()
        session._run_town_loop = Mock()
        session._release_town_mouse = Mock()
        session._unbind_hotkey = Mock()
        session.last_result = "passed"
        with patch("diablo2.actions.run_lifecycle.ScreenCapture") as capture_factory:
            capture_factory.return_value.close.side_effect = RuntimeError("cleanup fault")
            session._run()
        self.assertFalse(session.is_running)
        self.assertEqual(session.last_result, "failed")
        self.assertIn("cleanup failed", session.events.get().message)
        session._unbind_hotkey.assert_called_once()

    def test_capture_and_recorder_close_their_own_resources(self):
        capture = ScreenCapture.__new__(ScreenCapture)
        capture._sct = Mock()
        capture.close()
        capture._sct.close.assert_called_once()
        recorder = SessionRecorder(Mock(), (1920, 1080))
        recorder.close()


if __name__ == "__main__":
    unittest.main()
