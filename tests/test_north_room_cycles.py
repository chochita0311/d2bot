import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from diablo2.runs.summoner.main_run import run


class NorthRoomCycleTests(unittest.TestCase):
    def batch(self, *, recovery=False, outbound_resume=False, statuses=("north_room_closed", "north_room_closed")):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "batch"
            calls, outcomes = [], iter(statuses)

            def run_room(config, **options):
                calls.append(options)
                target = options["output"]
                target.mkdir()
                status = next(outcomes)
                (target / "report.json").write_text(json.dumps(dict(status=status)), encoding="utf-8")
                return 0 if status == "north_room_closed" else 2

            with patch("diablo2.runs.summoner.main_run.run_once", side_effect=run_room), patch(
                "diablo2.runs.summoner.main_run.read_probe_signals", return_value=object()
            ) as read:
                result = run(
                    object(),
                    north_cycles=2,
                    live=True,
                    output=output,
                    continuous=True,
                    resume_arcane=recovery or outbound_resume,
                    resume_town=False,
                    return_from="north" if recovery else None,
                    emit=lambda _: None,
                )
            summary = json.loads((output / "report.json").read_text(encoding="utf-8"))
            read.assert_not_called()
            return result, summary, calls

    def test_new_room_starts_only_after_act1_and_character_select_finish(self):
        result, summary, calls = self.batch()
        self.assertEqual(0, result)
        self.assertEqual(2, summary["completed"])
        self.assertEqual(0, summary["acceptance_rooms"])
        self.assertTrue(all(call["finish_room"] for call in calls))
        self.assertIs(calls[0]["signals"], calls[1]["signals"])
        self.assertFalse(calls[1]["resume_arcane"])
        self.assertIsNone(calls[1]["return_from"])

    def test_existing_room_recovery_is_closed_without_counting_new_room_success(self):
        result, summary, calls = self.batch(recovery=True, statuses=("north_room_closed",) * 3)
        self.assertEqual(0, result)
        self.assertTrue(summary["recovery_closed"])
        self.assertEqual(2, summary["completed"])
        self.assertEqual([True, False, False], [room["recovery"] for room in summary["rooms"]])
        self.assertEqual("north", calls[0]["return_from"])
        self.assertFalse(calls[1]["resume_arcane"])

    def test_resuming_outbound_also_excludes_the_existing_room_from_new_room_count(self):
        result, summary, calls = self.batch(outbound_resume=True, statuses=("north_room_closed",) * 3)
        self.assertEqual(0, result)
        self.assertTrue(summary["recovery_closed"])
        self.assertEqual(2, summary["completed"])
        self.assertEqual([True, False, False], [room["recovery"] for room in summary["rooms"]])
        self.assertTrue(calls[0]["resume_arcane"])
        self.assertIsNone(calls[0]["return_from"])
        self.assertFalse(calls[1]["resume_arcane"])

    def test_unfinished_navigation_or_encounter_never_creates_next_room(self):
        result, summary, calls = self.batch(statuses=("summoner_platform_observed",))
        self.assertEqual(2, result)
        self.assertEqual(0, summary["completed"])
        self.assertEqual(1, len(calls))
        self.assertEqual("needs_navigation_or_encounter_review", summary["status"])

    def test_dry_and_invalid_scope_never_start_input_reader_or_room(self):
        with patch("diablo2.runs.summoner.main_run.run_once") as room, patch("diablo2.runs.summoner.main_run.read_probe_signals") as read:
            self.assertEqual(0, run(object(), north_cycles=2, live=False, continuous=True, emit=lambda _: None))
            self.assertEqual(0, run(object(), north_cycles=5, live=False, continuous=True, emit=lambda _: None))
            for options in (
                dict(north_cycles=0),
                dict(north_cycles=6),
                dict(north_cycles=True),
                dict(north_cycles=2, all_wings=True),
                dict(north_cycles=2, wing="east"),
            ):
                with self.assertRaises(ValueError):
                    run(object(), continuous=True, **options)
            room.assert_not_called()
            read.assert_not_called()
