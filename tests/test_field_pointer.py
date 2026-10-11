import unittest
from unittest.mock import Mock, patch

from diablo2.common import controller
from diablo2.common.controller import BotController, ControllerFieldInput
from diablo2.common.supervised_field import RecordingInput


class FieldPointerTests(unittest.TestCase):
    def backend(self):
        return ControllerFieldInput(BotController(dry_run=False), focused=lambda: True, bounds=lambda: (-1900, -1000, 3800, 2100))

    def test_virtual_desktop_edges_include_secondary_and_negative_coordinates(self):
        events = []

        def send(count, event, size):
            events.append((event.contents.ii.mi.dx, event.contents.ii.mi.dy, event.contents.ii.mi.dwFlags))
            return 1

        with patch.object(
            controller.ctypes.windll.user32,
            "GetSystemMetrics",
            side_effect=lambda metric: {76: -1920, 77: -1080, 78: 5760, 79: 3240}[metric],
        ):
            with patch.object(controller.pydirectinput, "SendInput", side_effect=send):
                controller._virtual_pointer_move(-1920, -1080)
                controller._virtual_pointer_move(3839, 2159)
        self.assertEqual([(0, 0, 0xC001), (65535, 65535, 0xC001)], events)

    def test_readback_is_required_and_logged_after_one_dispatch(self):
        backend = RecordingInput(self.backend())
        with patch("diablo2.common.controller._virtual_pointer_move", return_value=(-1920, -1080, 5760, 3240)) as dispatch:
            with patch("diablo2.common.controller.pydirectinput", Mock()) as driver:
                driver.position.side_effect = [(0, 0), (2280, 1256)]
                backend.move(2280, 1256)
        dispatch.assert_called_once_with(2280, 1256)
        event = backend.events[-1]
        self.assertTrue(event["sent"])
        self.assertEqual((2280, 1256), event["pointer_evidence"]["observed"])
        self.assertEqual((-1920, -1080, 5760, 3240), event["pointer_evidence"]["desktop"])

    def test_wrong_cursor_never_becomes_a_sent_move_and_is_not_retried(self):
        backend = RecordingInput(self.backend())
        with patch("diablo2.common.controller._virtual_pointer_move", return_value=(0, 0, 5760, 3240)) as dispatch:
            with patch("diablo2.common.controller.pydirectinput", Mock()) as driver:
                driver.position.return_value = (1919, 1079)
                with self.assertRaisesRegex(RuntimeError, "mapping mismatch"):
                    backend.move(2280, 1256)
        dispatch.assert_called_once()
        self.assertFalse(backend.events[-1]["sent"])
        self.assertIsNone(backend.backend.pointer_evidence)

    def test_send_failure_does_not_start_readback(self):
        with patch.object(
            controller.ctypes.windll.user32, "GetSystemMetrics", side_effect=lambda metric: {76: 0, 77: 0, 78: 3840, 79: 2160}[metric]
        ):
            with patch.object(controller.pydirectinput, "SendInput", return_value=0):
                with self.assertRaisesRegex(RuntimeError, "not dispatched"):
                    controller._virtual_pointer_move(2280, 1256)


if __name__ == "__main__":
    unittest.main()
