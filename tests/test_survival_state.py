import copy
import json
import unittest
from dataclasses import replace
from pathlib import Path

from diablo2.common.config import apply_character_selection, get_active_character_profile, load_config
from diablo2.common.survival import (
    CharacterSurvivalPolicy,
    CharacterSurvivalState,
    ObservationStamp,
    SafetyObservation,
    TimedBuffPolicy,
)


ROOT = Path(__file__).resolve().parents[1]


def flash_policy():
    return load_config(ROOT / "config").characters["flash"].survival


class SurvivalPolicyTests(unittest.TestCase):
    def test_actual_config_uses_latest_sequence_and_character_owned_values(self):
        config = load_config(ROOT / "config")
        flash = config.characters["flash"]
        self.assertEqual(flash.actions.pre_run_buff_order, ["w", "a", "a", "s", "f1", "d", "w"])
        self.assertEqual((flash.survival.life_potion_below, flash.survival.mana_potion_below), (0.5, 0.1))
        self.assertEqual(flash.survival.belt_toggle_key, "`")
        self.assertEqual(flash.survival.buffs[0], TimedBuffPolicy("battle_orders", "s", 150, 10))
        self.assertIsNone(config.characters["abyss_knight"].survival)
        self.assertIsNone(config.characters["constantin"].survival)
        apply_character_selection(config, "flash")
        self.assertIs(get_active_character_profile(config), flash)
        apply_character_selection(config, "constantin")
        self.assertIsNone(get_active_character_profile(config).survival)

    def test_other_character_has_its_own_belt_keys_ratios_and_duration(self):
        policy = CharacterSurvivalPolicy.from_dict(
            {
                "life_potion_below": 0.8,
                "mana_potion_below": 0.2,
                "potion_kind": "healing",
                "replenish_at_or_below": 1,
                "exit_on_empty": False,
                "belt_toggle_key": "b",
                "belt_columns": [{"column": 2, "key": "q", "kind": "healing", "capacity": 2, "target_count": 2}],
                "buffs": [{"name": "armor", "key": "e", "duration_seconds": 80, "renew_before_seconds": 5}],
            }
        )
        state = CharacterSurvivalState("other", "room", policy)
        state.observe_belt(ObservationStamp("other", "room", 1, 0), {2: ("healing", 2)})
        observation = SafetyObservation(ObservationStamp("other", "room", 2, 1), 0.7, 1, "field", True)
        result = state.decide(observation, 1, max_observation_age_seconds=1, max_belt_age_seconds=2)
        self.assertEqual((result.action, result.potion_key), ("use_potion", "q"))
        state.observe_belt(ObservationStamp("other", "room", 3, 2), {2: (None, 0)})
        observation = replace(observation, stamp=ObservationStamp("other", "room", 4, 2))
        self.assertEqual(state.decide(observation, 2, max_observation_age_seconds=1, max_belt_age_seconds=2).action, "hold")

    def test_invalid_policy_values_are_rejected(self):
        raw = json.loads((ROOT / "config/characters/characters.json").read_text(encoding="utf-8"))["characters"]["flash"]["survival"]
        cases = [
            ("life_potion_below", True),
            ("mana_potion_below", float("nan")),
            ("life_potion_below", 0),
            ("mana_potion_below", 1.1),
            ("life_potion_below", 10**500),
            ("replenish_at_or_below", 13),
            ("replenish_at_or_below", 6.0),
            ("exit_on_empty", 1),
            ("belt_toggle_key", ""),
            ("unknown_option", 1),
            ("belt_columns", []),
        ]
        for key, value in cases:
            with self.subTest(key=key, value=value):
                candidate = copy.deepcopy(raw)
                candidate[key] = value
                with self.assertRaises(ValueError):
                    CharacterSurvivalPolicy.from_dict(candidate)
        for mutate in (
            lambda candidate: candidate["belt_columns"].append(candidate["belt_columns"][0]),
            lambda candidate: candidate["belt_columns"][0].update(target_count=5),
            lambda candidate: candidate["belt_columns"][0].update(target_count=None),
            lambda candidate: candidate["belt_columns"][0].update(column=True),
            lambda candidate: candidate["buffs"][0].update(duration_seconds=0),
            lambda candidate: candidate["buffs"][0].update(renew_before_seconds=150),
            lambda candidate: candidate["buffs"].append(candidate["buffs"][0]),
        ):
            candidate = copy.deepcopy(raw)
            mutate(candidate)
            with self.assertRaises(ValueError):
                CharacterSurvivalPolicy.from_dict(candidate)


