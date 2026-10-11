import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import cv2 as cv

from diablo2.common.field_control import DryFieldInput
from diablo2.common.survival_vision import ResourceReading
from diablo2.runs.summoner.encounter_execution import SummonerAppearance, SummonerAppearanceObserver, run_platform_encounter
from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver
from diablo2.runs.summoner.scene_vision import GroundReading
from diablo2.runs.summoner.scene_vision import HostileTarget


class SummonerEncounterTests(unittest.TestCase):
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
                            (ground, Mock()),
                            objective,
                            appearance,
                            **kwargs,
                        )
                else:
                    result = run_platform_encounter(
                        capture, hud, backend, lambda: True, profile, threading.Event(), (ground, Mock()), objective, appearance, **kwargs
                    )
                    self.assertEqual("kill_unconfirmed", result["status"])
                if condition == "near":
                    self.assertIn(("down", "f4"), backend.events)
                    self.assertEqual(("up", "f4"), backend.events[-1])
                else:
                    self.assertFalse(backend.events)
                self.assertNotIn(("down", "f2"), backend.events)


if __name__ == "__main__":
    unittest.main()
