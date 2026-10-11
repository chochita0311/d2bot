import unittest
import json
import tempfile
from pathlib import Path
from types import SimpleNamespace

import numpy as np

from diablo2.runs.summoner.camera_ground import CalibratedCameraGroundObserver
from diablo2.runs.summoner.scene_vision import GroundReading


class CameraGroundTests(unittest.TestCase):
    def setUp(self):
        self.frame = np.zeros((1140, 1922, 3), np.uint8)
        self.reading = GroundReading((962, 578), "flash_body_and_es", 0.95)
        self.ground = SimpleNamespace(observe=lambda frame, captured_at: self.reading)
        self.open = False
        self.observer = CalibratedCameraGroundObserver(self.ground, lambda frame: self.open)
        self.hud = SimpleNamespace(belt_visibility="closed", life=SimpleNamespace(ratio=1.0), mana=SimpleNamespace(ratio=0.8))
        self.proof = dict(source="supervised_visual", panels_closed=True, observed_at=10)
        self.observer.confirm_closed_viewport(self.proof, 10)

    def observe(self, at, *, focused=True, frame=None):
        frame = self.frame if frame is None else frame
        self.observer.prepare_frame(frame, at, self.hud, focused)
        return self.observer.observe(frame, captured_at=at)

    def calibrate(self):
        self.observer.confirm_closed_viewport(self.proof, 10)
        for at, point in ((10.0, (962, 578)), (10.1, (960, 580)), (10.2, (964, 576))):
            self.reading = GroundReading(point, "flash_body_and_es")
            self.assertEqual(point, self.observe(at).point)
        self.reading = GroundReading(None, "ground_unverified")

    def test_requires_fresh_closed_proof_and_three_actual_separate_samples(self):
        for proof in (dict(self.proof, panels_closed=False), dict(self.proof, source="guess"), dict(self.proof, observed_at=-10)):
            with self.assertRaises(ValueError):
                self.observer.confirm_closed_viewport(proof, 10)
        self.observe(10)
        self.observe(10)
        self.reading = GroundReading(None, "ground_unverified")
        self.assertIsNone(self.observe(10.1).point)
        self.calibrate()
        current = self.observe(10.3)
        self.assertEqual((962, 578), current.point)
        self.assertEqual("camera_ground_projection", current.reason)
        self.assertIsNone(current.score)
        self.assertAlmostEqual(0.1, current.calibration["occluded_seconds"])

    def test_expiry_does_not_renew_itself_and_current_actor_is_preferred(self):
        self.calibrate()
        self.assertIsNotNone(self.observe(12.2).point)
        self.assertIsNone(self.observe(12.21).point)
        self.reading = GroundReading((957, 583), "flash_body_and_es", 0.96)
        self.assertEqual(self.reading, self.observe(12.3))

    def test_panel_focus_layout_and_hud_failure_discard_calibration(self):
        for failure in ("panel", "focus", "layout", "hud", "belt"):
            with self.subTest(failure=failure):
                self.setUp()
                self.calibrate()
                if failure == "panel":
                    self.open = True
                if failure == "hud":
                    self.hud.mana.ratio = None
                if failure == "belt":
                    self.hud.belt_visibility = "unknown"
                frame = self.frame[:900] if failure == "layout" else self.frame
                self.assertIsNone(self.observe(10.3, frame=frame, focused=failure != "focus").point)
                self.assertIsNone(self.observer.center)

    def test_offset_actor_invalidates_camera_and_ambiguous_actor_is_not_projected(self):
        self.calibrate()
        self.reading = GroundReading(None, "ground_ambiguous")
        self.assertIsNone(self.observe(10.3).point)
        self.reading = GroundReading((1000, 578), "flash_body_and_es")
        self.assertIsNone(self.observe(10.4).point)
        self.assertIsNone(self.observer.center)
        self.reading = GroundReading(None, "ground_unverified")
        self.assertIsNone(self.observe(10.5).point)
        for at in (10.6, 10.7, 10.8):
            self.reading = GroundReading((962, 578), "flash_body_and_es")
            self.assertEqual((962, 578), self.observe(at).point)
        self.reading = GroundReading(None, "ground_unverified")
        self.assertEqual((962, 578), self.observe(10.9).point)

    def test_same_frame_context_and_monotonic_time_required(self):
        self.calibrate()
        self.assertIsNone(self.observer.observe(self.frame.copy(), captured_at=10.3).point)
        self.calibrate()
        self.assertEqual("camera_time_reversed", self.observe(10.1).reason)
        with self.assertRaises(ValueError):
            self.observe(float("nan"))

    def test_tracked_marker_cannot_seed_or_refresh_camera_calibration(self):
        self.reading = GroundReading((962, 578), "flash_marker_tracked")
        for at in (10.0, 10.1, 10.2):
            self.observe(at)
        self.assertIsNone(self.observer.center)
        self.calibrate()
        self.reading = GroundReading((962, 578), "flash_marker_tracked")
        self.observe(11.9)
        self.reading = GroundReading(None, "ground_unverified")
        self.assertIsNone(self.observe(12.21).point)

    def test_reviewed_layout_seed_requires_fresh_confirmation_and_expires_without_body(self):
        data = dict(
            schema=1, frame_size=[1922, 1140], body_observations=[dict(point=[962, 578]), dict(point=[960, 580]), dict(point=[964, 576])]
        )
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "camera.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            with self.assertRaises(ValueError):
                self.observer.confirm_closed_viewport(self.proof, 10, reviewed_layout=path)
            self.observer.confirm_closed_viewport(dict(self.proof, camera_layout_verified=True), 10, reviewed_layout=path)
            self.reading = GroundReading(None, "ground_unverified")
            reading = self.observe(10)
            self.assertEqual((962, 578), reading.point)
            self.assertIsNone(reading.calibration["last_body_at"])
            self.assertEqual("reviewed_layout", reading.calibration["reference_kind"])
            self.assertIsNotNone(self.observe(11.9).point)
            self.assertIsNone(self.observe(12.01).point)

    def test_persistent_reviewed_layout_requires_current_context_and_rejects_false_shape_offset(self):
        data = dict(schema=1, frame_size=[1922, 1140], body_observations=[dict(point=[962, 578])] * 3)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "camera.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            self.observer = CalibratedCameraGroundObserver(self.ground, lambda frame: self.open, persist_reviewed_layout=True)
            self.observer.confirm_closed_viewport(dict(self.proof, camera_layout_verified=True), 10, reviewed_layout=path)
            self.reading = GroundReading((926, 514), "flash_es_and_body_shape")
            self.assertEqual((962, 578), self.observe(10).point)
            self.reading = GroundReading(None, "ground_unverified")
            reading = self.observe(30)
            self.assertEqual((962, 578), reading.point)
            self.assertTrue(reading.calibration["current_reviewed_viewport"])
            self.assertIsNone(reading.calibration["last_body_at"])
            self.open = True
            self.assertIsNone(self.observe(30.1).point)
            self.assertFalse(self.observer.reviewed_projection_active)

    def test_persistent_layout_does_not_override_a_strong_displaced_actor(self):
        data = dict(schema=1, frame_size=[1922, 1140], body_observations=[dict(point=[962, 578])] * 3)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "camera.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            self.observer = CalibratedCameraGroundObserver(self.ground, lambda frame: self.open, persist_reviewed_layout=True)
            self.observer.confirm_closed_viewport(dict(self.proof, camera_layout_verified=True), 10, reviewed_layout=path)
            self.reading = GroundReading((926, 514), "flash_marker_and_body", 0.95)
            self.assertEqual("camera_actor_displaced", self.observe(10).reason)
            self.assertFalse(self.observer.reviewed_projection_active)
            self.reading = GroundReading(None, "ground_unverified")
            self.assertIsNone(self.observe(10.1).point)

    def test_displaced_actor_needs_three_current_body_samples_to_restore_reviewed_projection(self):
        data = dict(schema=1, frame_size=[1922, 1140], body_observations=[dict(point=[962, 578])] * 3)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "camera.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            self.observer = CalibratedCameraGroundObserver(self.ground, lambda frame: self.open, persist_reviewed_layout=True)
            self.observer.confirm_closed_viewport(dict(self.proof, camera_layout_verified=True), 10, reviewed_layout=path)
            self.reading = GroundReading((926, 514), "flash_marker_and_body", 0.95)
            self.assertEqual("camera_actor_displaced", self.observe(10).reason)
            for index, at in enumerate((10.1, 10.2, 10.3)):
                self.reading = GroundReading((962, 578), "flash_body_and_es", 0.96)
                self.assertEqual((962, 578), self.observe(at).point)
                self.assertEqual(index == 2, self.observer.reviewed_projection_active)
            self.reading = GroundReading(None, "ground_unverified")
            reading = self.observe(30)
            self.assertEqual((962, 578), reading.point)
            self.assertTrue(reading.calibration["current_reviewed_viewport"])
            self.open = True
            self.assertIsNone(self.observe(30.1).point)
            self.assertFalse(self.observer.authorized)

    def test_persistent_reviewed_reference_survives_only_transient_resource_failure(self):
        data = dict(schema=1, frame_size=[1922, 1140], body_observations=[dict(point=[962, 578])] * 3)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "camera.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            for failure in ("hud", "belt", "panel", "focus", "layout"):
                with self.subTest(failure=failure):
                    self.setUp()
                    self.observer = CalibratedCameraGroundObserver(self.ground, lambda frame: self.open, persist_reviewed_layout=True)
                    self.observer.confirm_closed_viewport(dict(self.proof, camera_layout_verified=True), 10, reviewed_layout=path)
                    self.reading = GroundReading(None, "ground_unverified")
                    self.hud.mana.ratio = None if failure in ("hud", "panel") else 0.8
                    self.hud.belt_visibility = "unknown" if failure == "belt" else "closed"
                    self.open = failure == "panel"
                    frame = self.frame[:900] if failure == "layout" else self.frame
                    self.assertIsNone(self.observe(10, frame=frame, focused=failure != "focus").point)
                    self.hud.mana.ratio, self.hud.belt_visibility, self.open = 0.8, "closed", False
                    recovered = self.observe(10.2)
                    self.assertEqual(failure in ("hud", "belt"), recovered.reason == "camera_ground_projection")
                    if failure in ("hud", "belt"):
                        self.assertEqual((962, 578), recovered.point)
                        self.assertTrue(self.observer.reset_events[-1]["reviewed_reference_retained"])


if __name__ == "__main__":
    unittest.main()