class SurvivalStateTests(unittest.TestCase):
    def setUp(self):
        self.policy = flash_policy()
        self.state = CharacterSurvivalState("flash", "room-1", self.policy)
        self.assertTrue(self.state.observe_belt(self.stamp(1, 0), self.belt(12)))
        self.assertTrue(self.state.confirm_buff(self.stamp(2, 0), "battle_orders", "field", True))

    def stamp(self, sequence, now, character="flash", room="room-1"):
        return ObservationStamp(character, room, sequence, now)

    @staticmethod
    def belt(potions, scrolls=4):
        result = {}
        for column in (1, 2, 3):
            count = min(potions, 4)
            result[column] = ("purple_potion" if count else None, count)
            potions -= count
        result[4] = ("town_portal_scroll" if scrolls else None, scrolls)
        return result

    def decide(self, now=1, life=1, mana=1, location="field", clear=True, state=None, sequence=1000, belt_age=1000):
        observation = SafetyObservation(self.stamp(sequence, now), life, mana, location, clear)
        return (state or self.state).decide(observation, now, max_observation_age_seconds=1, max_belt_age_seconds=belt_age)

    def test_exact_health_and_mana_boundaries_and_single_potion_request(self):
        self.assertEqual(self.decide(life=0.5, mana=0.1).action, "continue")
        for life, mana in ((0.499, 1), (1, 0.099), (0.1, 0.01)):
            with self.subTest(life=life, mana=mana):
                decision = self.decide(life=life, mana=mana)
                self.assertEqual((decision.action, decision.potion_key), ("use_potion", "1"))
                self.assertEqual(self.state.potions_remaining, 12)

    def test_resource_levels_distinguish_replenishment_and_exit(self):
        for count, action in ((7, "continue"), (6, "replenish"), (1, "replenish"), (0, "exit")):
            with self.subTest(count=count):
                self.assertTrue(self.state.observe_belt(self.stamp(20 - count, 1), self.belt(count)))
                decision = self.decide()
                self.assertEqual(decision.action, action)
                self.assertEqual(decision.replenishment_required, count <= 6)
        self.assertEqual(self.decide(life=None, mana=None).action, "exit")

    def test_recovery_precedes_buff_and_replenishment(self):
        self.state.observe_belt(self.stamp(3, 140), self.belt(6))
        decision = self.decide(now=140, life=0.2, location="town", clear=False)
        self.assertEqual(decision.action, "use_potion")
        self.assertTrue(decision.replenishment_required)
        self.state.observe_belt(self.stamp(4, 150), self.belt(0))
        self.assertEqual(self.decide(now=150, life=0.2, clear=False).action, "exit")

    def test_first_nonempty_configured_potion_column_is_used(self):
        contents = self.belt(0)
        contents[2] = ("purple_potion", 2)
        self.state.observe_belt(self.stamp(3, 1), contents)
        self.assertEqual(self.decide(life=0.2).potion_key, "2")

    def test_timer_uses_confirmed_cast_and_keeps_elapsed_time_during_pause(self):
        self.assertEqual(self.decide(now=139.999).action, "continue")
        self.assertEqual(self.decide(now=140).action, "buff")
        self.assertEqual(self.decide(now=150, clear=False).reason, "buff_unsafe")
        self.assertFalse(self.state.confirm_buff(self.stamp(3, 145), "battle_orders", "town", True))
        self.assertFalse(self.state.confirm_buff(self.stamp(4, 145), "battle_orders", "field", False))
        self.assertEqual(self.decide(now=500).action, "buff")
        self.assertTrue(self.state.confirm_buff(self.stamp(5, 500), "battle_orders", "field", True))
        self.assertEqual(self.decide(now=639.999).action, "continue")
        self.assertEqual(self.decide(now=640).action, "buff")

    def test_multiple_buff_cast_times_have_independent_deadlines(self):
        policy = replace(self.policy, buffs=(TimedBuffPolicy("armor", "d", 600, 10), TimedBuffPolicy("battle_orders", "s", 150, 10)))
        state = CharacterSurvivalState("flash", "room-1", policy)
        state.observe_belt(self.stamp(1, 0), self.belt(12))
        state.confirm_buff(self.stamp(2, 0), "armor", "field", True)
        state.confirm_buff(self.stamp(3, 10), "battle_orders", "field", True)
        self.assertEqual(self.decide(now=149.999, state=state).action, "continue")
        self.assertEqual(self.decide(now=150, state=state).buffs_due, ("battle_orders",))
        self.assertEqual(self.decide(now=590, state=state).buffs_due, ("armor", "battle_orders"))

    def test_initial_and_renewal_buffs_require_clear_field(self):
        fresh = CharacterSurvivalState("flash", "room-1", self.policy)
        fresh.observe_belt(self.stamp(1, 0), self.belt(12))
        for state, now in ((fresh, 1), (self.state, 140)):
            for location, clear, reason in (
                ("town", True, "buff_requires_field"),
                ("unknown", True, "buff_requires_field"),
                ("field", False, "buff_unsafe"),
                ("field", None, "buff_unsafe"),
            ):
                with self.subTest(now=now, location=location, clear=clear):
                    self.assertEqual(self.decide(now=now, location=location, clear=clear, state=state).reason, reason)
            self.assertEqual(self.decide(now=now, state=state).action, "buff")

    def test_confirmed_resource_changes_deduplicate_and_reconcile(self):
        used = self.stamp(3, 1)
        self.assertTrue(self.state.confirm_resource_change(used, 1, "purple_potion", -1))
        self.assertFalse(self.state.confirm_resource_change(used, 1, "purple_potion", -1))
        self.assertEqual(self.state.potions_remaining, 11)
        self.assertEqual(self.decide().deficits, ((1, 1),))
        self.assertFalse(self.state.observe_belt(self.stamp(2, 0), self.belt(12)))
        self.assertTrue(self.state.confirm_resource_change(self.stamp(4, 2), 1, "purple_potion", 1))
        self.assertEqual(self.state.potions_remaining, 12)
        self.assertEqual(self.decide(now=2).deficits, ())

    def test_null_scroll_target_uses_initial_observed_amount(self):
        state = CharacterSurvivalState("flash", "room-1", self.policy)
        state.observe_belt(self.stamp(1, 0), self.belt(12, scrolls=2))
        state.confirm_buff(self.stamp(2, 0), "battle_orders", "field", True)
        state.confirm_resource_change(self.stamp(3, 1), 4, "town_portal_scroll", -1)
        self.assertEqual(self.decide(state=state).deficits, ((4, 1),))
        self.assertEqual(state.potions_remaining, 12)

    def test_count_conflicts_and_wrong_item_kinds_require_new_observation(self):
        self.assertFalse(self.state.confirm_resource_change(self.stamp(3, 1), 1, "purple_potion", 1))
        self.assertIsNone(self.state.potions_remaining)
        self.assertEqual(self.decide().reason, "belt_unknown")
        wrong = self.belt(12)
        wrong[1] = ("mana_potion", 4)
        self.assertFalse(self.state.observe_belt(self.stamp(4, 2), wrong))
        self.assertIsNone(self.state.belt_counts)
        self.assertTrue(self.state.observe_belt(self.stamp(5, 3), self.belt(12)))
        self.assertEqual(self.decide(now=3).action, "continue")

    def test_partial_snapshot_and_manual_changes_never_mean_empty_or_full(self):
        partial = self.belt(0)
        del partial[4]
        self.assertFalse(self.state.observe_belt(self.stamp(3, 1), partial))
        self.assertEqual(self.decide().reason, "belt_unknown")
        self.state.observe_belt(self.stamp(4, 2), self.belt(7))
        self.state.invalidate_belt(self.stamp(5, 3))
        self.assertEqual(self.decide(now=3).reason, "belt_unknown")
        self.state.observe_belt(self.stamp(6, 4), self.belt(6))
        self.assertEqual(self.decide(now=4).action, "replenish")

    def test_old_context_reordered_time_and_reordered_view_are_rejected(self):
        self.assertFalse(self.state.observe_belt(self.stamp(3, 1, room="old"), self.belt(0)))
        self.assertFalse(self.state.confirm_resource_change(self.stamp(4, 1, character="other"), 1, "purple_potion", -4))
        self.assertFalse(self.state.confirm_buff(self.stamp(5, 1, room="old"), "battle_orders", "field", True))
        self.state.observe_belt(self.stamp(6, 10), self.belt(12))
        self.assertFalse(self.state.confirm_resource_change(self.stamp(7, 9), 1, "purple_potion", -1))
        self.assertEqual(self.state.potions_remaining, 12)
        self.assertEqual(self.decide(now=10, sequence=5).reason, "observation_precedes_state")

    def test_freshness_and_unknown_hud_cannot_authorize_progress(self):
        self.assertEqual(self.decide(life=None).reason, "hud_unknown")
        self.assertEqual(self.decide(now=2, belt_age=1).reason, "belt_stale")
        observation = SafetyObservation(self.stamp(1000, 0), 1, 1, "field", True)
        self.assertEqual(
            self.state.decide(observation, 2, max_observation_age_seconds=1, max_belt_age_seconds=3).reason, "observation_stale"
        )
        observation = replace(observation, stamp=self.stamp(1000, 3))
        self.assertEqual(
            self.state.decide(observation, 2, max_observation_age_seconds=1, max_belt_age_seconds=3).reason, "observation_stale"
        )
        with self.assertRaises(ValueError):
            self.state.decide(observation, 3, max_observation_age_seconds=0, max_belt_age_seconds=3)

    def test_new_room_and_missing_profile_do_not_inherit_flash_state(self):
        state = CharacterSurvivalState("flash", "room-2", self.policy)
        observation = SafetyObservation(self.stamp(1000, 1, room="room-2"), 1, 1, "field", True)
        self.assertEqual(state.decide(observation, 1, max_observation_age_seconds=1, max_belt_age_seconds=2).reason, "belt_unknown")
        unknown = CharacterSurvivalState("other", "room-2", None)
        self.assertEqual(
            unknown.decide(observation, 1, max_observation_age_seconds=1, max_belt_age_seconds=2).reason, "policy_unconfigured"
        )
        self.assertEqual(self.decide(state=state).reason, "wrong_context")
        with self.assertRaises(AttributeError):
            self.state.room_id = "room-2"

    def test_returned_belt_counts_are_not_mutable_tracker_storage(self):
        counts = self.state.belt_counts
        counts[1] = 0
        self.assertEqual(self.state.potions_remaining, 12)


if __name__ == "__main__":
    unittest.main()
