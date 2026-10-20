import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import cv2 as cv
import numpy as np

from diablo2.actions.town_loop_vision import TownLoopVision, TownLoopPlanner
from diablo2.common.field_control import DryFieldInput
from diablo2.runs.summoner.canyon_finish import CanyonApproachObserver, HorazonJournalObserver, enter_canyon, red_portal
from tests.test_conditional_loot import FakeStop


class CanyonBackend(DryFieldInput):
    def click(self):
        self.events.append(("click",))


class SummonerCanyonTests(unittest.TestCase):
    def test_current_canyon_left_floor_reopens_waypoint_when_character_covers_center(self):
        for name in ("waypoint-center-occupied.png", "waypoint-center-occupied-day.png"):
            with self.subTest(name=name):
                self._canyon_occupied_case(name)

    def _canyon_occupied_case(self, name):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/canyon-of-the-magi/remastered-ko-1922x1140"
        frame = cv.imread(str(owner / "references" / name))
        vision = TownLoopVision()
        self.assertEqual(0.92, vision.manifest["templates"]["world_canyon_waypoint"]["threshold"])
        for missing in (None, "floor", "area", "hud"):
            changed = frame.copy()
            if missing == "floor":
                changed[534:608, 701:902] = 0
            elif missing == "area":
                changed[90:140, 1700:1900] = 0
            elif missing == "hud":
                changed[880:1127, 250:1650] = 0
            planner = TownLoopPlanner(vision, 1, "hell", "canyon")
            planner.data["phase"] = "destination_arrival"
            action = planner.step(changed, now=1.0)
            if missing is None:
                self.assertEqual([610, 385], action["point"])
                self.assertEqual("return_panel", action["phase"])
                self.assertEqual([610, 385], planner.step(changed, now=2.0)["point"])
            else:
                self.assertIsNone(action, missing)
        self.assertIsNone(vision.find(vision.normalize(self.before), "world_canyon_waypoint"))

    @classmethod
    def setUpClass(cls):
        root = Path(__file__).resolve().parents[1] / "assets/regions"
        arcane = root / "arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        canyon = root / "canyon-of-the-magi/remastered-ko-1922x1140/references"
        cls.before = cv.imread(str(arcane / "platform-corpse-north.png"))
        cls.portal = cv.imread(str(arcane / "journal-red-portal.png"))
        cls.canyon = cv.imread(str(canyon / "portal-arrival.png"))
        cls.waypoint = cv.imread(str(canyon / "waypoint-arrival.png"))

    def test_current_journal_portal_and_canyon_landmarks_with_wrong_region_negatives(self):
        journal, stairs, vision = HorazonJournalObserver(), CanyonApproachObserver(), TownLoopVision()
        self.assertEqual((938, 369.5), journal.observe(self.before))
        self.assertEqual((800, 514), red_portal(self.portal))
        self.assertIsNone(red_portal(self.before))
        self.assertIsNone(red_portal(self.canyon))
        self.assertIsNone(journal.observe(self.canyon))
        self.assertIsNone(stairs.observe(self.before))
        self.assertIsNotNone(stairs.observe(self.canyon))
        for frame in (self.canyon, self.waypoint):
            gray = vision.normalize(frame)
            self.assertIsNotNone(vision.find(gray, "area_canyon"))
            self.assertIsNone(vision.find(gray, "area_arcane"))
        planner = TownLoopPlanner(vision, 1, "hell", "canyon")
        planner.data["phase"] = "destination_arrival"
        action = planner.step(self.waypoint)
        self.assertIn("waypoint", action["reason"].lower())
        self.assertEqual("return_panel", planner.data["phase"])

    def test_journal_immediate_floor_portal_then_current_canyon_waypoint(self):
        now, backend = [1.0], CanyonBackend()
        stop, capture, hud = FakeStop(now), Mock(target=dict(left=0, top=0)), Mock()
        frames = iter((self.before, self.before, self.portal, self.canyon, self.waypoint))

        def grab():
            now[0] += 0.01
            return SimpleNamespace(frame=next(frames))

        capture.grab.side_effect = grab
        hud.observe.return_value = SimpleNamespace(life=SimpleNamespace(ratio=1), mana=SimpleNamespace(ratio=1), belt_visibility="closed")
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
        ground = SimpleNamespace(observe=lambda *args, **kwargs: SimpleNamespace(point=(961, 578)))
        evidence = []
        result = enter_canyon(
            backend,
            capture,
            lambda: True,
            stop,
            hud,
            profile,
            (ground, None),
            TownLoopVision(),
            kill_confirmed=True,
            loot_complete=True,
            evidence=evidence,
            buff_expires_at=100,
            clock=lambda: now[0],
            emit=lambda message: None,
        )
        self.assertEqual("canyon_waypoint_observed", result["status"])
        self.assertEqual(4, backend.events.count(("click",)))
        self.assertIn("nearby floor immediately", result["actions"][1]["reason"])
        self.assertLess(result["actions"][1]["acquired_at"] - result["actions"][0]["acquired_at"], 0.2)

    def test_unconfirmed_kill_or_loot_rejects_before_observation_or_input(self):
        backend, capture = Mock(), Mock()
        for kill, loot in ((False, True), (True, False), (None, True)):
            with self.assertRaises(ValueError):
                enter_canyon(
                    backend,
                    capture,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    kill_confirmed=kill,
                    loot_complete=loot,
                    evidence=[],
                    buff_expires_at=100,
                )
        backend.assert_not_called()
        self.assertFalse(backend.method_calls)
        capture.grab.assert_not_called()

    def test_far_journal_uses_current_safe_teleport_before_click_and_dismiss(self):
        for mode in ("arrives", "no_floor", "no_landing"):
            with self.subTest(mode=mode):
                now, backend = [1.0], CanyonBackend()
                stop, capture, hud = FakeStop(now), Mock(target=dict(left=0, top=0)), Mock()
                frame = self.before

                def grab():
                    now[0] += 0.01
                    return SimpleNamespace(frame=frame)

                capture.grab.side_effect = grab
                hud.observe.return_value = SimpleNamespace(
                    life=SimpleNamespace(ratio=1), mana=SimpleNamespace(ratio=1), belt_visibility="closed"
                )
                # 같은 현재 일지까지 처음에는 멀고, 실제 F2 뒤에만 가까워진다.
                ground = SimpleNamespace(
                    observe=lambda *args, **kwargs: SimpleNamespace(
                        point=(938, 540) if ("press", "f2") in backend.events and mode == "arrives" else (938, 700)
                    )
                )
                path, vision = Mock(), Mock()
                path.choose.return_value = SimpleNamespace(aim=None if mode == "no_floor" else (938, 469.5))
                vision.normalize.side_effect = lambda value: value
                vision.find.side_effect = lambda image, key: (
                    (1, 1)
                    if key
                    in (("area_canyon", "hud", "world_canyon_waypoint") if backend.events.count(("click",)) == 2 else ("area_arcane",))
                    else None
                )
                profile = SimpleNamespace(
                    survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
                    actions=SimpleNamespace(movement_skill_key="f2"),
                )
                options = dict(
                    kill_confirmed=True,
                    loot_complete=True,
                    evidence=[],
                    buff_expires_at=100,
                    clock=lambda: now[0],
                    reference_seconds=0.32,
                    emit=lambda message: None,
                )
                args = (backend, capture, lambda: True, stop, hud, profile, (ground, path), vision)
                if mode == "arrives":
                    result = enter_canyon(*args, **options)
                    self.assertEqual("canyon_waypoint_observed", result["status"])
                    self.assertEqual(1, backend.events.count(("press", "f2")))
                    self.assertLess(backend.events.index(("press", "f2")), backend.events.index(("click",)))
                    self.assertEqual("Teleport near current Horazon journal", result["actions"][0]["reason"])
                    self.assertIn("Click current Horazon journal", result["actions"][1]["reason"])
                    self.assertIn("nearby floor immediately", result["actions"][2]["reason"])
                    self.assertLess(result["actions"][2]["acquired_at"] - result["actions"][1]["acquired_at"], 0.2)
                else:
                    with self.assertRaisesRegex(RuntimeError, "floor unverified" if mode == "no_floor" else "limit reached"):
                        enter_canyon(*args, **options)
                    self.assertNotIn(("click",), backend.events)
                    self.assertEqual(0 if mode == "no_floor" else 2, backend.events.count(("press", "f2")))
