import unittest
from dataclasses import replace

from diablo2.common.survival import ObservationStamp
from diablo2.runs.summoner.cycle import COMMANDS, CycleLimits, CycleObservation, SummonerCycle, WINGS


class SummonerCycleTests(unittest.TestCase):
    def setUp(self):
        self.now = 10.0
        self.sequence = -1
        self.limits = CycleLimits(1, 100, 500, {kind: 5 for kind in COMMANDS})
        self.cycle = self.make_cycle()

    def make_cycle(self, count=1, **changes):
        return SummonerCycle("flash", tuple("room-" + str(index) for index in range(count)), replace(self.limits, **changes), started_at=10)

    def observation(self, event="unknown", **changes):
        self.now += 0.1
        self.sequence += 1
        return replace(
            CycleObservation(
                ObservationStamp("flash", self.cycle.room_id, self.sequence, self.now),
                0,
                event,
                self.cycle.pending.command_id if self.cycle.pending else None,
            ),
            **changes,
        )

    def step(self, event="unknown", **changes):
        return self.cycle.step(self.observation(event, **changes), self.now, focused=True)

    def prepare(self):
        for event, command in (
            ("character_select", "create_room"),
            ("act1_ready", "enter_arcane"),
            ("arcane_ready", "prepare_field"),
            ("field_ready", "search_wing"),
        ):
            self.assertEqual(self.step(event).command.kind, command)

    def finish_room(self, boss_index=0, keys=0):
        self.prepare()
        for index in range(boss_index):
            self.assertEqual(self.cycle.pending.wing, WINGS[index])
            self.assertEqual(self.step("wing_empty_confirmed").command.kind, "return_hub")
            self.assertEqual(self.step("hub_ready").command.kind, "search_wing")
        self.assertEqual(self.cycle.pending.wing, WINGS[boss_index])
        for event, command in (
            ("summoner_alive", "fight_boss"),
            ("summoner_dead_confirmed", "inspect_loot"),
        ):
            self.assertEqual(self.step(event).command.kind, command)
        self.assertEqual(self.step("loot_complete", keys_dropped=keys, keys_acquired=keys).command.kind, "return_hub")
        for event, command in (("hub_ready", "return_act1"), ("act1_ready", "restock"), ("restock_complete", "exit_room")):
            self.assertEqual(self.step(event).command.kind, command)
        self.assertEqual(len(self.cycle.results), self.cycle.room_index)
        return self.step("character_select")

    def test_zero_key_first_wing_run_finishes_only_after_act1_restock_and_select(self):
        result = self.finish_room()
        self.assertEqual((result.action, result.reason), ("release", "complete"))
        self.assertTrue(self.cycle.done)
        self.assertEqual(len(self.cycle.results), 1)
        room = self.cycle.results[0]
        self.assertEqual((room.outcome, room.boss_wing, room.keys_dropped, room.keys_acquired), ("normal", "north", 0, 0))
        self.assertEqual(room.checked_wings, ())
        self.assertTrue(room.room_entered)
        self.step("character_select")
        self.assertEqual(len(self.cycle.results), 1)

    def test_last_wing_requires_each_central_return_and_counts_multiple_keys(self):
        self.finish_room(3, 2)
        room = self.cycle.results[0]
        self.assertEqual(room.checked_wings, ("north", "east", "south"))
        self.assertEqual((room.boss_wing, room.keys_dropped, room.keys_acquired), ("west", 2, 2))

    def test_30_room_replay_never_creates_room_31_and_results_are_unique(self):
        self.cycle = self.make_cycle(30)
        for index in range(30):
            if index:
                # 다음 방 생성은 직전 방의 선택 화면 확인 때 단 한 번 요청됐다.
                self.assertEqual(self.cycle.pending.kind, "create_room")
                self.assertEqual(self.step("act1_ready").command.kind, "enter_arcane")
                self.step("arcane_ready")
                self.step("field_ready")
                self.step("summoner_alive")
                self.step("summoner_dead_confirmed")
                self.step("loot_complete")
                self.step("hub_ready")
                self.step("act1_ready")
                self.step("restock_complete")
                last = self.step("character_select")
            else:
                last = self.finish_room()
            self.assertEqual(len(self.cycle.results), index + 1)
        self.assertEqual(last.action, "release")
        self.assertIsNone(self.cycle.pending)
        self.assertEqual(len({room.room_id for room in self.cycle.results}), 30)
        self.assertTrue(all(room.outcome == "normal" for room in self.cycle.results))
        self.step("character_select")
        self.assertEqual(len(self.cycle.results), 30)

    def test_unknown_initial_screen_or_missing_screen_never_requests_creation(self):
        self.assertEqual(self.cycle.step(None, self.now, focused=True).action, "release")
        self.assertEqual(self.step("act1_ready").action, "release")
        self.assertIsNone(self.cycle.pending)

    def test_unknown_or_wrong_phase_confirmation_cannot_skip_stage_or_repeat_command(self):
        self.step("character_select")
        pending = self.cycle.pending
        for event in ("unknown", "arcane_ready", "field_ready", "summoner_dead_confirmed", "loot_complete", "character_select"):
            self.assertEqual(self.step(event).action, "wait")
            self.assertIs(self.cycle.pending, pending)
        self.assertEqual(self.cycle.step(None, self.now, focused=True).action, "wait")

    def test_old_command_result_after_new_request_does_not_advance(self):
        self.step("character_select")
        old = self.cycle.pending.command_id
        self.step("act1_ready")
        current = self.cycle.pending
        self.assertEqual(self.step("arcane_ready", command_id=old).reason, "command_unconfirmed")
        self.assertIs(self.cycle.pending, current)
        self.assertEqual(self.step("arcane_ready").command.kind, "prepare_field")

    def test_duplicate_frame_capture_before_request_and_previous_generation_are_rejected(self):
        self.step("character_select", generation=1)
        command = self.cycle.pending
        good = self.observation("act1_ready", generation=1)
        for stale in (
            replace(good, stamp=replace(good.stamp, sequence=command.source_sequence)),
            replace(good, stamp=replace(good.stamp, observed_at=command.issued_at - 0.01)),
            replace(good, generation=0),
        ):
            self.assertEqual(self.cycle.step(stale, self.now, focused=True).action, "wait")
            self.assertIs(self.cycle.pending, command)
        self.assertEqual(self.cycle.step(good, self.now, focused=True).command.kind, "enter_arcane")

    def test_new_generation_blocks_late_result_even_if_command_id_matches(self):
        self.step("character_select")
        self.step("unknown", generation=2)
        self.assertEqual(self.step("act1_ready", generation=1).reason, "observation_not_newer")
        self.assertEqual(self.step("act1_ready", generation=2).command.kind, "enter_arcane")

    def test_all_empty_wings_is_review_after_hub_confirmation_and_never_normal(self):
        self.prepare()
        for wing in WINGS:
            self.assertEqual(self.cycle.pending.wing, wing)
            self.step("wing_empty_confirmed")
            last = self.step("hub_ready")
        self.assertEqual((last.action, last.reason), ("release", "summoner_not_found"))
        self.assertEqual(self.cycle.results[0].outcome, "review_required")
        self.assertEqual(self.cycle.results[0].checked_wings, WINGS)

    def test_unconfirmed_boss_death_or_loot_cannot_resume_search(self):
        self.prepare()
        self.step("summoner_alive")
        for event in ("unknown", "wing_empty_confirmed", "loot_complete", "hub_ready"):
            self.assertEqual(self.step(event).action, "wait")
            self.assertEqual(self.cycle.pending.kind, "fight_boss")
        self.step("summoner_dead_confirmed")
        self.assertEqual(self.step("hub_ready").action, "wait")
        self.assertEqual(self.cycle.pending.kind, "inspect_loot")

    def test_uncollected_key_requires_review_and_does_not_restart(self):
        self.cycle = self.make_cycle(30)
        self.prepare()
        self.step("summoner_alive")
        self.step("summoner_dead_confirmed")
        result = self.step("loot_complete", keys_dropped=2, keys_acquired=1)
        self.assertEqual(result.reason, "key_pickup_incomplete")
        self.assertEqual(self.cycle.results[0].keys_acquired, 1)
        self.step("character_select")
        self.assertIsNone(self.cycle.pending)
        self.assertEqual(len(self.cycle.results), 1)

    def test_stop_focus_failure_and_wrong_context_cancel_once(self):
        for problem in ("stop", "focus", "failed", "character", "room", "future", "stale"):
            with self.subTest(problem=problem):
                self.cycle = self.make_cycle(30)
                self.step("character_select")
                frame = self.observation("failed" if problem == "failed" else "act1_ready")
                if problem in ("character", "room", "future", "stale"):
                    field, value = {
                        "character": ("character_id", "other"),
                        "room": ("room_id", "other"),
                        "future": ("observed_at", self.now + 1),
                        "stale": ("observed_at", self.now - 2),
                    }[problem]
                    frame = replace(frame, stamp=replace(frame.stamp, **{field: value}))
                result = self.cycle.step(frame, self.now, focused=problem != "focus", stopped=problem == "stop")
                self.assertEqual(result.action, "release")
                self.assertTrue(self.cycle.done)
                self.assertEqual(self.cycle.results[0].outcome, "review_required")
                self.assertFalse(self.cycle.results[0].room_entered)
                self.cycle.step(None, self.now, focused=True)
                self.assertEqual(len(self.cycle.results), 1)
                self.assertIsNone(self.cycle.pending)

    def test_stage_room_total_deadlines_and_clock_regression_release(self):
        for limits, time, reason in (
            ({}, 15.1, "stage_timeout:create_room"),
            ({"room_seconds": 0.5}, 10.5, "room_timeout"),
            ({"total_seconds": 0.4}, 10.4, "total_timeout"),
            ({}, 9, "clock_regressed"),
        ):
            with self.subTest(reason=reason):
                self.cycle = self.make_cycle(**limits)
                self.now = 10
                self.step("character_select")
                result = self.cycle.step(None, time, focused=True)
                self.assertEqual((result.action, result.reason), ("release", reason))
                self.assertTrue(self.cycle.done)

    def test_explicit_limits_context_and_key_counts_are_validated(self):
        for changes in ({"screen_age": 0}, {"total_seconds": float("inf")}, {"stage_seconds": {"create_room": 1}}):
            with self.assertRaises(ValueError):
                replace(self.limits, **changes)
        for rooms in ((), ("same", "same"), ("",), None):
            with self.assertRaises(ValueError):
                SummonerCycle("flash", rooms, self.limits, started_at=10)
        frame = self.observation("loot_complete")
        for changes in ({"keys_dropped": 1, "keys_acquired": 2}, {"keys_dropped": True}, {"event": "unknown", "keys_dropped": 1}):
            with self.assertRaises(ValueError):
                replace(frame, **changes)
        stages = {kind: 5 for kind in COMMANDS}
        immutable = CycleLimits(1, 100, 500, stages)
        stages["create_room"] = 999
        self.assertEqual(immutable.stage_seconds["create_room"], 5)
        with self.assertRaises(ValueError):
            self.cycle.step(None, self.now, focused="yes")


if __name__ == "__main__":
    unittest.main()
