import json
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import cv2 as cv
import numpy as np

from diablo2.common.automap_vision import AutomapObserver
from diablo2.common.frame_geometry import FrameGeometry, ReferenceLayout
from diablo2.common.supervised_field import ensure_automap_on


class AutomapTests(unittest.TestCase):
    def test_stone_underlay_title_requires_all_current_letter_parts(self):
        root = Path(__file__).resolve().parents[1]
        frame = cv.imread(
            str(root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/area-title-stone-underlay.png")
        )
        observer = AutomapObserver()
        self.assertTrue(observer.visible(frame, "arcane_sanctuary"))
        self.assertTrue(observer.last_evidence["template"].endswith("area-title-stone-underlay-window.png"))
        self.assertEqual(3, len(observer.last_evidence["part_scores"]))
        self.assertTrue(all(value >= 0.86 for value in observer.last_evidence["part_scores"]))
        self.assertFalse(observer.visible(frame, "canyon_of_the_magi"))
        normalized = cv.resize(frame, (1267, 753), interpolation=cv.INTER_AREA)
        for a, b in ((1178, 1200), (1200, 1223), (1223, 1246), (1178, 1246)):
            changed = normalized.copy()
            changed[70:85, a:b] = 0
            with self.subTest(removed=(a, b)):
                self.assertFalse(observer.visible(changed, "arcane_sanctuary"))
                self.assertIsNone(observer.last_evidence)

    def test_shared_area_title_survives_gray_map_lines_but_blank_and_other_layout_do_not_pass(self):
        directory = Path(__file__).resolve().parents[1] / "assets/ui/automap"
        manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
        title = cv.imdecode(np.fromfile(str(directory / manifest["areas"]["arcane_sanctuary"]["file"]), np.uint8), cv.IMREAD_COLOR)
        frame = np.zeros((753, 1267, 3), np.uint8)
        frame[130:145, 1178:1246] = title
        # 별도 정보 행으로 제목이 내려가거나 회색 지도 선이 겹쳐도 확인한다.
        gray_line = np.all(frame[130:145, 1178:1246] < 35, axis=2)
        patch = frame[130:145, 1178:1246]
        patch[gray_line] = (90, 90, 90)
        observer = AutomapObserver(directory)
        self.assertTrue(observer.visible(frame, "arcane_sanctuary"))
        self.assertFalse(observer.visible(np.zeros_like(frame), "arcane_sanctuary"))
        self.assertFalse(observer.visible(np.zeros((753, 900, 3), np.uint8), "arcane_sanctuary"))

    def test_current_light_floor_title_variant_requires_the_actual_arcane_letters(self):
        root = Path(__file__).resolve().parents[1]
        filename = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/area-title-light-floor.png"
        frame = cv.imdecode(np.fromfile(str(filename), np.uint8), cv.IMREAD_COLOR)
        observer = AutomapObserver()
        self.assertTrue(observer.visible(frame, "arcane_sanctuary"))
        self.assertGreaterEqual(observer.last_evidence["score"], 0.86)
        self.assertTrue(observer.last_evidence["template"].endswith("area-title-light-floor-window.png"))
        self.assertFalse(observer.visible(frame, "canyon_of_the_magi"))
        removed = cv.resize(frame, (1267, 753), interpolation=cv.INTER_AREA)
        removed[70:85, 1178:1246] = 0
        self.assertFalse(observer.visible(removed, "arcane_sanctuary"))
        self.assertIsNone(observer.last_evidence)
        canyon = np.zeros((753, 1267, 3), np.uint8)
        title_path = root / "assets/ui/automap" / observer.manifest["areas"]["canyon_of_the_magi"]["file"]
        title = cv.imdecode(np.fromfile(str(title_path), np.uint8), cv.IMREAD_COLOR)
        canyon[100 : 100 + title.shape[0], 1150 : 1150 + title.shape[1]] = title
        self.assertTrue(observer.visible(canyon, "canyon_of_the_magi"))
        self.assertFalse(observer.visible(canyon, "arcane_sanctuary"))

    def test_area_title_survives_client_downsampling_without_accepting_removed_letters(self):
        root = Path(__file__).resolve().parents[1]
        layout = ReferenceLayout.load(root / "assets/ui/layouts/remastered-1922x1140/viewport.json")
        source = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/area-title-light-floor.png"
        frame = cv.imdecode(np.fromfile(str(source), np.uint8), cv.IMREAD_COLOR)
        observer = AutomapObserver()
        x0, y0, x1, y1 = layout.client_box
        for size, client in (((1716, 1011), (11, 40, 1705, 1000)), ((1373, 809), (9, 32, 1364, 800))):
            with self.subTest(size=size):
                a, b, c, d = client
                raw = cv.resize(frame, size, interpolation=cv.INTER_AREA)
                raw[b:d, a:c] = cv.resize(frame[y0:y1, x0:x1], (c - a, d - b), interpolation=cv.INTER_AREA)
                geometry = FrameGeometry.create(dict(left=-100, top=20, width=size[0], height=size[1]), layout, client)
                normalized = geometry.normalize(raw)
                self.assertTrue(observer.visible(normalized, "arcane_sanctuary"))
                self.assertGreaterEqual(observer.last_evidence["score"], 0.86)
                self.assertFalse(observer.visible(normalized, "canyon_of_the_magi"))
                removed = cv.resize(normalized, (1267, 753), interpolation=cv.INTER_AREA)
                removed[70:85, 1178:1246] = 0
                self.assertFalse(observer.visible(removed, "arcane_sanctuary"))

    def test_marker_overlap_title_requires_all_three_current_letter_parts(self):
        root = Path(__file__).resolve().parents[1]
        filename = root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/area-title-marker-overlap.png"
        frame = cv.imdecode(np.fromfile(str(filename), np.uint8), cv.IMREAD_COLOR)
        observer = AutomapObserver()
        self.assertTrue(observer.visible(frame, "arcane_sanctuary"))
        self.assertTrue(observer.last_evidence["template"].endswith("area-title-marker-overlap-window.png"))
        self.assertGreaterEqual(observer.last_evidence["score"], 0.86)
        self.assertEqual(3, len(observer.last_evidence["part_scores"]))
        self.assertTrue(all(value >= 0.86 for value in observer.last_evidence["part_scores"]))
        self.assertFalse(observer.visible(frame, "canyon_of_the_magi"))
        normalized = cv.resize(frame, (1267, 753), interpolation=cv.INTER_AREA)
        for a, b in ((1178, 1200), (1200, 1223), (1223, 1246), (1178, 1246)):
            with self.subTest(removed=(a, b)):
                removed = normalized.copy()
                removed[70:85, a:b] = 0
                self.assertFalse(observer.visible(removed, "arcane_sanctuary"))
                self.assertIsNone(observer.last_evidence)

    def test_gold_title_mask_rejects_retained_act1_and_act2_town_frames(self):
        root = Path(__file__).resolve().parents[1] / "assets/regions"
        observer = AutomapObserver()
        for filename in (
            "rogue-encampment/remastered-ko-window/references/map-reviewed-window.png",
            "rogue-encampment/remastered-ko-window/references/map-baseline.jpg",
            "lut-gholein/remastered-ko-window/references/town-baseline.jpg",
            "lut-gholein/remastered-ko-window/references/main-reviewed.png",
        ):
            with self.subTest(filename=filename):
                frame = cv.imdecode(np.fromfile(str(root / filename), np.uint8), cv.IMREAD_COLOR)
                self.assertFalse(observer.visible(frame, "arcane_sanctuary"))

    def prepare(self, visible, *, focused=True, panel=False, blocked=None, slow=False, obscured=False, supervised_on=None, **policy):
        now = [0.0]
        stop, capture, backend, observer = Mock(), Mock(), Mock(), Mock()
        stop.is_set.return_value = False
        stop.wait.side_effect = lambda seconds: now.__setitem__(0, now[0] + seconds)

        def grab():
            now[0] += 0.3 if slow else 0.02
            return SimpleNamespace(frame=object())

        capture.grab.side_effect = grab
        observer.visible.side_effect = lambda *args: visible(backend)
        observer.last_evidence = dict(score=0.95)
        observer.last_obscured = obscured
        with patch("diablo2.common.supervised_field.resource_guard", return_value=blocked):
            try:
                result = ensure_automap_on(
                    capture,
                    backend,
                    lambda: focused,
                    stop,
                    Mock(),
                    Mock(),
                    observer,
                    lambda frame: panel,
                    clock=lambda: now[0],
                    supervised_on=supervised_on,
                    **policy,
                )
            except RuntimeError as error:
                result = error
        return result, backend

    def test_already_on_never_toggles(self):
        result, backend = self.prepare(lambda backend: True)
        self.assertEqual("on_observed", result["status"])
        self.assertFalse(result["tab_sent"])
        backend.press.assert_not_called()

    def test_missing_title_toggles_once_and_requires_current_positive_result(self):
        result, backend = self.prepare(lambda backend: backend.press.called)
        self.assertEqual("on_observed", result["status"])
        self.assertTrue(result["tab_sent"])
        backend.press.assert_called_once_with("tab")

    def test_failed_confirmation_never_retries_tab(self):
        result, backend = self.prepare(lambda backend: False)
        self.assertIsInstance(result, RuntimeError)
        self.assertIn("unverified", str(result))
        backend.press.assert_called_once_with("tab")

    def test_restore_confirmation_never_toggles_again_or_accepts_unknown_title(self):
        for visible in (False, True):
            with self.subTest(visible=visible):
                result, backend = self.prepare(lambda backend: visible, allow_toggle=False, timeout=0.5)
                backend.press.assert_not_called()
                if visible:
                    self.assertEqual("on_observed", result["status"])
                else:
                    self.assertIsInstance(result, RuntimeError)

    def test_label_occluded_title_never_turns_an_unknown_map_off(self):
        result, backend = self.prepare(lambda backend: False, obscured=True)
        self.assertIsInstance(result, RuntimeError)
        backend.press.assert_not_called()

    def test_supervised_visible_map_survives_title_occlusion_without_becoming_automatic_evidence(self):
        proof = dict(source="supervised_visual", area="arcane_sanctuary", observed_at=100, automap_on=True)
        with patch("diablo2.common.supervised_field.time.time", return_value=115):
            result, backend = self.prepare(lambda backend: False, obscured=True, supervised_on=proof)
        self.assertEqual("on_supervised", result["status"])
        self.assertEqual(proof, result["evidence"])
        backend.press.assert_not_called()
        for change in (dict(observed_at=99.99), dict(observed_at=116), dict(source="unknown"), dict(area="town")):
            with patch("diablo2.common.supervised_field.time.time", return_value=115):
                result, backend = self.prepare(lambda backend: False, supervised_on=dict(proof, **change))
            self.assertIsInstance(result, RuntimeError)
            backend.press.assert_not_called()

    def test_supervised_map_does_not_override_current_focus_panel_resources_or_stale_frame(self):
        proof = dict(source="supervised_visual", area="arcane_sanctuary", observed_at=100, automap_on=True)
        for settings in (dict(focused=False), dict(panel=True), dict(blocked="resource_low"), dict(slow=True)):
            with patch("diablo2.common.supervised_field.time.time", return_value=100):
                result, backend = self.prepare(lambda backend: False, supervised_on=proof, **settings)
            self.assertIsInstance(result, RuntimeError)
            backend.press.assert_not_called()

    def test_stale_focus_panel_and_resource_failures_do_not_toggle(self):
        for settings in (dict(focused=False), dict(panel=True), dict(blocked="resource_low"), dict(slow=True)):
            result, backend = self.prepare(lambda backend: False, **settings)
            self.assertIsInstance(result, RuntimeError)
            backend.press.assert_not_called()


if __name__ == "__main__":
    unittest.main()
