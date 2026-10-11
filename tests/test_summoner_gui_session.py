import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

from diablo2.runs.summoner.gui_session import SummonerGuiSession


class SummonerGuiSessionTests(unittest.TestCase):
    def setUp(self):
        self.config = SimpleNamespace(characters={"selected": object(), "flash": object()})
        self.session = SummonerGuiSession(self.config)

    def finish(self):
        self.session._thread.join(timeout=2)
        self.assertFalse(self.session.is_running)

    def final_event(self):
        events = []
        while not self.session.events.empty():
            events.append(self.session.events.get_nowait())
        return events[-1]

    def test_latest_engine_receives_gui_settings_and_automatic_options(self):
        with patch("diablo2.runs.summoner.main_run.run", return_value=0) as runner:
            output = self.session.start(repeat_count=3, difficulty="nightmare", character_key="selected")
            self.finish()
        args, options = runner.call_args
        self.assertEqual((self.config,), args)
        self.assertEqual(
            dict(
                north_cycles=3,
                live=True,
                resume_arcane=False,
                casts=1,
                difficulty="nightmare",
                output=output,
                continuous=True,
                field_seconds=60,
                segments=4,
                automatic=True,
                stop_event=self.session._stop_event,
                character_key="selected",
                engage_monsters=True,
                automap="on",
                emit=self.session._queue_log,
            ),
            options,
        )
        self.assertEqual(Path("recordings/summoner/evidence"), output.parent)
        self.assertEqual("complete", self.final_event().payload["status"])

    def test_blank_repeat_defaults_to_one_and_each_start_has_a_unique_output(self):
        outputs = []
        with patch("diablo2.runs.summoner.main_run.run", return_value=0) as runner:
            for _ in range(2):
                outputs.append(self.session.start(character_key="selected"))
                self.finish()
        self.assertEqual([1, 1], [call[1]["north_cycles"] for call in runner.call_args_list])
        self.assertNotEqual(*outputs)

    def test_invalid_repeat_counts_are_rejected_before_worker_start(self):
        for count in (0, -1, 6, 1.5, "2", True):
            with self.subTest(count=count):
                with self.assertRaisesRegex(ValueError, "Repeat Count must be from 1 to 5"):
                    self.session.start(repeat_count=count, character_key="selected")
                self.assertIsNone(self.session._thread)

    def test_character_and_difficulty_must_be_explicit_valid_gui_choices(self):
        with self.assertRaisesRegex(ValueError, "Select a configured character"):
            self.session.start()
        with self.assertRaisesRegex(ValueError, "valid Summoner difficulty"):
            self.session.start(difficulty="unknown", character_key="selected")

    def test_stop_is_immediate_and_worker_remains_owned_until_it_finishes(self):
        entered, release = threading.Event(), threading.Event()

        def blocked_run(config, **options):
            entered.set()
            release.wait(2)
            return 0

        with patch("diablo2.runs.summoner.main_run.run", side_effect=blocked_run):
            self.session.start(character_key="selected")
            self.assertTrue(entered.wait(1))
            try:
                self.session.stop()
                self.assertTrue(self.session._stop_event.is_set())
                self.assertTrue(self.session.is_running)
                with self.assertRaisesRegex(RuntimeError, "already running"):
                    self.session.start(character_key="selected")
                with self.assertRaisesRegex(RuntimeError, "before changing"):
                    self.session.update_config(self.config)
            finally:
                release.set()
                self.finish()
        self.assertEqual("stopped", self.final_event().payload["status"])

    def test_close_requests_stop_without_joining_the_worker(self):
        self.session._thread = Mock()
        self.session._thread.is_alive.return_value = True
        self.session.close()
        self.assertTrue(self.session._stop_event.is_set())
        self.session._thread.join.assert_not_called()

    def test_runtime_stop_hotkey_is_reported_as_cancellation(self):
        def hotkey_run(config, **options):
            options["stop_event"].set()
            return 2

        with patch("diablo2.runs.summoner.main_run.run", side_effect=hotkey_run):
            self.session.start(character_key="selected")
            self.finish()
        self.assertEqual("stopped", self.final_event().payload["status"])

    def test_incomplete_exit_does_not_emit_success(self):
        with patch("diablo2.runs.summoner.main_run.run", return_value=2):
            self.session.start(character_key="selected")
            self.finish()
        event = self.final_event()
        self.assertEqual("incomplete", event.payload["status"])
        self.assertEqual("warning", event.level)
        self.assertEqual(2, event.payload["exit_code"])

    def test_exception_is_reported_as_failure_and_releases_the_worker(self):
        with patch("diablo2.runs.summoner.main_run.run", side_effect=RuntimeError("arrival missing")):
            self.session.start(character_key="selected")
            self.finish()
        event = self.final_event()
        self.assertEqual("failed", event.payload["status"])
        self.assertEqual("error", event.level)
        self.assertIn("arrival missing", event.message)

    def test_runtime_messages_are_queued_without_tk_access(self):
        def fake_run(config, **options):
            options["emit"]("observed hub")
            return 0

        with patch("diablo2.runs.summoner.main_run.run", side_effect=fake_run):
            self.session.start(character_key="selected")
            self.finish()
        messages = []
        while not self.session.events.empty():
            messages.append(self.session.events.get_nowait().message)
        self.assertIn("observed hub", messages)


if __name__ == "__main__":
    unittest.main()
