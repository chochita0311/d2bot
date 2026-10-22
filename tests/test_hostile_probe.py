import unittest
from types import SimpleNamespace
from unittest.mock import Mock

import cv2 as cv
import numpy as np

from diablo2.common.hostile_probe import NearbyHostileProbe, nearby_actor_proposals


class NearbyHostileProbeTests(unittest.TestCase):
    def test_proposed_body_hover_uses_its_near_ground_and_still_requires_current_panel(self):
        for condition in ("confirmed", "no_panel", "pointer", "moved_player", "distant_ground"):
            with self.subTest(condition=condition):
                now = [10.0]
                backend, observer = Mock(), Mock()
                backend.pointer_matches.return_value = condition != "pointer"
                target = object()
                observer.observe.return_value = None if condition == "no_panel" else target
                probe = NearbyHostileProbe(observer, backend, (100, 200), clock=lambda: now[0])
                frame = SimpleNamespace(shape=(1140, 1922, 3))
                point = (1300, 429) if condition == "distant_ground" else (1154, 429)
                self.assertIsNone(probe.observe(frame, (962, 580), 10, points=(point,)))
                now[0] = 10.1
                player = (500, 580) if condition == "moved_player" else (962, 580)
                result = probe.observe(frame, player, 10.1, points=(point,))
                self.assertEqual(condition == "confirmed", result is target)
                if condition == "distant_ground":
                    backend.move.assert_not_called()
                else:
                    backend.move.assert_called_once_with(1254, 629)
                backend.key_down.assert_not_called()

    def test_near_effect_only_proposes_hover_points_and_excludes_player_and_distant_effects(self):
        frame = np.zeros((1140, 1922, 3), np.uint8)
        for point in ((962, 580), (1122, 600), (1450, 600)):
            cv.ellipse(frame, point, (40, 10), 0, 0, 360, (0, 220, 0), -1)
        self.assertEqual(((1122, 520),), nearby_actor_proposals(frame, (962, 580)))
        self.assertEqual((), nearby_actor_proposals(frame, None))

    def test_proposal_is_not_hostile_without_fresh_panel_and_retries_are_bounded(self):
        now = [10.0]
        backend, observer = Mock(), Mock()
        observer.observe.return_value = None
        backend.pointer_matches.return_value = True
        probe = NearbyHostileProbe(observer, backend, (0, 0), clock=lambda: now[0])
        frame = SimpleNamespace(shape=(1140, 1922, 3))
        points = ((1122, 520),)
        self.assertIsNone(probe.observe(frame, (962, 580), 10, points=points))
        now[0] = 10.1
        self.assertIsNone(probe.observe(frame, (962, 580), 10.1, points=points))
        self.assertEqual(1, backend.move.call_count)
        self.assertIsNone(probe.pending)
        now[0] = 10.51
        probe.observe(frame, (962, 580), 10.51, points=points)
        self.assertEqual(2, backend.move.call_count)
        observer.observe.return_value = object()
        now[0] = 10.61
        self.assertIsNotNone(probe.observe(frame, (962, 580), 10.61, points=points))

    def test_still_visible_panel_needs_current_near_proposal_at_original_ground(self):
        for points in ((), ((1240, 520),), ((1450, 520),), ((1120, 520),)):
            with self.subTest(points=points):
                now = [10.0]
                backend, observer = Mock(), Mock()
                backend.pointer_matches.return_value = True
                target = object()
                observer.observe.return_value = target
                probe = NearbyHostileProbe(observer, backend, (0, 0), clock=lambda: now[0])
                frame = SimpleNamespace(shape=(1140, 1922, 3))
                probe.observe(frame, (962, 580), 10, points=((1120, 520),))
                now[0] = 10.1
                result = probe.observe(frame, (962, 580), 10.1, points=points)
                self.assertEqual(points == ((1120, 520),), result is target)
                if result is None:
                    observer.observe.assert_not_called()
        backend.key_down.assert_not_called()

    def test_current_hover_panel_is_required_after_move_and_never_reused_when_missing(self):
        now = [10.0]
        backend, observer = Mock(), Mock()
        frame = SimpleNamespace(shape=(1140, 1922, 3))
        target = object()
        observer.observe.return_value = target
        backend.pointer_matches.return_value = True
        probe = NearbyHostileProbe(observer, backend, (100, 200), clock=lambda: now[0])
        self.assertIsNone(probe.observe(frame, (962, 580), 10))
        observer.observe.assert_not_called()
        backend.move.assert_called_once_with(982, 650)
        now[0] = 10.1
        self.assertIs(target, probe.observe(frame, (962, 580), 10.1))
        observer.observe.return_value = None
        now[0] = 10.2
        self.assertIsNone(probe.observe(frame, (962, 580), 10.2))
        self.assertEqual(2, backend.move.call_count)
        backend.press.assert_not_called()
        backend.key_down.assert_not_called()

    def test_unknown_ground_stale_frame_pointer_mismatch_and_far_hover_deny_confirmation(self):
        for condition in ("ground", "stale", "pointer", "far"):
            now = [10.0]
            backend, observer = Mock(), Mock()
            backend.pointer_matches.return_value = condition != "pointer"
            probe = NearbyHostileProbe(observer, backend, (0, 0), clock=lambda: now[0])
            frame = SimpleNamespace(shape=(1140, 1922, 3))
            probe.observe(frame, (962, 580), 10)
            now[0] = 10.4 if condition == "stale" else 10.1
            player = None if condition == "ground" else (1500, 700) if condition == "far" else (962, 580)
            self.assertIsNone(probe.observe(frame, player, 10.1))
            observer.observe.assert_not_called()

    def test_slow_panel_read_cannot_authorize_attack_from_expired_frame(self):
        now = [10.0]
        backend, observer = Mock(), Mock()
        backend.pointer_matches.return_value = True
        probe = NearbyHostileProbe(observer, backend, (0, 0), clock=lambda: now[0])
        frame = SimpleNamespace(shape=(1140, 1922, 3))
        probe.observe(frame, (962, 580), 10)
        now[0] = 10.1
        observer.observe.side_effect = lambda frame: now.__setitem__(0, 10.4) or object()
        self.assertIsNone(probe.observe(frame, (962, 580), 10.1))


if __name__ == "__main__":
    unittest.main()
