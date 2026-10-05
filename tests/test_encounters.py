from dataclasses import replace
import json
from pathlib import Path
import tempfile
import unittest

import cv2 as cv
import numpy as np

from diablo2.common.encounters import CombatRadius, LifeWitness, SceneContext, VisualCandidate, life_state, proximity
from diablo2.common.encounter_vision import RegionObserver
from diablo2.common.survival import ObservationStamp
from diablo2.common.survival_vision import DEFAULT_ASSETS, SurvivalHudObserver
from diablo2.common.config import load_config


ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"


def context(sequence=10, observed_at=10, generation=0, size=(200, 100)):
    return SceneContext(
        ObservationStamp("sample-character", "sample-room", sequence, observed_at), "sample-region", generation, size, (0, 0, *size)
    )


def candidate(scene, ground=(110, 50), category="monster", appearance="alive"):
    x, y = ground or (110, 50)
    return VisualCandidate(
        scene, "sample-asset", "sample-kind", category, appearance, 0.95, (int(x) - 5, int(y) - 5, int(x) + 5, int(y) + 5), ground
    )


class EncounterContractTests(unittest.TestCase):
    def check_range(self, ground, *, scene=None, radius=None, **kwargs):
        scene = scene or context()
        return proximity(
            candidate(scene, ground),
            scene,
            (100, 50),
            radius or CombatRadius(0.1, 0.2),
            now=scene.stamp.observed_at,
            max_age=3,
            target_id="a",
            **kwargs,
        )

    def test_enter_leave_boundaries_and_explicit_radius(self):
        self.assertEqual(self.check_range((120, 50)).state, "near")
        self.assertEqual(self.check_range((140, 50)).state, "far")
        self.assertEqual(self.check_range((130, 50)).state, "boundary")
        for enter, leave in [(0, 0.2), (0.2, 0.2), (0.3, 0.2), (0.1, 1.1), (True, 0.2), (float("nan"), 0.2)]:
            with self.subTest(enter=enter, leave=leave), self.assertRaises(ValueError):
                CombatRadius(enter, leave)

    def test_distance_normalizes_native_width_and_height(self):
        small = self.check_range((120, 60))
        large_context = context(size=(400, 200))
        large = proximity(
            candidate(large_context, (240, 120)), large_context, (200, 100), CombatRadius(0.1, 0.2), now=10, max_age=1, target_id=None
        )
        self.assertAlmostEqual(small.distance, large.distance)
        self.assertEqual(small.state, large.state)

    def test_hysteresis_requires_same_target_fresh_context_and_movement(self):
        old = context()
        near = self.check_range((110, 50), scene=old)
        new = context(11, 11)
        self.assertEqual(self.check_range((130, 50), scene=new, previous=near, previous_context=old).state, "near")
        for scene, before, prior in [
            (context(11, 11, 1), old, near),
            (new, replace(old, stamp=replace(old.stamp, room_id="other")), near),
            (new, old, replace(near, target_id="b")),
            (context(15, 15), old, near),
        ]:
            with self.subTest(scene=scene):
                self.assertEqual(self.check_range((130, 50), scene=scene, previous=prior, previous_context=before).state, "boundary")

    def test_stale_future_and_different_frames_cannot_supply_distance(self):
        scene = context()
        for current, now in [(scene, 14), (scene, 9), (context(11, 10), 10)]:
            result = proximity(candidate(scene), current, (100, 50), CombatRadius(0.1, 0.2), now=now, max_age=1, target_id=None)
            self.assertEqual(result.state, "unknown")
            self.assertIsNone(result.distance)

    def test_landmarks_corpses_groups_and_unobserved_ground_are_unknown(self):
        scene = context()
        for item, player in [
            (candidate(scene, category="landmark", appearance="landmark"), (100, 50)),
            (candidate(scene, category="monster-group", appearance="corpse"), (100, 50)),
            (candidate(scene, appearance="corpse"), (100, 50)),
            (candidate(scene, None), (100, 50)),
            (candidate(scene), None),
        ]:
            result = proximity(item, scene, player, CombatRadius(0.1, 0.2), now=10, max_age=1, target_id=None)
            self.assertEqual(result.state, "unknown")

    def test_invalid_native_geometry_is_rejected(self):
        for viewport in [(0, 0, 201, 100), (10, 10, 10, 20), (-1, 0, 20, 30)]:
            with self.assertRaises(ValueError):
                replace(context(), viewport=viewport)
        for ground in [(float("nan"), 50), (True, 50), (200, 50), (110, 70)]:
            with self.assertRaises(ValueError):
                replace(candidate(context()), ground=ground)

    def witnesses(self):
        alive = LifeWitness(context(10, 10), "a", "ghoul", "alive", "human-reviewed")
        dead = LifeWitness(context(11, 11), "a", "ghoul", "dead", "linked-observation", 10)
        follow = LifeWitness(context(12, 12), "a", "ghoul", "dead", "linked-observation", 10)
        return alive, dead, follow

    def test_positive_alive_and_linked_death_need_independent_confirmation(self):
        alive, dead, follow = self.witnesses()
        self.assertEqual(life_state(alive.context, alive, now=10, max_age=3), "alive")
        self.assertEqual(life_state(dead.context, dead, previous_alive=alive, now=11, max_age=3), "unknown")
        self.assertEqual(life_state(follow.context, follow, previous_alive=alive, first_dead=dead, now=12, max_age=3), "dead")
        self.assertEqual(life_state(dead.context, dead, previous_alive=alive, first_dead=dead, now=11, max_age=3), "unknown")

    def test_disappearance_template_or_unlinked_corpse_does_not_prove_death(self):
        alive, dead, follow = self.witnesses()
        for witness in [
            None,
            replace(follow, provenance="template-candidate"),
            replace(follow, linked_alive_sequence=None),
            replace(follow, target_id="b"),
            replace(follow, kind="hell-clan"),
            replace(follow, state="unknown"),
        ]:
            self.assertEqual(life_state(follow.context, witness, previous_alive=alive, first_dead=dead, now=12, max_age=3), "unknown")
        template_alive = replace(alive, provenance="template-candidate")
        self.assertEqual(life_state(alive.context, template_alive, now=10, max_age=3), "unknown")

    def test_teleport_room_character_or_old_witness_breaks_death_link(self):
        alive, dead, follow = self.witnesses()
        for changed in [
            replace(alive, context=context(10, 10, 1)),
            replace(alive, context=replace(alive.context, stamp=replace(alive.context.stamp, character_id="b"))),
            replace(alive, context=replace(alive.context, stamp=replace(alive.context.stamp, room_id="b"))),
            replace(alive, provenance="template-candidate"),
        ]:
            self.assertEqual(life_state(follow.context, follow, previous_alive=changed, first_dead=dead, now=12, max_age=3), "unknown")
        self.assertEqual(life_state(follow.context, follow, previous_alive=alive, first_dead=dead, now=12, max_age=1), "unknown")


class RegionReplayTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((ASSETS / "manifest.json").read_text(encoding="utf-8"))
        cls.observer = RegionObserver(ASSETS / "manifest.json")

    def replay(self, name):
        case = next(item for item in self.manifest["cases"] if item["id"] == name)
        scene = SceneContext(
            ObservationStamp("sample-character", "sample-room", 10, 10),
            self.manifest["region_id"],
            0,
            tuple(self.manifest["frame_size"]),
            tuple(case["viewport"]),
        )
        return cv.imread(str(ASSETS / case["path"])), scene, case

    def scan(self, name):
        frame, scene, _ = self.replay(name)
        return self.observer.scan(frame, scene, now=10, max_age=1, limit_per_template=3)

    def test_reviewed_actual_candidates_preserve_native_box_score_ground(self):
        hits = self.scan("alive")
        self.assertEqual({h.asset_id for h in hits}, {"ghoul-alive", "hell-clan-alive"})
        ghoul = next(h for h in hits if h.asset_id == "ghoul-alive")
        self.assertGreater(ghoul.score, 0.99)
        self.assertEqual(ghoul.box, (612, 623, 731, 774))
        self.assertEqual(ghoul.ground, (700, 760))
        # 템플릿과 다른 직후 자세는 미검출이다. 사망으로 전환하지 않는다.
        self.assertEqual(self.scan("alive-followup"), ())

    def test_actual_corpse_group_has_no_instance_count_or_ground(self):
        for name in ("corpses", "corpses-followup"):
            hits = self.scan(name)
            self.assertEqual(len(hits), 1)
            self.assertEqual(hits[0].category, "monster-group")
            self.assertEqual(hits[0].appearance, "corpse")
            self.assertIsNone(hits[0].ground)
        self.assertEqual(self.scan("corpse-ground")[0].asset_id, "ghoul-corpse")

    def test_shrine_negative_does_not_match_monsters(self):
        hits = self.scan("shrine")
        self.assertEqual([h.category for h in hits], ["landmark"])
        self.assertEqual(hits[0].kind, "fire-resist-shrine")
        self.assertEqual(self.scan("stairs"), ())

    def test_actual_gray_floor_vs_void_and_obscured_patch(self):
        frame, scene, _ = self.replay("terrain")

        def read(box):
            return self.observer.terrain(frame, scene, box, now=10, max_age=1, minimum_floor=0.75, maximum_void=0.15)

        floor = read((1300, 340, 1360, 385))
        void = read((550, 500, 610, 560))
        obscured = read((920, 395, 985, 550))
        self.assertEqual(floor.state, "floor-candidate")
        self.assertGreater(floor.floor_ratio, 0.8)
        self.assertEqual(void.state, "reject-or-unknown")
        self.assertGreater(void.void_ratio, 0.99)
        self.assertEqual(obscured.state, "reject-or-unknown")

    def test_wrong_size_region_stale_and_outside_roi_cannot_be_used(self):
        frame, scene, _ = self.replay("terrain")
        self.assertEqual(self.observer.scan(frame, scene, now=12, max_age=1, limit_per_template=1), ())
        for pixels, current in [(frame[:-1], scene), (frame, replace(scene, region_id="other"))]:
            with self.assertRaises(ValueError):
                self.observer.scan(pixels, current, now=10, max_age=1, limit_per_template=1)
        with self.assertRaises(ValueError):
            self.observer.terrain(frame, scene, (0, 0, 20, 20), now=10, max_age=1, minimum_floor=0.75, maximum_void=0.15)
        stale = self.observer.terrain(frame, scene, (1300, 340, 1360, 385), now=12, max_age=1, minimum_floor=0.75, maximum_void=0.15)
        self.assertEqual(stale.state, "unknown")

    def test_synthetic_repeated_sprite_is_not_reduced_to_one_name(self):
        _, scene, _ = self.replay("alive")
        pixels = np.zeros((710, 1600, 3), np.uint8)
        sprite = cv.imread(str(ASSETS / "hell-clan-alive.png"))
        for x, y in [(100, 100), (700, 400)]:
            pixels[y : y + sprite.shape[0], x : x + sprite.shape[1]] = sprite
        hits = self.observer.scan(pixels, scene, now=10, max_age=1, limit_per_template=3)
        reds = [h for h in hits if h.asset_id == "hell-clan-alive"]
        self.assertEqual(len(reds), 2)
        self.assertEqual({h.box[:2] for h in reds}, {(220, 280), (820, 580)})

    def test_manifest_rejects_mislabeled_webp_and_path_escape(self):
        for filename, payload in [("fake.png", b"RIFFjunkWEBP"), ("../escape.png", b"unused")]:
            with tempfile.TemporaryDirectory(prefix="d2-encounter-test-") as temp:
                owner = Path(temp)
                raw = dict(self.manifest)
                raw["templates"] = [dict(raw["templates"][0], path=filename)]
                (owner / "manifest.json").write_text(json.dumps(raw), encoding="utf-8")
                if filename == "fake.png":
                    (owner / filename).write_bytes(payload)
                with self.assertRaises(ValueError):
                    RegionObserver(owner / "manifest.json")

    def test_actual_consumed_belt_crop_reads_eleven_on_composed_town_hud(self):
        observer = SurvivalHudObserver()
        baseline = next(item for item in observer.manifest["references"] if item["path"] == "references/expanded.png")
        frame = np.zeros((1140, 1922, 3), np.uint8)
        x0, y0, x1, y1 = baseline["canvas_box"]
        frame[y0:y1, x0:x1] = cv.imread(str(DEFAULT_ASSETS / baseline["path"]))
        frame[885:1120, 1070:1340] = cv.imread(str(DEFAULT_ASSETS / "references/belt-after-north-11.png"))
        reading = observer.observe(frame)
        policy = load_config(ROOT / "config").characters["flash"].survival
        self.assertEqual(reading.belt_visibility, "expanded")
        contents = reading.belt_contents(policy)
        self.assertIsNotNone(contents)
        self.assertEqual([count for _, count in contents.values()], [3, 4, 4, 4])


if __name__ == "__main__":
    unittest.main()
