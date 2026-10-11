import unittest
from pathlib import Path
from queue import Queue
from types import SimpleNamespace
from unittest.mock import Mock, patch

from diablo2.runs.summoner.gui_session import SummonerEvent
from diablo2.ui.gui import D2BotControlPanel


class Value:
    def __init__(self, value):
        self.value = value

    def get(self):
        return self.value

    def set(self, value):
        self.value = value


class FakeAction:
    def __init__(self):
        self.is_running = False
        self.events = Queue()
        self.request_stop = Mock()
        self.stop = Mock(side_effect=AssertionError("Tk must not join a worker"))
        self.start = Mock()


class GuiSummonerTests(unittest.TestCase):
    def setUp(self):
        self.panel = D2BotControlPanel.__new__(D2BotControlPanel)
        panel = self.panel
        panel.root = Mock()
        panel._closing = False
        panel._recording_cleanup = None
        panel._preserve_current_geometry = Mock()
        panel._apply_runtime_config = Mock()
        panel._append_log = Mock()
        panel.repeat_count_var = Value("")
        panel.difficulty_var = Value("nightmare")
        panel.character_var = Value("Chosen hero")
        panel.town_loop_var = Value(False)
        panel.status_var = Value("Idle")
        panel.output_var = Value("")
        panel._character_display_to_key = {"Chosen hero": "selected"}
        for name in ("recording", "lifecycle", "gem", "loot", "summoner"):
            setattr(panel, name + "_session", FakeAction())
        for name in ("record", "stop", "lifecycle", "loot", "key", "gem", "stop_action"):
            setattr(panel, name + "_button", Mock())
        for name in ("character_box", "window_title_box", "backend_box"):
            setattr(panel, name, Mock())

        def start_summoner(**kwargs):
            panel.summoner_session.is_running = True
            return Path("recordings/summoner/evidence/gui-test")

        panel.summoner_session.start.side_effect = start_summoner

    def test_summoner_button_reads_all_settings_and_blank_default(self):
        self.panel.start_key_of_hate()
        self.panel.summoner_session.start.assert_called_once_with(repeat_count=None, difficulty="nightmare", character_key="selected")
        self.assertEqual("Summoner Run", self.panel.status_var.get())
        self.assertEqual(str(Path("recordings/summoner/evidence/gui-test")), self.panel.output_var.get())
        self.panel.key_button.state.assert_called_with(["disabled"])
        self.panel.gem_button.state.assert_called_with(["disabled"])
        self.panel.character_box.configure.assert_called_with(state="disabled")

    def test_explicit_repeat_count_is_forwarded(self):
        self.panel.repeat_count_var.set("5")
        self.panel.start_key_of_hate()
        self.assertEqual(5, self.panel.summoner_session.start.call_args[1]["repeat_count"])

    def test_invalid_repeat_text_is_shown_as_a_clear_error(self):
        self.panel.repeat_count_var.set("bad")
        with patch("diablo2.ui.gui.messagebox.showerror") as error:
            self.panel.start_key_of_hate()
        self.panel.summoner_session.start.assert_not_called()
        error.assert_called_once_with("Summoner Run Error", "Repeat Count must be a whole number.")

    def test_all_game_action_starters_reject_an_existing_worker(self):
        self.panel.summoner_session.is_running = True
        with patch("diablo2.ui.gui.messagebox.showerror") as error:
            for start in (
                self.panel.start_key_of_hate,
                self.panel.start_gem_summing,
                self.panel.start_loot_pickup,
                self.panel.start_run_lifecycle,
            ):
                start()
        self.assertEqual(4, error.call_count)
        for action in self.panel._action_sessions():
            action.start.assert_not_called()
        self.panel._apply_runtime_config.assert_not_called()

    def test_stop_requests_cancellation_and_keeps_inputs_disabled_until_exit(self):
        self.panel.summoner_session.is_running = True
        self.panel.stop_current_action()
        self.panel.summoner_session.request_stop.assert_called_once()
        self.panel.summoner_session.stop.assert_not_called()
        self.assertEqual("Stopping", self.panel.status_var.get())
        self.panel.key_button.state.assert_called_with(["disabled"])
        self.panel.summoner_session.is_running = False
        self.panel._refresh_action_controls()
        self.assertEqual("Stopped", self.panel.status_var.get())
        self.panel.key_button.state.assert_called_with(["!disabled"])

    def test_close_requests_stop_and_waits_through_poll_without_blocking_tk(self):
        self.panel.summoner_session.is_running = True
        self.panel._on_close()
        for action in self.panel._action_sessions():
            action.request_stop.assert_called_once()
            action.stop.assert_not_called()
        self.panel.root.destroy.assert_not_called()
        self.panel._poll_events()
        self.panel.root.destroy.assert_not_called()
        self.panel.summoner_session.is_running = False
        self.panel._poll_events()
        self.panel.root.destroy.assert_called_once()

    def test_close_moves_recording_join_off_the_tk_thread(self):
        self.panel.recording_session.is_running = True
        with patch("diablo2.ui.gui.threading.Thread") as thread:
            self.panel._on_close()
        self.assertEqual(self.panel.recording_session.stop, thread.call_args[1]["target"])
        thread.return_value.start.assert_called_once()
        self.panel.recording_session.stop.assert_not_called()

    def test_result_statuses_are_distinct_and_remain_visible_after_polling(self):
        for status, label, level in (
            ("complete", "Summoner complete", "info"),
            ("incomplete", "Incomplete", "warning"),
            ("failed", "Error", "error"),
            ("stopped", "Stopped", "info"),
        ):
            with self.subTest(status=status):
                self.panel.summoner_session.events.put(SummonerEvent(level, "result", {"status": status, "output": "evidence"}))
                self.panel._poll_events()
                self.assertEqual(label, self.panel.status_var.get())
                self.assertEqual("evidence", self.panel.output_var.get())
                self.panel._poll_events()
                self.assertEqual(label, self.panel.status_var.get())

    def test_recording_does_not_hide_an_incomplete_run(self):
        self.panel.recording_session.is_running = True
        self.panel.summoner_session.events.put(SummonerEvent("warning", "incomplete", {"status": "incomplete"}))
        self.panel._poll_events()
        self.assertEqual("Incomplete", self.panel.status_var.get())

    def test_new_start_event_supersedes_an_unread_previous_result(self):
        self.panel.summoner_session.is_running = True
        self.panel.summoner_session.events.put(SummonerEvent("warning", "old result", {"status": "incomplete"}))
        self.panel.summoner_session.events.put(SummonerEvent("info", "new run", {"status": "running"}))
        self.panel._poll_events()
        self.assertEqual("Summoner Run", self.panel.status_var.get())

    def test_unread_start_event_does_not_hide_pending_stop(self):
        self.panel.summoner_session.is_running = True
        self.panel.summoner_session.events.put(SummonerEvent("info", "run start", {"status": "running"}))
        self.panel.stop_current_action()
        self.panel._poll_events()
        self.assertEqual("Stopping", self.panel.status_var.get())


if __name__ == "__main__":
    unittest.main()
