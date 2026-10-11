import unittest

import cv2 as cv
import numpy as np

from diablo2.common.label_occlusion import label_occlusion_mask, visible_template_peak
from diablo2.common.supervised_field import enable_fresh_room_item_labels
from types import SimpleNamespace
from unittest.mock import Mock, patch


class LabelOcclusionTests(unittest.TestCase):
    def test_visible_texture_match_requires_sixty_percent_and_rejects_different_texture(self):
        random = np.random.RandomState(37)
        template = random.randint(0, 256, (40, 50)).astype(np.uint8)
        frame = random.randint(0, 256, (48, 60)).astype(np.uint8)
        frame[4:44, 6:56] = template
        visible = np.ones_like(frame)
        frame[4:20, 6:56] = 0
        visible[4:20, 6:56] = 0
        score, location, fraction = visible_template_peak(frame, template, visible)
        self.assertGreater(score, 0.99)
        self.assertEqual((6, 4), location)
        self.assertAlmostEqual(0.6, fraction)
        visible[20:21, 6:56] = 0
        self.assertLess(visible_template_peak(frame, template, visible)[0], 0.92)
        other = random.randint(0, 256, template.shape).astype(np.uint8)
        self.assertLess(visible_template_peak(frame, other, np.ones_like(frame))[0], 0.92)
        self.assertEqual(-1, visible_template_peak(np.zeros_like(frame), template, np.ones_like(frame))[0])

    def test_dark_label_rows_are_current_frame_masks_and_can_exclude_gold_metadata(self):
        frame = np.zeros((1140, 1922, 3), np.uint8)
        cv.putText(frame, "ITEM LABEL", (700, 400), cv.FONT_HERSHEY_SIMPLEX, 0.7, (240, 240, 240), 1)
        cv.putText(frame, "AREA TITLE", (1700, 150), cv.FONT_HERSHEY_SIMPLEX, 0.7, (40, 140, 210), 1)
        mask = label_occlusion_mask(frame, include_gold=False)
        self.assertTrue(np.any(mask[380:410, 700:850]))
        self.assertFalse(np.any(mask[120:160, 1700:1900]))
        self.assertFalse(np.any(label_occlusion_mask(np.zeros_like(frame))))

    def test_fresh_room_label_request_is_guarded_and_does_not_claim_visible_drops(self):
        for condition in ("ready", "focus", "stop", "resource", "panel", "stale"):
            with self.subTest(condition=condition):
                capture, backend, stop = Mock(), Mock(), Mock()
                capture.grab.return_value = SimpleNamespace(frame=object())
                stop.is_set.return_value = condition == "stop"
                times = iter((0, 0.3 if condition == "stale" else 0.1))
                with patch("diablo2.common.supervised_field.resource_guard", return_value="low" if condition == "resource" else None):
                    arguments = (capture, backend, lambda: condition != "focus", stop, Mock(), Mock(), lambda frame: condition == "panel")
                    if condition == "ready":
                        result = enable_fresh_room_item_labels(*arguments, clock=lambda: next(times))
                        self.assertEqual("unverified_until_drop", result["visibility"])
                        backend.press.assert_called_once_with("alt")
                    else:
                        with self.assertRaises(RuntimeError):
                            enable_fresh_room_item_labels(*arguments, clock=lambda: next(times))
                        backend.press.assert_not_called()
