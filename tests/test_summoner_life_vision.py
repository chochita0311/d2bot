import unittest
from pathlib import Path
from types import SimpleNamespace

import cv2 as cv

from diablo2.runs.summoner.encounter_execution import SummonerAppearanceObserver
from diablo2.runs.summoner.life_vision import SummonerLifeTracker
from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver
from diablo2.runs.summoner.journal_vision import HorazonJournalObserver


class SummonerLifeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        cls.frames = [cv.imread(str(owner / name)) for name in ("platform-alive-north.png", "platform-corpse-north.png")]
        cls.platforms = [ArcaneObjectiveObserver().observe(frame) for frame in cls.frames]
        observer = SummonerAppearanceObserver()
        cls.alive = observer.observe(cls.frames[0], cls.platforms[0])
        cls.corpse = observer.observe_corpse(cls.frames[1], cls.platforms[1])
        cls.observer = observer

    def test_reviewed_poses_link_camera_translation_and_two_ordered_corpse_frames(self):
        tracker = SummonerLifeTracker("flash", "one")
        self.assertIsNotNone(self.alive)
        self.assertIsNotNone(self.corpse)
        self.assertIsNone(self.observer.observe(self.frames[1], self.platforms[1]))
        self.assertIsNone(self.observer.observe_corpse(self.frames[0], self.platforms[0]))
        states = []
        for sequence, (alive, corpse, platform) in enumerate(
            ((self.alive, None, self.platforms[0]), (None, self.corpse, self.platforms[1]), (None, self.corpse, self.platforms[1]))
        ):
            states.append(
                tracker.observe(alive, corpse, platform, sequence=sequence, acquired_at=1 + sequence / 10, now=1.01 + sequence / 10)[
                    "state"
                ]
            )
        self.assertEqual(["alive", "unknown", "dead"], states)

    def test_corpse_pair_does_not_require_individual_alive_witness_or_pre_teleport_link(self):
        for condition in ("no_alive", "old_alive", "moved", "far_alive"):
            with self.subTest(condition=condition):
                tracker = SummonerLifeTracker("flash", "one")
                if condition != "no_alive":
                    alive = SimpleNamespace(ground=(1500, 800), asset_id="alive") if condition == "far_alive" else self.alive
                    tracker.observe(alive, None, self.platforms[0], sequence=0, acquired_at=1, now=1.01)
                if condition == "moved":
                    tracker.moved()
                for sequence in (1, 2):
                    at = 14 + sequence / 10 if condition == "old_alive" else 1 + sequence / 10
                    row = tracker.observe(None, self.corpse, self.platforms[1], sequence=sequence, acquired_at=at, now=at + 0.01)
                    self.assertEqual("unknown" if sequence == 1 else "dead", row["state"])
                self.assertEqual("current_platform_corpse_pair", row["confirmation_kind"])
                self.assertIsNone(row["linked_alive_sequence"])

    def test_corpse_pair_requires_fresh_ordered_same_platform_and_stable_relative_position(self):
        for condition in (
            "absence",
            "stale",
            "future",
            "old_pair",
            "moved",
            "wrong_anchor",
            "far",
            "both",
            "same_sequence",
            "same_time",
            "no_platform",
        ):
            with self.subTest(condition=condition):
                tracker = SummonerLifeTracker("flash", "one")
                tracker.observe(None, self.corpse, self.platforms[1], sequence=1, acquired_at=1, now=1.01)
                if condition == "moved":
                    tracker.moved()
                platform = self.platforms[1]
                if condition == "wrong_anchor":
                    platform = SimpleNamespace(ground_point=platform.ground_point, evidence_kind="different")
                row = tracker.observe(
                    self.alive if condition == "both" else None,
                    None
                    if condition == "absence"
                    else SimpleNamespace(ground=(1500, 800), asset_id="different")
                    if condition == "far"
                    else self.corpse,
                    None if condition == "no_platform" else platform,
                    sequence=1 if condition == "same_sequence" else 2,
                    acquired_at=1 if condition == "same_time" else 14 if condition == "old_pair" else 1.1,
                    now=1 if condition == "future" else 14.01 if condition == "old_pair" else 1.4 if condition == "stale" else 1.11,
                )
                self.assertEqual("unknown", row["state"])
                if condition in ("absence", "stale", "future", "both", "no_platform"):
                    row = tracker.observe(
                        None,
                        self.corpse,
                        self.platforms[1],
                        sequence=3,
                        acquired_at=1.5,
                        now=1.51,
                    )
                    self.assertEqual("unknown", row["state"])

    def test_additional_turn_poses_link_only_with_current_ordered_witnesses(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        alive_frame, dead_frame = [cv.imread(str(owner / name)) for name in ("platform-alive-hit-turn.png", "platform-corpse-turn.png")]
        objective = ArcaneObjectiveObserver()
        alive_platform, dead_platform = objective.observe(alive_frame), objective.observe(dead_frame)
        alive = self.observer.observe(alive_frame, alive_platform)
        corpse = self.observer.observe_corpse(dead_frame, dead_platform)
        self.assertIsNotNone(alive)
        self.assertIsNotNone(corpse)
        self.assertIsNone(self.observer.observe_corpse(alive_frame, alive_platform))
        self.assertIsNone(self.observer.observe(dead_frame, dead_platform))
        for moved in (False, True):
            tracker = SummonerLifeTracker("flash", "turn")
            tracker.observe(alive, None, alive_platform, sequence=0, acquired_at=1, now=1.01)
            if moved:
                tracker.moved()
            states = [
                tracker.observe(None, corpse, dead_platform, sequence=i, acquired_at=1 + i / 10, now=1.01 + i / 10)["state"] for i in (1, 2)
            ]
            self.assertEqual(["unknown", "dead"], states)

    def test_map_occluded_robe_parts_require_current_platform_unique_state_and_visible_body(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        objective = ArcaneObjectiveObserver()
        cases = (
            ("alive-back-robe", "alive", (470, 752), (447, 643, 494, 686), (430, 630, 510, 775)),
            ("alive-lower-robe", "alive", (1010, 632), (1003, 575, 1026, 620), (970, 470, 1045, 650)),
            ("corpse-map-occluded", "corpse", (1014, 627), (1008, 594, 1041, 627), (975, 580, 1050, 640)),
            ("corpse-face-down", "corpse", (580, 728), (548, 681, 615, 729), (535, 675, 625, 745)),
        )
        for name, state, point, crop, body in cases:
            with self.subTest(name=name):
                frame = cv.imread(str(owner / ("platform-" + name + ".png")))
                platform = objective.observe(frame)
                self.assertIsNotNone(platform)
                observe = self.observer.observe if state == "alive" else self.observer.observe_corpse
                opposite = self.observer.observe_corpse if state == "alive" else self.observer.observe
                match = observe(frame, platform)
                self.assertEqual(point, match.ground)
                self.assertGreaterEqual(match.score, 0.92)
                self.assertIsNone(opposite(frame, platform))
                self.assertIsNone(observe(frame, None))
                hidden = frame.copy()
                x0, y0, x1, y1 = body
                hidden[y0:y1, x0:x1] = 0
                self.assertIsNone(observe(hidden, platform))
                duplicate = frame.copy()
                x0, y0, x1, y1 = crop
                duplicate[y0 - 120 : y1 - 120, x0 + 200 : x1 + 200] = frame[y0:y1, x0:x1]
                self.assertIsNone(observe(duplicate, platform))

    def test_new_robe_parts_reacquire_alive_after_teleport_before_linked_corpse(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frames = [
            cv.imread(str(owner / ("platform-" + name + ".png"))) for name in ("alive-back-robe", "alive-lower-robe", "corpse-map-occluded")
        ]
        objective = ArcaneObjectiveObserver()
        platforms = [objective.observe(frame) for frame in frames]
        alive = [self.observer.observe(frame, platform) for frame, platform in zip(frames[:2], platforms[:2])]
        corpse = self.observer.observe_corpse(frames[2], platforms[2])
        tracker = SummonerLifeTracker("flash", "map")
        self.assertEqual("alive", tracker.observe(alive[0], None, platforms[0], sequence=0, acquired_at=1, now=1.01)["state"])
        tracker.moved()
        self.assertEqual("unknown", tracker.observe(None, corpse, platforms[2], sequence=1, acquired_at=1.1, now=1.11)["state"])
        states = []
        for sequence, (target, dead, platform) in enumerate(
            ((alive[1], None, platforms[1]), (None, corpse, platforms[2]), (None, corpse, platforms[2])), 2
        ):
            states.append(
                tracker.observe(target, dead, platform, sequence=sequence, acquired_at=1 + sequence / 10, now=1.01 + sequence / 10)["state"]
            )
        self.assertEqual(["alive", "unknown", "dead"], states)

    def test_face_down_lighting_requires_current_unique_body_and_ordered_corpse_pair(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        names = ("before", "after-approach", None)
        objective, observer = ArcaneObjectiveObserver(), SummonerAppearanceObserver()
        tracker = SummonerLifeTracker("flash", "lighting")
        states = []
        for index, (suffix, at, sequence) in enumerate(zip(names, (23924.421, 23925.312, 23936.484), (0, 2, 111))):
            frame = cv.imread(str(owner / ("platform-corpse-face-down-lighting" + ("-" + suffix if suffix else "") + ".png")))
            platform = objective.observe(frame)
            self.assertIsNotNone(platform)
            corpse = observer.observe_corpse(frame, platform)
            self.assertIsNotNone(corpse)
            self.assertEqual("summoner-robe-corpse-face-down", corpse.asset_id)
            self.assertGreaterEqual(corpse.score, 0.90)
            self.assertLess(corpse.score, 0.92)
            self.assertIsNone(observer.observe(frame, platform))
            self.assertIsNone(observer.observe_corpse(frame, None))
            left, top, right, bottom = (1370, 680, 1525, 790) if index == 0 else (870, 690, 1030, 790)
            hidden = frame.copy()
            hidden[top:bottom, left:right] = 0
            self.assertIsNone(observer.observe_corpse(hidden, platform))
            duplicate = frame.copy()
            duplicate[top - 180 : bottom - 180, left - 200 : right - 200] = frame[top:bottom, left:right]
            self.assertIsNone(observer.observe_corpse(duplicate, platform))
            if index == 1:
                tracker.moved()
            states.append(tracker.observe(None, corpse, platform, sequence=sequence, acquired_at=at, now=at + 0.01)["state"])
        self.assertEqual(["unknown", "unknown", "dead"], states)

    def test_face_down_tuning_rejects_other_region_scenes_even_with_permissive_platform_roi(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references"
        observer, journal = SummonerAppearanceObserver(), HorazonJournalObserver()
        platform = SimpleNamespace(rune_boxes=((100, 100, 1800, 400),))
        frames = sorted(path for folder in ("terminal", "hub", "navigation") for path in (owner / folder).glob("*.png"))
        native_count = 0
        for path in frames:
            with self.subTest(path=path.name):
                frame = cv.imread(str(path))
                if frame.shape != (1140, 1922, 3):
                    with self.assertRaises(ValueError):
                        observer.observe_corpse(frame, platform)
                    continue
                native_count += 1
                self.assertIsNone(observer.observe_corpse(frame, platform))
                self.assertIsNone(observer.observe(frame, platform))
                self.assertIsNone(journal.observe(frame))
        self.assertGreaterEqual(native_count, 72)
