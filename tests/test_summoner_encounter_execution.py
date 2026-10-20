import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import cv2 as cv
import numpy as np

from diablo2.common.field_control import DryFieldInput
from diablo2.common.survival_vision import ResourceReading
from diablo2.runs.summoner.encounter_execution import SummonerAppearance, SummonerAppearanceObserver, run_platform_encounter
from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver
from diablo2.runs.summoner.scene_vision import GroundReading
from diablo2.runs.summoner.scene_vision import HostileTarget
from diablo2.runs.summoner.life_vision import SummonerLifeTracker
from tests.test_conditional_loot import FakeStop


class SummonerEncounterTests(unittest.TestCase):
    def test_unverified_mercenary_stops_before_platform_movement_or_attack(self):
        capture, hud, objective, appearance = Mock(), Mock(), Mock(), Mock()
        capture.grab.return_value = SimpleNamespace(frame=np.zeros((1140, 1922, 3), np.uint8))
        hud.observe.return_value = SimpleNamespace(
            life=ResourceReading(900, 1000), mana=ResourceReading(900, 1000), belt_visibility="closed", mercenary_status="unverified"
        )
        profile = SimpleNamespace(
            survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
            actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
        )
        backend = DryFieldInput()
        with self.assertRaisesRegex(RuntimeError, "mercenary_unverified"):
            run_platform_encounter(
                capture,
                hud,
                backend,
                lambda: True,
                profile,
                threading.Event(),
                (Mock(), Mock()),
                objective,
                appearance,
                buff_expires_at=100,
                reference_seconds=0.01,
                evidence=[],
                clock=lambda: 1.0,
                emit=lambda _: None,
            )
        self.assertFalse(backend.events)
        objective.observe.assert_not_called()
        appearance.observe.assert_not_called()

    def test_additional_pose_requires_native_match_and_unique_current_target(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        observer, objective = SummonerAppearanceObserver(), ArcaneObjectiveObserver()
        for state, name, point, box in (
            ("alive", "platform-alive-shield.png", (470, 752), (452, 652, 489, 742)),
            ("corpse", "platform-corpse-shield.png", (672, 710), (631, 668, 703, 709)),
            ("alive", "platform-alive-hit-turn.png", (550, 697), (514, 576, 570, 693)),
            ("corpse", "platform-corpse-turn.png", (980, 641), (938, 606, 1021, 654)),
            ("alive", "platform-alive-raised-arm.png", (556, 696), (529, 612, 584, 696)),
            ("corpse", "platform-corpse-side-robe.png", (993, 633), (966, 604, 1040, 638)),
            ("alive", "platform-alive-front-cast.png", (785, 787), (768, 699, 813, 779)),
            ("corpse", "platform-corpse-wide-side.png", (799, 783), (754, 745, 845, 789)),
            ("corpse", "platform-corpse-side-lower-bands.png", (1002, 639), (974, 619, 1030, 637)),
            ("corpse", "platform-corpse-side-lower-bands-followup-map-on.png", (1002, 639), (974, 619, 1030, 637)),
            ("corpse", "platform-corpse-side-lower-bands-followup-map-off.png", (1002, 639), (974, 619, 1030, 637)),
            ("corpse", "platform-corpse-lower-band-lighting-before.png", (1030, 625), (1026, 605, 1058, 619)),
            ("corpse", "platform-corpse-lower-band-lighting-after.png", (1030, 625), (1026, 605, 1058, 619)),
            ("alive", "platform-alive-front-open.png", (1020, 525), (1004, 423, 1044, 515)),
            ("corpse", "platform-corpse-front-hem.png", (1002, 639), (1000, 616, 1034, 632)),
            ("corpse", "platform-corpse-front-hem-followup.png", (1002, 639), (1000, 616, 1034, 632)),
            ("corpse", "platform-corpse-left-hem.png", (1002, 639), (972, 614, 1012, 638)),
            ("corpse", "platform-corpse-left-hem-followup.png", (1002, 639), (972, 614, 1012, 638)),
        ):
            with self.subTest(state=state, name=name):
                frame = cv.imread(str(owner / name))
                observe = observer.observe if state == "alive" else observer.observe_corpse
                platform = objective.observe(frame)
                self.assertIsNotNone(platform)
                self.assertEqual(point, observe(frame, platform).ground)
                opposite = observer.observe_corpse if state == "alive" else observer.observe
                self.assertIsNone(opposite(frame, platform))
                self.assertIsNone(observe(frame, None))
                shifted = cv.copyMakeBorder(frame[:-1, :-1], 1, 0, 1, 0, cv.BORDER_CONSTANT)
                self.assertEqual(tuple(value + 1 for value in point), observe(shifted, objective.observe(shifted)).ground)
                left, top, right, bottom = box
                hidden = frame.copy()
                hidden[top:bottom, left:right] = 0
                self.assertIsNone(observe(hidden, platform))
                duplicate = frame.copy()
                duplicate[top - 120 : bottom - 120, left + 200 : right + 200] = frame[top:bottom, left:right]
                self.assertIsNone(observe(duplicate, platform))

    def test_live_executor_consumes_linked_death_and_preserves_three_witness_frames(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frames = [cv.imread(str(owner / name)) for name in ("platform-alive-north.png", "platform-corpse-north.png")]
        for unconfirmed_click in (False, True):
            now, capture, hud = [1.0], Mock(target=dict(left=0, top=0)), Mock()
            index = [0]

            def grab():
                now[0] += 0.1
                frame = frames[0 if index[0] < 2 else 1]
                index[0] += 1
                return SimpleNamespace(frame=frame)

            capture.grab.side_effect = grab
            hud.observe.return_value = SimpleNamespace(
                life=ResourceReading(900, 1000), mana=ResourceReading(900, 1000), belt_visibility="closed"
            )
            ground = SimpleNamespace(observe=lambda *args, **kwargs: GroundReading((525, 770) if index[0] <= 2 else (947, 712), "test"))
            path = Mock()
            path.choose.return_value = SimpleNamespace(aim=(525, 770))
            profile = SimpleNamespace(
                survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
                actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
            )
            loot = Mock(movement_allowed=True)
            loot.after_combat.return_value = dict(
                status="click_unconfirmed" if unconfirmed_click else "inspection_complete",
                bounded_reason="two_frames_without_eligible_candidate",
                movement_allowed=True,
                clicks=[{}] if unconfirmed_click else [],
            )
            evidence, backend = [], DryFieldInput()
            result = run_platform_encounter(
                capture,
                hud,
                backend,
                lambda: True,
                profile,
                FakeStop(now),
                (ground, path),
                ArcaneObjectiveObserver(),
                SummonerAppearanceObserver(),
                buff_expires_at=100,
                reference_seconds=0.01,
                evidence=evidence,
                clock=lambda: now[0],
                emit=lambda message: None,
                life_tracker=SummonerLifeTracker("flash", "one"),
                loot_controller=loot,
            )
            self.assertTrue(result["kill_confirmed"])
            self.assertEqual(not unconfirmed_click, result["loot_complete"])
            self.assertEqual(["alive", "alive", "unknown", "dead"], [row["life"]["state"] for row in result["observations"]])
            self.assertEqual(0, result["approach_requests"])
            self.assertNotIn(("press", "f2"), backend.events)
            self.assertIn(("down", "f4"), backend.events)
            self.assertIn("encounter-alive-witness.png", [name for name, _ in evidence])
            self.assertIn("encounter-first-corpse.png", [name for name, _ in evidence])
            self.assertEqual(len(evidence), result["evidence_frames"])
            self.assertLessEqual(len(evidence), 24)
            self.assertIs(evidence[-1][1], frames[1])

    def test_entry_corpse_after_general_nova_closes_without_targeted_approach_or_alive_witness(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frames = [cv.imread(str(owner / ("platform-corpse-side-robe-followup-%d.png" % i))) for i in (1, 2)]
        self.check_entry_corpse_executor(frames)

    def test_face_down_lighting_executor_confirms_corpse_without_extra_approach_or_nova(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frames = [cv.imread(str(owner / ("platform-corpse-face-down-lighting" + suffix + ".png"))) for suffix in ("-after-approach", "")]
        self.check_entry_corpse_executor(frames)

    def test_wide_side_corpse_executor_uses_two_current_observations_without_alive_requirement(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frame = cv.imread(str(owner / "platform-corpse-wide-side.png"))
        shifted = cv.copyMakeBorder(frame[:-1, :-1], 1, 0, 1, 0, cv.BORDER_CONSTANT)
        self.check_entry_corpse_executor([frame, shifted])

    def test_corpse_lower_bands_executor_consumes_separate_followup_frames(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frames = [cv.imread(str(owner / ("platform-corpse-side-lower-bands-followup-map-" + state + ".png"))) for state in ("on", "off")]
        self.check_entry_corpse_executor(frames)

    def test_corpse_right_bands_executor_confirms_after_camera_and_lighting_change(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frames = [cv.imread(str(owner / ("platform-corpse-lower-band-lighting-" + stage + ".png"))) for stage in ("before", "after")]
        self.check_entry_corpse_executor(frames)

    def check_entry_corpse_executor(self, frames):
        now, capture, hud = [1.0], Mock(target=dict(left=0, top=0)), Mock()
        index = [0]

        def grab():
            now[0] += 0.1
            frame = frames[min(index[0], 1)]
            index[0] += 1
            return SimpleNamespace(frame=frame)

        capture.grab.side_effect = grab
        hud.observe.return_value = SimpleNamespace(
            life=ResourceReading(900, 1000), mana=ResourceReading(900, 1000), belt_visibility="closed"
        )
        ground = SimpleNamespace(observe=lambda *args, **kwargs: GroundReading((962, 580), "test"))
        profile = SimpleNamespace(
            survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
            actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
        )
        loot = Mock(movement_allowed=True)
        loot.after_combat.return_value = dict(
            status="inspection_complete", bounded_reason="two_frames_without_eligible_candidate", movement_allowed=True, clicks=[]
        )
        evidence, backend = [], DryFieldInput()
        result = run_platform_encounter(
            capture,
            hud,
            backend,
            lambda: True,
            profile,
            FakeStop(now),
            (ground, Mock()),
            ArcaneObjectiveObserver(),
            SummonerAppearanceObserver(),
            buff_expires_at=100,
            reference_seconds=0.01,
            evidence=evidence,
            clock=lambda: now[0],
            emit=lambda message: None,
            life_tracker=SummonerLifeTracker("flash", "aoe-room"),
            loot_controller=loot,
        )
        self.assertTrue(result["kill_confirmed"])
        self.assertTrue(result["loot_complete"])
        self.assertEqual("current_platform_corpse_pair", result["life_confirmation"]["confirmation_kind"])
        self.assertIsNone(result["life_confirmation"]["linked_alive_sequence"])
        self.assertEqual(["unknown", "dead"], [row["life"]["state"] for row in result["observations"]])
        self.assertEqual(0, result["approach_requests"])
        self.assertEqual(0, result["nova_dispatches"])
        self.assertFalse(backend.events)
        self.assertNotIn("encounter-alive-witness.png", [name for name, _ in evidence])
        self.assertIn("encounter-first-corpse.png", [name for name, _ in evidence])
        self.assertEqual(len(evidence), result["evidence_frames"])
        loot.after_combat.assert_called_once()

    def test_after_approach_nearby_panel_allows_nova_without_individual_alive_reobservation(self):
        now, capture, hud, appearance = [1.0], Mock(target=dict(left=0, top=0)), Mock(), Mock()
        backend = DryFieldInput()

        def grab():
            now[0] += 0.1
            return SimpleNamespace(frame=SimpleNamespace(shape=(1140, 1922, 3)))

        capture.grab.side_effect = grab
        hud.observe.return_value = SimpleNamespace(
            life=ResourceReading(900, 1000), mana=ResourceReading(900, 1000), belt_visibility="closed"
        )
        appearance.observe.side_effect = lambda *args: (
            None if ("press", "f2") in backend.events else SummonerAppearance((980, 500, 1010, 560), (1000, 600), 0.95)
        )
        appearance.observe_corpse.return_value = None
        objective = Mock()
        objective.observe.return_value = SimpleNamespace(ground_point=(1000, 600), evidence_kind="reviewed_platform")
        ground = SimpleNamespace(
            observe=lambda *args, **kwargs: GroundReading((962, 580) if ("press", "f2") in backend.events else (500, 580), "test")
        )
        path, target = Mock(), Mock()
        path.choose.return_value = SimpleNamespace(aim=(1000, 600))
        target.observe.return_value = HostileTarget("hostile-target", (800, 80, 1100, 110), None)
        profile = SimpleNamespace(
            survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
            actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
        )
        result = run_platform_encounter(
            capture,
            hud,
            backend,
            lambda: True,
            profile,
            FakeStop(now),
            (ground, path),
            objective,
            appearance,
            buff_expires_at=100,
            reference_seconds=0.01,
            evidence=[],
            clock=lambda: now[0],
            emit=lambda message: None,
            life_tracker=SummonerLifeTracker("flash", "one"),
            target_observer=target,
        )
        self.assertEqual(1, result["approach_requests"])
        self.assertGreater(result["nova_dispatches"], 0)
        self.assertEqual("kill_unconfirmed", result["status"])
        self.assertTrue(any(row.get("action") == "nova_on_current_hostile_panel" for row in result["observations"]))
        target.observe.assert_called()

    def test_platform_entry_without_robe_requires_current_floor_then_positive_hostile_panel(self):
        for condition in ("panel", "no_panel", "bad_floor", "no_platform", "stale", "expired", "unverified", "unverified_deadline"):
            with self.subTest(condition=condition):
                now = [1.0]
                frame = SimpleNamespace(shape=(1140, 1922, 3))
                capture, hud, objective, appearance = Mock(), Mock(), Mock(), Mock()
                capture.target = dict(left=0, top=0)
                backend, evidence = DryFieldInput(), []

                def grab():
                    now[0] += 0.1
                    return SimpleNamespace(frame=frame)

                def platform(seen):
                    if condition == "stale":
                        now[0] += 0.3
                    if condition == "no_platform":
                        return None
                    moved = ("press", "f2") in backend.events
                    return SimpleNamespace(ground_point=(962, 578) if moved else (1400, 578))

                capture.grab.side_effect = grab
                hud.observe.return_value = SimpleNamespace(
                    belt_visibility="closed",
                    life=ResourceReading() if condition.startswith("unverified") else ResourceReading(900, 1000),
                    mana=ResourceReading(900, 1000),
                )
                objective.observe.side_effect = platform
                appearance.observe.return_value = None
                ground = SimpleNamespace(observe=lambda *args, **kwargs: GroundReading((962, 578), "test"))
                path = Mock()
                path.choose.return_value = SimpleNamespace(aim=None if condition == "bad_floor" else (1400, 578))
                target = Mock()
                target.observe.return_value = HostileTarget("hostile-target", (800, 80, 1100, 110), None) if condition == "panel" else None
                profile = SimpleNamespace(
                    survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
                    actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
                )
                result = run_platform_encounter(
                    capture,
                    hud,
                    backend,
                    lambda: True,
                    profile,
                    threading.Event(),
                    (ground, path),
                    objective,
                    appearance,
                    buff_expires_at=0 if condition == "expired" else 1.55 if condition == "unverified_deadline" else 100,
                    reference_seconds=0.01,
                    evidence=evidence,
                    clock=lambda: now[0],
                    emit=lambda *args: None,
                    target_observer=target,
                )
                self.assertEqual("kill_unconfirmed", result["status"])
                self.assertLessEqual(len(evidence), 24)
                self.assertEqual(len(evidence), result["evidence_frames"])
                self.assertEqual([name for name, _ in evidence], [item["file"] for item in result["evidence"]])
                for item in result["evidence"]:
                    row = next(row for row in result["observations"] if row["sequence"] == item["sequence"])
                    self.assertEqual(row["acquired_at"], item["acquired_at"])
                    self.assertIn(item["phase"], ("initial", "prefight", "combat", "final"))
                if condition.startswith("unverified"):
                    self.assertTrue(result["observations"])
                    self.assertTrue(all(row["blocked"] == "observation_unverified" for row in result["observations"]))
                if condition in ("panel", "no_panel"):
                    self.assertEqual(1, result["approach_requests"])
                    self.assertIn(("press", "f2"), backend.events)
                else:
                    self.assertFalse(backend.events)
                if condition == "panel":
                    self.assertIn(("down", "f4"), backend.events)
                    self.assertTrue(any(row.get("action") == "nova_on_current_hostile_panel" for row in result["observations"]))
                    self.assertGreaterEqual(len(evidence), 6)
                else:
                    self.assertNotIn(("down", "f4"), backend.events)

    def test_reviewed_robe_requires_current_platform_and_does_not_claim_life_state(self):
        source = (
            Path(__file__).resolve().parents[1]
            / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner/platform-alive.png"
        )
        if not source.exists():
            self.fail("Required shared encounter reference unavailable")
        frame = cv.imread(str(source))
        observer = SummonerAppearanceObserver()
        platform = ArcaneObjectiveObserver().observe(frame)
        candidate = observer.observe(frame, platform)
        self.assertEqual((1456, 734), candidate.ground)
        self.assertGreaterEqual(candidate.score, 0.92)
        self.assertFalse(hasattr(candidate, "dead"))
        self.assertIsNone(observer.observe(frame, None))
        self.assertIsNone(observer.observe(frame, SimpleNamespace(rune_boxes=((40, 50, 100, 110),))))

    def test_nova_requires_fresh_current_platform_appearance_ground_and_resources(self):
        for condition in ("near", "unknown", "no_platform", "stale", "no_ground", "expired", "low"):
            with self.subTest(condition=condition):
                now = [1.0]
                frame, capture, hud, objective, appearance = object(), Mock(), Mock(), Mock(), Mock()
                capture.target = dict(left=0, top=0)

                def grab():
                    now[0] += 0.1
                    return SimpleNamespace(frame=frame)

                capture.grab.side_effect = grab
                hud.observe.return_value = SimpleNamespace(
                    belt_visibility="closed",
                    life=ResourceReading(400 if condition == "low" else 900, 1000),
                    mana=ResourceReading(900, 1000),
                )
                objective.observe.return_value = None if condition == "no_platform" else object()

                def observe(seen, platform):
                    if condition == "stale":
                        now[0] += 0.3
                    return (
                        None if condition == "unknown" or platform is None else SummonerAppearance((980, 500, 1010, 560), (1000, 600), 0.95)
                    )

                appearance.observe.side_effect = observe
                ground = SimpleNamespace(
                    observe=lambda *args, **kwargs: GroundReading(None if condition == "no_ground" else (962, 580), "test")
                )
                profile = SimpleNamespace(
                    survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
                    actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
                )
                backend, evidence = DryFieldInput(), []
                path = Mock()
                path.choose.return_value = SimpleNamespace(aim=(1000, 600))
                kwargs = dict(
                    buff_expires_at=0 if condition == "expired" else 100,
                    reference_seconds=0.32,
                    evidence=evidence,
                    clock=lambda: now[0],
                    emit=lambda *args: None,
                )
                if condition == "low":
                    with self.assertRaisesRegex(RuntimeError, "resource_low"):
                        run_platform_encounter(
                            capture,
                            hud,
                            backend,
                            lambda: True,
                            profile,
                            threading.Event(),
                            (ground, path),
                            objective,
                            appearance,
                            **kwargs,
                        )
                else:
                    result = run_platform_encounter(
                        capture, hud, backend, lambda: True, profile, threading.Event(), (ground, path), objective, appearance, **kwargs
                    )
                    self.assertEqual("kill_unconfirmed", result["status"])
                if condition == "near":
                    self.assertNotIn(("press", "f2"), backend.events)
                    self.assertIn(("down", "f4"), backend.events)
                    self.assertEqual(("up", "f4"), backend.events[-1])
                else:
                    self.assertFalse(backend.events)
                self.assertNotIn(("down", "f2"), backend.events)


if __name__ == "__main__":
    unittest.main()
