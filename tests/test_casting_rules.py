import copy
import json
import unittest
from dataclasses import FrozenInstanceError, replace
from pathlib import Path

from diablo2.common.casting import CastingCatalog, CharacterCastingProfile
from diablo2.common.config import BotConfig, CharacterProfile, apply_character_selection, get_active_casting_estimate, load_config


ROOT = Path(__file__).resolve().parents[1]
RULE_SET = "d2r-normal-teleport-reference-v1"


def raw_catalog():
    return json.loads((ROOT / "config/game-rules/casting.json").read_text(encoding="utf-8"))["casting_rule_sets"]


def profile(fcr=105):
    return CharacterCastingProfile(RULE_SET, "teleport", "normal", fcr)


class CastingRulesTests(unittest.TestCase):
    def setUp(self):
        self.catalog = CastingCatalog.from_list(raw_catalog())

    def test_flash_config_and_selection_resolve_reported_fcr(self):
        config = load_config(ROOT / "config")
        self.assertEqual(get_active_casting_estimate(config).reason, "casting_unconfigured")
        apply_character_selection(config, "flash")
        flash = config.characters["flash"]
        result = get_active_casting_estimate(config)
        self.assertEqual((flash.character_class, flash.casting.fcr), ("sorceress", 105))
        self.assertEqual((result.action, result.frames, result.minimum_fcr), ("estimate", 8, 105))
        self.assertEqual((result.skill_id, result.form, result.fcr, result.character_class), ("teleport", "normal", 105, "sorceress"))
        self.assertEqual(result.ruleset_family, "resurrection")
        self.assertEqual(
            (result.rule_set_id, result.revision, result.evidence_status), (RULE_SET, "library-reference-2026-10-05", "reference")
        )
        self.assertEqual(result.source_url, "https://www.mannm.org/d2library/faqtoids/stattab_eng.html")
        self.assertEqual(flash.survival.buffs[0].duration_seconds, 150)
        apply_character_selection(config, "constantin")
        self.assertEqual(get_active_casting_estimate(config).action, "hold")
        for key in ("abyss_knight", "constantin"):
            self.assertIsNone(config.characters[key].character_class)
            self.assertIsNone(config.characters[key].casting)

    def test_all_source_breakpoints_and_neighboring_fcr_values(self):
        expected = {
            "sorceress": [(0, 13), (9, 12), (20, 11), (37, 10), (63, 9), (105, 8), (200, 7)],
            "paladin": [(0, 15), (9, 14), (18, 13), (30, 12), (48, 11), (75, 10), (125, 9)],
        }
        for character_class, points in expected.items():
            for index, (fcr, frames) in enumerate(points):
                for value in (fcr, fcr + 1):
                    with self.subTest(character_class=character_class, fcr=value):
                        result = self.catalog.estimate("resurrection", character_class, profile(value))
                        self.assertEqual((result.minimum_fcr, result.frames), (fcr, frames))
                if index:
                    result = self.catalog.estimate("resurrection", character_class, profile(fcr - 1))
                    self.assertEqual((result.minimum_fcr, result.frames), points[index - 1])
            self.assertEqual(self.catalog.estimate("resurrection", character_class, profile(10**300)).frames, points[-1][1])

    def test_same_fcr_has_class_specific_frames_without_interpolation(self):
        self.assertEqual(self.catalog.estimate("resurrection", "sorceress", profile(104)).frames, 9)
        self.assertEqual(self.catalog.estimate("resurrection", "sorceress", profile(105)).frames, 8)
        self.assertEqual(self.catalog.estimate("resurrection", "paladin", profile(105)).frames, 10)
        self.assertEqual(self.catalog.estimate("resurrection", "paladin", profile(124)).frames, 10)
        self.assertEqual(self.catalog.estimate("resurrection", "paladin", profile(125)).frames, 9)

    def test_two_names_of_same_class_use_one_rule(self):
        config = load_config(ROOT / "config")
        apply_character_selection(config, "flash")
        first = get_active_casting_estimate(config)
        config.characters["other_sorceress"] = replace(config.characters["flash"], display_name="Another Sorceress")
        apply_character_selection(config, "other_sorceress")
        self.assertEqual(get_active_casting_estimate(config), first)
        self.assertEqual(len(config.casting_rules.rule_sets), 1)
        self.assertEqual(len(config.casting_rules.rule_sets[0].rules), 2)

    def test_missing_and_unsupported_contexts_hold_without_fallback(self):
        cases = [
            ("resurrection", "sorceress", None, "casting_unconfigured"),
            ("resurrection", None, profile(), "class_unconfigured"),
            ("resurrection", "sorceress", profile(None), "fcr_unconfigured"),
            ("resurrection", "sorceress", replace(profile(), rule_set_id="unknown"), "rule_set_unsupported"),
            ("rotw", "sorceress", profile(), "ruleset_unsupported"),
            ("resurrection", "necromancer", profile(), "context_unsupported"),
            ("resurrection", "sorceress", replace(profile(), skill_id="lightning"), "context_unsupported"),
            ("resurrection", "sorceress", replace(profile(), form="werebear"), "context_unsupported"),
        ]
        for ruleset, character_class, casting, reason in cases:
            with self.subTest(reason=reason, character_class=character_class):
                result = self.catalog.estimate(ruleset, character_class, casting)
                self.assertEqual((result.action, result.reason, result.frames, result.minimum_fcr), ("hold", reason, None, None))
        self.assertEqual(CastingCatalog().estimate("resurrection", "sorceress", profile()).reason, "rule_set_unsupported")

    def test_missing_or_invalid_active_character_does_not_choose_another(self):
        self.assertEqual(get_active_casting_estimate(BotConfig()).reason, "character_missing")
        config = load_config(ROOT / "config")
        config.active_character = "unknown"
        self.assertEqual(get_active_casting_estimate(config).reason, "character_missing")
        config.active_character = None
        self.assertEqual(get_active_casting_estimate(config).reason, "character_missing")

    def test_character_file_without_catalog_remains_loadable(self):
        config = load_config(ROOT / "config/characters/characters.json")
        apply_character_selection(config, "flash")
        self.assertEqual(get_active_casting_estimate(config).reason, "rule_set_unsupported")
        self.assertEqual(config.characters["constantin"].actions.movement_skill_key, "a")

    def test_personal_fcr_schema_rejects_invalid_values_and_fields(self):
        for value in (True, False, -1, 105.0, float("nan"), float("inf"), "105", []):
            with self.subTest(value=value), self.assertRaises(ValueError):
                profile(value)
        raw = {"rule_set_id": RULE_SET, "skill_id": "teleport", "form": "normal", "fcr": 105}
        for candidate in (dict(raw, unknown=1), {key: value for key, value in raw.items() if key != "fcr"}, None, []):
            with self.assertRaises(ValueError):
                CharacterCastingProfile.from_dict(candidate)
        for name in ("rule_set_id", "skill_id", "form"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                CharacterCastingProfile.from_dict(dict(raw, **{name: " "}))
        for value in ("", " ", 1, True):
            with self.assertRaises(ValueError):
                CharacterProfile("name", "standard", "resurrection", character_class=value)

    def test_invalid_rule_tables_are_rejected(self):
        mutations = [
            lambda raw: raw[0].update(evidence_status="verified"),
            lambda raw: raw[0].update(source_url=""),
            lambda raw: raw[0].update(unknown=1),
            lambda raw: raw[0].update(rules=[]),
            lambda raw: raw.append(copy.deepcopy(raw[0])),
            lambda raw: raw[0]["rules"].append(copy.deepcopy(raw[0]["rules"][0])),
            lambda raw: raw[0]["rules"][0].update(breakpoints=[]),
            lambda raw: raw[0]["rules"][0].update(breakpoints={}),
            lambda raw: raw[0]["rules"][0]["breakpoints"][0].update(minimum_fcr=1),
            lambda raw: raw[0]["rules"][0]["breakpoints"][1].update(minimum_fcr=0),
            lambda raw: raw[0]["rules"][0]["breakpoints"][1].update(minimum_fcr=9.0),
            lambda raw: raw[0]["rules"][0]["breakpoints"][1].update(frames=13),
            lambda raw: raw[0]["rules"][0]["breakpoints"][1].update(frames=True),
            lambda raw: raw[0]["rules"][0]["breakpoints"][1].update(frames=0),
            lambda raw: raw[0]["rules"][0]["breakpoints"][1].update(extra=1),
        ]
        for index, mutate in enumerate(mutations):
            raw = raw_catalog()
            mutate(raw)
            with self.subTest(index=index), self.assertRaises(ValueError):
                CastingCatalog.from_list(raw)
        for raw in (None, {}, "", [None]):
            with self.assertRaises(ValueError):
                CastingCatalog.from_list(raw)

    def test_catalog_and_results_are_immutable_snapshots(self):
        raw = raw_catalog()
        catalog = CastingCatalog.from_list(raw)
        result = catalog.estimate("resurrection", "sorceress", profile())
        raw[0]["rules"][0]["breakpoints"][5]["frames"] = 999
        self.assertEqual(catalog.estimate("resurrection", "sorceress", profile()).frames, 8)
        for instance, field, value in ((profile(), "fcr", 200), (result, "frames", 1), (catalog.rule_sets[0], "revision", "fake")):
            with self.assertRaises(FrozenInstanceError):
                setattr(instance, field, value)
        with self.assertRaises(FrozenInstanceError):
            catalog.rule_sets[0].rules[0].breakpoints[0].frames = 1

    def test_fcr_or_reference_revision_change_requires_new_query(self):
        first = self.catalog.estimate("resurrection", "sorceress", profile(104))
        self.assertEqual(self.catalog.estimate("resurrection", "sorceress", profile(105)).frames, 8)
        self.assertEqual(first.frames, 9)
        changed = raw_catalog()
        changed[0]["revision"] = "synthetic-test-revision"
        changed[0]["rules"][0]["breakpoints"][5]["minimum_fcr"] = 106
        other_catalog = CastingCatalog.from_list(changed)
        result = other_catalog.estimate("resurrection", "sorceress", profile(105))
        self.assertEqual((result.frames, result.revision), (9, "synthetic-test-revision"))
        self.assertEqual(self.catalog.estimate("resurrection", "sorceress", profile(105)).frames, 8)

    def test_custom_class_is_data_driven_without_name_branching(self):
        raw = raw_catalog()
        raw[0]["rules"].append(
            {
                "character_class": "synthetic-test-class",
                "skill_id": "teleport",
                "form": "normal",
                "breakpoints": [{"minimum_fcr": 0, "frames": 20}, {"minimum_fcr": 40, "frames": 18}],
            }
        )
        result = CastingCatalog.from_list(raw).estimate("resurrection", "synthetic-test-class", profile(105))
        self.assertEqual((result.minimum_fcr, result.frames), (40, 18))

    def test_invalid_query_types_are_rejected(self):
        for ruleset, character_class, casting in (
            (None, "sorceress", profile()),
            ("resurrection", True, profile()),
            ("resurrection", "sorceress", 105),
        ):
            with self.assertRaises(ValueError):
                self.catalog.estimate(ruleset, character_class, casting)


if __name__ == "__main__":
    unittest.main()
