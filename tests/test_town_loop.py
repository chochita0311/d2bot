import threading
import unittest
from unittest.mock import Mock, patch
from queue import Queue

import numpy as np
import cv2 as cv

from diablo2.actions.run_lifecycle import RunLifecycleSession
from diablo2.common.capture import ScreenCapture, SessionRecorder
from diablo2.actions.town_loop_vision import TownLoopPlanner, TownLoopVision
from diablo2.common.scene_motion import CameraMotion


class SceneVision:
    def __init__(self):
        self.names = set()
        self.templates = {}

    def normalize(self, frame):
        return frame

    def reset_camera(self):
        pass

    def camera_stable(self, frame, now):
        return True

    def find(self, frame, name):
        return {"x": 100, "y": 100, "score": 1.0} if name in self.names else None


class TownLoopContractTests(unittest.TestCase):
    def setUp(self):
        self.vision = SceneVision()
        self.planner = TownLoopPlanner(self.vision)

    def step(self, now, *names):
        self.vision.names = set(names)
        return self.planner.step(None, now)

    def test_waypoint_blue_pair_rejects_flat_chat_shapes_and_multiple_pairs(self):
        vision = TownLoopVision.__new__(TownLoopVision)
        definition = dict(
            search=[0, 0, 500, 200],
            hsv_lower=[95, 55, 120],
            hsv_upper=[135, 255, 255],
            minimum_area=200,
            width=[6, 55],
            height=[25, 100],
            separation=[115, 155],
            vertical_difference=20,
            click_offset=[0, 25],
            maximum_aspect_ratio=1.35,
        )
        vision.color = np.zeros((200, 500, 3), np.uint8)
        for x in (30, 160):
            cv.rectangle(vision.color, (x, 40), (x + 37, 53), (255, 100, 0), -1)
        self.assertIsNone(vision.find_blue_pair(definition))
        for x in (30, 160):
            cv.rectangle(vision.color, (x, 80), (x + 35, 120), (255, 100, 0), -1)
        self.assertIsNotNone(vision.find_blue_pair(definition))
        for x in (310, 440):
            cv.rectangle(vision.color, (x, 80), (x + 35, 120), (255, 100, 0), -1)
        self.assertIsNone(vision.find_blue_pair(definition))

    def test_camera_rebases_unknown_correspondence_but_requires_new_stable_frames(self):
        vision = TownLoopVision.__new__(TownLoopVision)
        vision.motion = Mock()
        vision.reset_camera()
        vision.motion.observe_mask.side_effect = [
            CameraMotion(None, 4, "correspondence_insufficient"),
            CameraMotion(None, 0, "correspondence_unavailable"),
            CameraMotion((12, 0), 30, "background_translation"),
            CameraMotion((0.2, 0), 30, "background_translation"),
            CameraMotion((0.3, 0), 30, "background_translation"),
            CameraMotion((0.1, 0), 30, "background_translation"),
        ]
        frame = np.zeros((753, 1267, 3), np.uint8)
        self.assertFalse(vision.camera_stable(frame, 0))
        self.assertEqual(2, vision.motion.reset.call_count)
        self.assertFalse(vision.camera_stable(frame, 0.1))
        self.assertFalse(vision.camera_stable(frame, 0.2))
        self.assertFalse(vision.camera_stable(frame, 0.3))
        self.assertTrue(vision.camera_stable(frame, 0.4))

    def test_unknown_initial_screen_never_sends_input_and_times_out(self):
        self.assertIsNone(self.step(0, "hud"))
        self.assertIsNone(self.step(20, "hud"))
        with self.assertRaisesRegex(RuntimeError, "timed out"):
            self.step(26, "hud")

    def test_selected_character_uses_play_but_requires_closed_difficulty_overlay(self):
        self.assertIsNone(self.step(0))
        self.assertIsNone(self.step(1, "play", "difficulty_header"))
        self.assertEqual(self.step(2, "play")["phase"], "difficulty")

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

    def test_act2_reset_creates_one_nightmare_preparation_room_without_counting_finish(self):
        self.vision.templates = {"area_arcane": None, "world_arcane_waypoint": None}
        self.planner = TownLoopPlanner(self.vision, destination="arcane", arrival_only=True, reset_act2_start=True)
        self.planner.data.update(phase="act1_spawn", started=0, deadline=30)
        self.assertEqual("esc", self.step(0, "hud", "area_act2")["key"])
        self.assertEqual("click", self.step(0.2, "save_exit")["kind"])
        self.assertEqual("difficulty", self.step(0.4, "play", "flash")["phase"])
        self.assertEqual("nightmare", self.planner.data["difficulty"])
        self.step(0.6, "difficulty_header", "nightmare")
        self.assertEqual("esc", self.step(2.1, "hud", "area_act1")["key"])
        self.step(2.3, "save_exit")
        self.step(2.5, "play", "flash")
        self.assertEqual("hell", self.planner.data["difficulty"])
        self.assertEqual(1, self.planner.data["preparation_rooms"])
        self.assertEqual(0, self.planner.data["completed"])
        self.step(2.7, "difficulty_header", "hell")
        action = self.step(4.2, "hud", "area_act1", "world_act1_waypoint")
        self.assertEqual("wait_act1_panel", action["phase"])
        self.assertEqual(0, self.planner.data["completed"])

    def test_failed_difficulty_reset_cannot_restart_reset_or_move_in_act2(self):
        self.planner.data.update(phase="act1_spawn", started=0, deadline=30, reset_stage="restored_hell", difficulty_reset_used=True)
        with self.assertRaisesRegex(RuntimeError, "did not restore Act 1"):
            self.step(1, "hud", "area_act2")
        self.assertEqual(0, self.planner.data["action_number"])

    def test_waypoint_first_click_approaches_and_second_uses_new_position(self):
        self.planner.data.update(phase="act1_waypoint", started=0, deadline=30)
        self.assertEqual("click", self.step(0, "hud", "area_act1", "world_act1_waypoint")["kind"])
        self.assertIsNone(self.step(1, "hud", "area_act1", "world_act1_waypoint"))
        self.assertEqual("click", self.step(2.1, "hud", "area_act1", "world_act1_waypoint")["kind"])
        self.assertEqual(2, self.planner.data["waypoint_attempts"])
        self.assertEqual("act2_list", self.step(2.3, "waypoint_header", "list_act1")["phase"])

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

    def test_act2_initial_waypoint_uses_positive_current_object_and_no_blind_search(self):
        self.planner.data.update(phase="act1_spawn", started=0, deadline=30)
        with self.assertRaisesRegex(RuntimeError, "reviewed waypoint approach"):
            self.step(1, "hud", "area_act2")
        self.planner.data.update(phase="act1_spawn")
        action = self.step(2, "hud", "area_act2", "world_act2_waypoint")
        self.assertEqual((100, 100), tuple(action["point"]))
        self.assertEqual("act2_spawn", action["phase"])
        self.assertEqual(1, self.planner.data["waypoint_attempts"])

    def test_act2_spawn_probes_require_reviewed_layout_and_stop_on_current_waypoint(self):
        self.planner.data.update(phase="act1_spawn", started=0, deadline=30)
        action = self.step(1, "hud", "area_act2", "layout_act2_spawn")
        self.assertEqual("hold", action["kind"])
        self.assertEqual(1, self.planner.data["act2_probes"])
        self.assertEqual("release", self.step(1.1, "hud", "area_act2", "world_act2_waypoint")["kind"])
        self.assertEqual("click", self.step(1.3, "hud", "area_act2", "world_act2_waypoint")["kind"])
        self.assertEqual(1, self.planner.data["act2_probes"])

    def test_act2_spawn_probes_release_on_lost_town_or_npc_and_have_total_bound(self):
        self.planner.data.update(phase="act1_spawn", started=0, deadline=30)
        self.step(1, "hud", "area_act2", "layout_act2_spawn")
        self.assertEqual("release", self.step(1.1, "hud")["kind"])
        self.assertEqual("hold", self.step(1.3, "hud", "area_act2")["kind"])
        self.assertEqual("release", self.step(1.4, "hud", "area_act2", "npc_cancel")["kind"])
        self.assertEqual("hold", self.step(1.6, "hud", "area_act2")["kind"])
        self.assertEqual("release", self.step(4, "hud", "area_act2")["kind"])
        with self.assertRaisesRegex(RuntimeError, "three reviewed spawn"):
            self.step(4.2, "hud", "area_act2")

    def test_town_release_waits_for_observed_camera_and_unknown_motion_times_out(self):
        self.planner.data.update(phase="act1_spawn", started=0, deadline=30)
        self.step(1, "hud", "area_act2", "layout_act2_spawn")
        self.step(3, "hud", "area_act2")
        self.vision.camera_stable = Mock(return_value=False)
        self.assertIsNone(self.step(3.2, "hud", "area_act2", "world_act2_waypoint"))
        self.assertEqual(1, self.planner.data["act2_probes"])
        self.vision.camera_stable.return_value = True
        self.assertEqual("click", self.step(3.4, "hud", "area_act2", "world_act2_waypoint")["kind"])
        self.planner.data.update(camera_settling=True, camera_deadline=4)
        self.vision.camera_stable.return_value = False
        with self.assertRaisesRegex(RuntimeError, "did not settle"):
            self.step(4.2, "hud", "area_act2")

    def test_positive_palace_recovery_is_one_approach_not_unbounded_spawn_search(self):
        self.planner.data.update(phase="act1_spawn", started=0, deadline=30)
        action = self.step(1, "hud", "area_act2", "layout_act2_palace")
        self.assertEqual([900, 220], action["point"])
        self.assertTrue(self.planner.data["palace_approach_used"])
        self.assertEqual("release", self.step(3.5, "hud", "area_act2")["kind"])
        with self.assertRaisesRegex(RuntimeError, "reviewed waypoint approach"):
            self.step(3.7, "hud", "area_act2", "layout_act2_palace")

    def test_arcane_arrival_requires_current_waypoint_and_retries_without_npc_recovery(self):
        self.vision.templates = {"area_arcane": None, "world_arcane_waypoint": None}
        self.planner = TownLoopPlanner(self.vision, destination="arcane")
        self.planner.data.update(phase="destination_arrival", started=0, deadline=30)
        self.assertIsNone(self.step(0, "hud", "area_arcane"))
        self.assertIsNone(self.step(1, "hud", "area_act2", "world_arcane_waypoint"))
        action = self.step(2, "hud", "area_arcane", "world_arcane_waypoint")
        self.assertEqual(action["point"], [100, 100])
        self.assertEqual(action["phase"], "return_panel")
        self.assertIsNone(self.step(3, "hud", "world_arcane_waypoint"))
        action = self.step(4, "hud", "area_arcane", "world_arcane_waypoint", "npc_cancel")
        self.assertEqual(action["kind"], "click")
        self.assertEqual(self.planner.data["npc_recoveries"], 0)
        self.step(5, "hud", "area_arcane", "world_arcane_waypoint")
        with self.assertRaisesRegex(RuntimeError, "three observed"):
            self.step(6, "hud", "area_arcane", "world_arcane_waypoint")

    def test_arcane_positive_area_without_waypoint_calibration_cannot_start(self):
        self.vision.templates = {"area_arcane": None}
        with self.assertRaisesRegex(ValueError, "not been calibrated"):
            TownLoopPlanner(self.vision, destination="arcane")

    def test_arrival_only_stops_at_positive_hub_without_return_or_exit_inputs(self):
        self.vision.templates = {"area_arcane": None, "world_arcane_waypoint": None}
        self.planner = TownLoopPlanner(self.vision, destination="arcane", arrival_only=True)
        self.planner.data.update(phase="destination_arrival", started=0, deadline=30)
        self.assertIsNone(self.step(0, "hud", "area_arcane"))
        self.assertEqual(self.planner.data["completed"], 0)
        self.assertIsNone(self.step(1, "hud", "area_arcane", "world_arcane_waypoint"))
        self.assertEqual(self.planner.data["phase"], "done")
        self.assertEqual(self.planner.data["completed"], 1)
        self.assertIsNone(self.step(2, "play", "flash"))
        for count, destination in ((2, "arcane"), (1, "town")):
            with self.assertRaisesRegex(ValueError, "exactly one Arcane"):
                TownLoopPlanner(self.vision, count, destination=destination, arrival_only=True)

    def test_arrival_only_requires_guarded_planner_before_any_worker_starts(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        with self.assertRaisesRegex(ValueError, "supervised town planner"):
            session.start(1, arrival_only=True)

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

    def test_town_hover_wait_rechecks_stop_before_click(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session._stop_event = Mock()
        session._town_mouse_held = False
        session._move_absolute = Mock()
        session._relative_point = Mock(return_value=(100, 100))
        session._ensure_input_allowed = Mock(side_effect=[None, None, RuntimeError("stopped")])
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as input_mock:
            with self.assertRaisesRegex(RuntimeError, "stopped"):
                session._dispatch_town_action(
                    Mock(), np.zeros((753, 1267, 3), np.uint8), {"kind": "click", "point": [100, 100]}, (1267, 753)
                )
            session._stop_event.wait.assert_called_once_with(session.TOWN_HOVER_SETTLE_SECONDS)
            input_mock.click.assert_not_called()

    def test_template_larger_than_capture_is_not_an_opencv_crash(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        self.assertIsNone(session._locate_template(np.zeros((10, 10, 3), dtype=np.uint8), np.zeros((20, 20, 3), dtype=np.uint8), 0.8))

    def test_delayed_pointer_observation_waits_without_resending_or_clicking(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session._stop_event = Mock()
        session._stop_event.is_set.return_value = False
        session._town_guard = Mock()
        session._last_pointer = (0, 0)
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as backend, patch(
            "diablo2.actions.run_lifecycle.time.monotonic", side_effect=(0.0, 0.012)
        ):
            backend.position.side_effect = [(2772, 1098), (2787, 1097)]
            session._wait_for_town_pointer(2787, 1097)
            backend.SendInput.assert_not_called()
            backend.click.assert_not_called()
        self.assertEqual(session._last_pointer, (2787, 1097))
        self.assertEqual(session._town_guard.call_count, 2)

    def test_persistent_pointer_mismatch_and_stop_never_admit_click(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session._stop_event = Mock()
        session._stop_event.is_set.return_value = False
        session._town_guard = Mock()
        session._last_pointer = (0, 0)
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as backend, patch(
            "diablo2.actions.run_lifecycle.time.monotonic", side_effect=(0.0, 0.012, 0.081)
        ):
            backend.position.return_value = (2772, 1098)
            with self.assertRaisesRegex(RuntimeError, "Pointer mapping mismatch"):
                session._wait_for_town_pointer(2787, 1097)
            backend.click.assert_not_called()
        self.assertEqual(session._last_pointer, (0, 0))
        session._stop_event.is_set.return_value = True
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as backend:
            with self.assertRaisesRegex(RuntimeError, "stopped before"):
                session._wait_for_town_pointer(2787, 1097)
            backend.position.assert_not_called()

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
