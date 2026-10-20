import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np

from diablo2.common.frame_geometry import FrameGeometry, ReferenceLayout, desktop_to_frame, frame_to_desktop, geometry_is_current
from diablo2.common.capture import ScreenCapture, WindowInfo, focus_window
from diablo2.common.config import CaptureConfig
from diablo2.common.field_control import DryFieldInput
from diablo2.common.hostile_probe import NearbyHostileProbe
from diablo2.actions.run_lifecycle import RunLifecycleSession
from diablo2.runs.summoner.main_run import exit_to_character
from diablo2.runs.summoner.scene_vision import ArcaneHazardObserver, ArcaneHoverObserver


class FrameGeometryTests(unittest.TestCase):
    def setUp(self):
        self.layout = ReferenceLayout((200, 140), (10, 30, 190, 130))
        self.target = dict(left=-300, top=140, width=126, height=84)
        self.client = (3, 12, 123, 79)
        self.geometry = FrameGeometry.create(self.target, self.layout, self.client)
        self.capture = SimpleNamespace(
            target=self.target,
            frame_geometry=self.geometry,
            current_target=lambda: dict(self.target),
            current_client_box=lambda: self.client,
        )

    def test_native_layout_keeps_pixels_and_negative_origin_exactly(self):
        target = dict(left=-1920, top=400, width=200, height=140)
        geometry = FrameGeometry.create(target, self.layout, self.layout.client_box)
        frame = np.arange(140 * 200 * 3, dtype=np.uint8).reshape((140, 200, 3))
        self.assertIs(frame, geometry.normalize(frame))
        self.assertEqual((-1820, 470), geometry.to_desktop((100, 70)))
        self.assertEqual((100, 70), geometry.from_desktop((-1820, 470)))

    def test_game_content_is_scaled_separately_from_title_bar_and_borders(self):
        frame = np.full((84, 126, 3), 17, np.uint8)
        frame[12:79, 3:123] = 93
        normalized = self.geometry.normalize(frame)
        self.assertEqual((140, 200, 3), normalized.shape)
        np.testing.assert_array_equal(normalized[30:130, 10:190], np.full((100, 180, 3), 93, np.uint8))
        self.assertEqual(17, normalized[0, 0, 0])
        self.assertEqual((-237, 179), self.geometry.to_desktop((100, 70)))

    def test_round_trip_keeps_reference_points_within_one_source_pixel(self):
        for point in ((0, 0), (8, 20), (10, 30), (100, 70), (189, 129), (199, 139)):
            with self.subTest(point=point):
                desktop = frame_to_desktop(self.capture, point)
                current = desktop_to_frame(self.capture, desktop)
                np.testing.assert_allclose(current, point, atol=2)
                self.assertTrue(self.target["left"] <= desktop[0] < self.target["left"] + self.target["width"])
                self.assertTrue(self.target["top"] <= desktop[1] < self.target["top"] + self.target["height"])

    def test_wrong_aspect_or_outside_client_is_rejected_before_normalization(self):
        for client in ((3, 12, 120, 70), (-1, 12, 123, 79), (3, 12, 127, 79), (3, 12, 3, 79)):
            with self.subTest(client=client), self.assertRaises(ValueError):
                FrameGeometry.create(self.target, self.layout, client)
        with self.assertRaises(ValueError):
            self.geometry.normalize(np.zeros((83, 126, 3), np.uint8))
        for point in ((200, 70), (-1, 30), (100, float("nan"))):
            with self.subTest(point=point), self.assertRaises(ValueError):
                self.geometry.to_desktop(point)

    def test_resized_desktop_pointer_can_reenter_integer_hazard_and_hover_regions(self):
        layout = ReferenceLayout((1922, 1140), (13, 47, 1909, 1127))
        target, client = dict(left=135, top=1061, width=1696, height=1005), (11, 40, 1685, 994)
        capture = SimpleNamespace(
            target=target,
            frame_geometry=FrameGeometry.create(target, layout, client),
            current_target=lambda: target,
            current_client_box=lambda: client,
        )
        point = desktop_to_frame(capture, (1454, 1246))
        self.assertTrue(all(type(value) is int for value in point))
        frame = np.zeros((1140, 1922, 3), np.uint8)
        hazard = ArcaneHazardObserver.__new__(ArcaneHazardObserver)
        hazard.sequence = 0
        hazard.region = SimpleNamespace(region_id="arcane", frame_size=(1922, 1140), scan=Mock(return_value=()))
        self.assertEqual((), hazard.observe(frame, (963, 577), 0, 0, aim=point))
        self.assertEqual(2, hazard.region.scan.call_count)
        hover = ArcaneHoverObserver.__new__(ArcaneHoverObserver)
        hover.templates = []
        self.assertEqual((), hover.observe(frame, point))

    def test_moved_resized_or_changed_client_cannot_reuse_old_coordinates(self):
        for change in ("left", "width", "client"):
            with self.subTest(change=change):
                self.setUp()
                if change == "client":
                    self.client = (4, 12, 124, 79)
                else:
                    self.target[change] += 10
                self.assertFalse(geometry_is_current(self.capture))
                with self.assertRaises(RuntimeError):
                    frame_to_desktop(self.capture, (100, 70))
                with self.assertRaises(RuntimeError):
                    desktop_to_frame(self.capture, (-237, 179))

    def test_unconfigured_captures_keep_native_coordinate_behavior(self):
        capture = SimpleNamespace(target=dict(left=300, top=-100))
        self.assertEqual((400, -30), frame_to_desktop(capture, (100, 70)))
        self.assertEqual((100, 70), desktop_to_frame(capture, (400, -30)))
        geometry = FrameGeometry.create(self.target)
        frame = np.zeros((84, 126, 3), np.uint8)
        self.assertIs(frame, geometry.normalize(frame))

    def test_capture_reuses_geometry_until_source_or_client_changes(self):
        window = WindowInfo(1, "test", **self.target)
        frame = np.zeros((84, 126, 3), np.uint8)
        with patch("diablo2.common.capture.mss.mss"), patch.object(
            ScreenCapture, "_resolve_target", return_value=self.target
        ), patch.object(ScreenCapture, "_resolve_window", return_value=window), patch.object(
            ScreenCapture, "current_client_box", side_effect=lambda: self.client
        ), patch(
            "diablo2.common.capture.capture_window_image", return_value=frame
        ), patch(
            "diablo2.common.capture.ReferenceLayout.load", return_value=self.layout
        ):
            capture = ScreenCapture(CaptureConfig(window_title="test", reference_layout="layout.json"))
            first, second = capture.grab(), capture.grab()
            self.assertIs(first.geometry, second.geometry)
            self.assertEqual((140, 200, 3), second.frame.shape)
            self.assertEqual(self.target, capture.target)
            self.client = (4, 12, 124, 79)
            self.assertFalse(geometry_is_current(capture))
            third = capture.grab()
            self.assertIsNot(first.geometry, third.geometry)
            self.assertTrue(geometry_is_current(capture))
            capture.close()

    def test_town_reference_point_is_converted_once_before_actual_click(self):
        session = RunLifecycleSession.__new__(RunLifecycleSession)
        session._ensure_input_allowed = Mock()
        session._move_absolute = Mock()
        session._stop_event = Mock()
        session._town_mouse_held = False
        frame = np.zeros((140, 200, 3), np.uint8)
        with patch("diablo2.actions.run_lifecycle.pydirectinput") as backend:
            session._dispatch_town_action(self.capture, frame, dict(kind="click", point=(633.5, 376.5)), (1267, 753))
        session._move_absolute.assert_called_once_with(self.capture, -237, 179)
        backend.click.assert_called_once()

    def test_current_hover_uses_the_same_mapping_for_move_and_pointer_confirmation(self):
        backend, observer, now = DryFieldInput(), Mock(), [0.0]
        observer.observe.return_value = "current hostile"
        probe = NearbyHostileProbe(
            observer, backend, (0, 0), clock=lambda: now[0], to_desktop=lambda point: frame_to_desktop(self.capture, point)
        )
        frame = np.zeros((140, 200, 3), np.uint8)
        self.assertIsNone(probe.observe(frame, (100, 70), 0, points=((70, 60),)))
        self.assertIn(("move", *self.geometry.to_desktop((70, 60))), backend.events)
        now[0] = 0.06
        self.assertEqual("current hostile", probe.observe(frame, (100, 70), 0.06))

    def test_save_exit_uses_captured_reference_dimensions_instead_of_physical_window_size(self):
        self.capture.grab = Mock(return_value=SimpleNamespace(frame=np.zeros((140, 200, 3), np.uint8)))
        session, vision, stop = Mock(), Mock(), Mock()
        stop.is_set.return_value = False
        vision.find.return_value = dict(x=633.5, y=376.5)
        exit_to_character(session, self.capture, lambda: True, stop, vision)
        session._press_key.assert_called_once_with("esc")
        session._click_relative.assert_called_once_with(self.capture, (100, 70))
        self.assertEqual(2, self.capture.grab.call_count)

    def test_focusing_preserves_split_or_maximized_window_geometry(self):
        window = WindowInfo(7, "test", 100, 200, 126, 84)
        with patch("diablo2.common.capture.USER32") as api:
            api.IsIconic.return_value = False
            api.GetForegroundWindow.return_value = 7
            self.assertTrue(focus_window(window))
            api.ShowWindow.assert_not_called()
            api.IsIconic.return_value = True
            self.assertTrue(focus_window(window))
            api.ShowWindow.assert_called_once_with(7, 9)


if __name__ == "__main__":
    unittest.main()
