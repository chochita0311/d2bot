import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

import cv2 as cv
import numpy as np

from diablo2.actions.town_loop_vision import TownLoopVision, TownLoopPlanner
from diablo2.common.field_control import DryFieldInput
from diablo2.runs.summoner.canyon_finish import CanyonApproachObserver, HorazonJournalObserver, enter_canyon, journal_landing, red_portal
from diablo2.runs.summoner.navigation_execution import load_arcane_step_planner
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

    def test_hovered_portal_requires_unique_destination_letters_red_support_and_dark_core(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        frame = cv.imread(str(owner / "journal-portal-hover.png"))
        self.assertEqual((1020, 510), red_portal(frame))
        shifted = cv.copyMakeBorder(frame[:-1, :-1], 1, 0, 1, 0, cv.BORDER_CONSTANT)
        self.assertEqual((1021, 511), red_portal(shifted))
        for left, right in ((925, 988), (988, 1052), (1052, 1115), (925, 1115)):
            changed = frame.copy()
            changed[497:523, left:right] = 0
            self.assertIsNone(red_portal(changed))
        label_only = np.zeros_like(frame)
        label_only[497:523, 925:1115] = frame[497:523, 925:1115]
        self.assertIsNone(red_portal(label_only))
        bright_core = frame.copy()
        bright_core[547:647, 990:1050] = 255
        self.assertIsNone(red_portal(bright_core))
        duplicate = frame.copy()
        duplicate[250:650, 1200:1600] = frame[350:750, 800:1200]
        self.assertIsNone(red_portal(duplicate))

    def test_dark_current_canyon_steps_use_only_reviewed_appearance_threshold(self):
        owner = Path(__file__).resolve().parents[1] / "assets/regions/canyon-of-the-magi/remastered-ko-1922x1140"
        frame = cv.imread(str(owner / "references/portal-arrival-dark-steps.png"))
        stairs = CanyonApproachObserver()
        self.assertEqual(0.90, stairs.rule["threshold"])
        self.assertEqual((1010.5, 154.5), stairs.observe(frame))
        shifted = cv.copyMakeBorder(frame[:-1, :-1], 1, 0, 1, 0, cv.BORDER_CONSTANT)
        self.assertEqual((1011.5, 155.5), stairs.observe(shifted))
        for left, right in ((951, 1010), (1010, 1070), (951, 1070)):
            changed = frame.copy()
            changed[129:180, left:right] = 0
            self.assertIsNone(stairs.observe(changed))
        self.assertEqual(0.92, TownLoopVision().manifest["templates"]["world_canyon_waypoint"]["threshold"])

    def test_journal_immediate_floor_portal_then_current_canyon_waypoint(self):
        now, backend = [1.0], CanyonBackend()
        stop, capture, hud = FakeStop(now), Mock(target=dict(left=0, top=0)), Mock()
        frames = (self.before, self.before, self.portal, self.canyon, self.waypoint)

        def grab():
            now[0] += 0.01
            return SimpleNamespace(frame=frames[min(backend.events.count(("click",)), len(frames) - 1)])

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
        self.assertEqual(1, [name for name, _ in evidence].count("journal-exit-last.png"))
        self.assertIs(self.waypoint, dict(evidence)["journal-exit-last.png"])

    def test_already_open_current_arcane_portal_skips_repeated_journal_and_floor_clicks(self):
        now, backend = [1.0], CanyonBackend()
        stop, capture, hud = FakeStop(now), Mock(target=dict(left=0, top=0)), Mock()
        frames = (self.portal, self.canyon, self.waypoint)

        def grab():
            now[0] += 0.01
            return SimpleNamespace(frame=frames[min(backend.events.count(("click",)), len(frames) - 1)])

        capture.grab.side_effect = grab
        hud.observe.return_value = SimpleNamespace(life=SimpleNamespace(ratio=1), mana=SimpleNamespace(ratio=1), belt_visibility="closed")
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
        result = enter_canyon(
            backend,
            capture,
            lambda: True,
            stop,
            hud,
            profile,
            (Mock(), Mock()),
            TownLoopVision(),
            kill_confirmed=True,
            loot_complete=True,
            evidence=[],
            buff_expires_at=100,
            clock=lambda: now[0],
            emit=lambda message: None,
        )
        self.assertEqual("canyon_waypoint_observed", result["status"])
        self.assertEqual(2, backend.events.count(("click",)))
        self.assertEqual("Enter current red portal to Canyon of the Magi", result["actions"][0]["reason"])
        self.assertFalse(any("journal" in action["reason"].lower() for action in result["actions"]))

    def test_portal_click_rechecks_current_frame_pointer_resources_focus_and_stop_after_aim(self):
        for condition in ("stale", "pointer", "resources", "focus", "stop"):
            with self.subTest(condition=condition):
                now, focused, grabs = [1.0], [True], [0]
                backend, capture, hud = CanyonBackend(), Mock(target=dict(left=0, top=0)), Mock()
                stop = FakeStop(now)

                def grab():
                    grabs[0] += 1
                    now[0] += 0.3 if grabs[0] > 1 and condition == "stale" else 0.01
                    if grabs[0] > 1:
                        focused[0] = condition != "focus"
                        if condition == "stop":
                            stop.set()
                    return SimpleNamespace(frame=self.portal)

                capture.grab.side_effect = grab
                hud.observe.side_effect = lambda _: SimpleNamespace(
                    life=SimpleNamespace(ratio=0.1 if grabs[0] > 1 and condition == "resources" else 1),
                    mana=SimpleNamespace(ratio=1),
                    belt_visibility="closed",
                )
                backend.pointer_matches = lambda _: condition != "pointer"
                profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
                with self.assertRaises(RuntimeError):
                    enter_canyon(
                        backend,
                        capture,
                        lambda: focused[0],
                        stop,
                        hud,
                        profile,
                        (Mock(), Mock()),
                        TownLoopVision(),
                        kill_confirmed=True,
                        loot_complete=True,
                        evidence=[],
                        buff_expires_at=100,
                        clock=lambda: now[0],
                        emit=lambda message: None,
                    )
                self.assertEqual(2, grabs[0])
                self.assertEqual(1, len(backend.events))
                self.assertEqual("move", backend.events[0][0])

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
                path.choose.return_value = SimpleNamespace(aim=None if mode == "no_floor" else (1098, 449.5))
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

    def test_journal_landing_uses_current_floor_beside_book_and_checks_other_side(self):
        path, view = Mock(), object()
        path.choose.side_effect = [SimpleNamespace(aim=(940, 470)), SimpleNamespace(aim=(778, 450))]
        self.assertEqual((778, 450), journal_landing(path, view, (938, 370), (961, 700)))
        self.assertIs(view, path.choose.call_args_list[0][0][0])
        self.assertEqual((137, -250), path.choose.call_args_list[0][1]["destination"])
        self.assertEqual((-183, -250), path.choose.call_args_list[1][1]["destination"])
        path.choose.reset_mock(side_effect=True)
        path.choose.return_value = SimpleNamespace(aim=(778, 450))
        self.assertEqual((778, 450), journal_landing(path, view, (938, 370), (700, 700)))
        self.assertEqual(1, path.choose.call_count)

    def test_journal_landing_rejects_body_overlap_distant_floor_and_unobserved_floor(self):
        for aim in (None, (938, 470), (1040, 450), (1098, 550), (1300, 450)):
            with self.subTest(aim=aim):
                path = Mock()
                path.choose.return_value = SimpleNamespace(aim=aim)
                self.assertIsNone(journal_landing(path, object(), (938, 370), (961, 700)))

    def test_stored_far_journals_select_safe_side_floor_without_changing_exploration_stride(self):
        ground, path = load_arcane_step_planner()
        settings, journal = dict(path.teleport), HorazonJournalObserver()
        owner = Path(__file__).resolve().parents[1] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/summoner"
        for pose in ("back-robe", "front-open", "hit-turn", "lower-robe", "raised-arm"):
            with self.subTest(pose=pose):
                frame = cv.imread(str(owner / ("platform-alive-" + pose + ".png")))
                player, point = ground.observe(frame, captured_at=1).point, journal.observe(frame)
                view = path.observe(frame, sequence=1, generation=0, captured_at=1, viewport=(120, 180, 1720, 890), anchor=player)
                self.assertIsNotNone(journal_landing(path, view, point, player))
                self.assertEqual(settings, path.teleport)
