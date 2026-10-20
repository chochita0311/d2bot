import threading
import time
import unittest
import json
import tempfile
from contextlib import ExitStack
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

import numpy as np

from diablo2.app import build_parser
from diablo2.runs.summoner.main_run import (
    BuffSiteGuard,
    clear_buff_site,
    enter_arcane,
    reusable_buffs,
    buff_renewal_deadlines,
    renew_supervised_field_buffs,
    run_north_segment,
    maintain_paused_field,
    validate_buff_site_permit,
    run_once,
    run,
    _json_scalar,
    return_to_act1_and_exit,
)
from diablo2.common.field_control import DryFieldInput
from diablo2.runs.summoner.navigation import PathChoice
from diablo2.runs.summoner.scene_vision import GroundReading, HoverCandidate
from diablo2.common.supervised_field import ensure_weapon_set
from diablo2.common.config import load_config
from diablo2.common.survival_vision import ResourceReading
from diablo2.common.encounters import SceneContext, VisualCandidate
from diablo2.common.survival import ObservationStamp


class SummonerMainRunTests(unittest.TestCase):
    def test_failed_room_finish_restores_hidden_map_only_with_current_clear_hud_and_focus(self):
        for obstruction in (None, "panel", "hud", "stop", "focus"):
            with self.subTest(obstruction=obstruction):
                capture, session, vision, planner = Mock(), Mock(), Mock(), Mock()
                capture.target = dict(width=1922)
                capture.grab.return_value = SimpleNamespace(frame=np.zeros((1140, 1922, 3), np.uint8))
                planner.data = dict(phase="destination_arrival", arcane_map_inspection=dict(status="hidden"))
                planner.step.side_effect = RuntimeError("current waypoint absent")
                session._accept_waypoint_cursor_shift.return_value = None
                vision.find.side_effect = lambda gray, name: (
                    dict(score=1)
                    if (name == "hud" and obstruction != "hud") or (name == "waypoint_header" and obstruction == "panel")
                    else None
                )
                stop = threading.Event()
                if obstruction == "stop":
                    stop.set()
                with patch("diablo2.actions.town_loop_vision.TownLoopPlanner", return_value=planner):
                    with self.assertRaises(RuntimeError):
                        return_to_act1_and_exit(
                            session, capture, lambda: obstruction != "focus", stop, vision, evidence=[], emit=lambda _: None
                        )
                self.assertEqual(1 if obstruction is None else 0, session._dispatch_town_action.call_count)
                if obstruction is None:
                    self.assertEqual("tab", session._dispatch_town_action.call_args[0][2]["key"])
                session._release_town_mouse.assert_called_once()

    def test_labels_off_assertion_requires_current_field_resume(self):
        with self.assertRaisesRegex(ValueError, "Arcane resumption"):
            run_once(Mock(), live=False, resume_arcane=False, resume_labels_off=True, casts=1, difficulty="hell", output=Path("unused"))
        args = build_parser().parse_args(["--summoner-run", "--resume-arcane", "--resume-labels-off"])
        self.assertTrue(args.resume_labels_off)

    def test_fresh_run_initializes_and_retains_route_state_through_room_finish(self):
        self._fresh_navigation_case()

    def test_missing_mercenary_stops_fresh_run_without_pause_exit_or_next_room(self):
        self._fresh_navigation_case("mercenary_unverified")

    def test_same_room_recovery_does_not_require_mercenary_portrait(self):
        with patch("diablo2.common.survival_vision.SurvivalHudObserver") as observer:
            self.assertEqual(
                0,
                run_once(
                    load_config(Path("config")),
                    live=False,
                    resume_arcane=True,
                    casts=1,
                    difficulty="hell",
                    output=Path("unused"),
                    continuous=True,
                    character_key="flash",
                    return_from="north",
                    finish_room=True,
                    emit=lambda _: None,
                ),
            )
        observer.assert_called_once_with(allow_high_orb_fallback=True, allow_partial_orb_fallback=True, observe_mercenary=False)

    def _fresh_navigation_case(self, failure=None):
        config = load_config(Path("config"))
        capture = Mock()
        capture.target = dict(left=0, top=0, width=1922, height=1140)
        capture.current_target.return_value = capture.target
        window = SimpleNamespace(handle=7)
        driver = Mock()
        actual_deadline = time.monotonic() + 140
        driver.execute.return_value = dict(
            observation={}, safety={}, buff={}, deadlines=[actual_deadline], preparation_at=actual_deadline - 4
        )
        intervals = []

        def navigate(*args, **kwargs):
            if failure:
                raise RuntimeError(failure)
            self.assertEqual(actual_deadline - 4, kwargs["buff_expires_at"])
            progress = kwargs["progress"]
            intervals.append(progress)
            if len(intervals) == 1:
                self.assertEqual([0.0, 0.0], progress["position"])
                self.assertEqual([], progress["steps"])
                return dict(
                    status="north_segment_observed",
                    position=[80.0, -40.0],
                    steps=[{"index": 0}],
                    breadcrumbs=[],
                    diagnostic_flags=dict(blocked=np.bool_(False), samples=np.int64(3), score=np.float32(0.5)),
                )
            self.assertEqual([80.0, -40.0], progress["position"])
            self.assertEqual([{"index": 0}], progress["steps"])
            return dict(status="hub_arrived", **progress)

        with tempfile.TemporaryDirectory(prefix="summoner-run-test-") as directory, ExitStack() as stack:
            replacements = {
                "diablo2.runs.summoner.main_run.enter_arcane": Mock(),
                "diablo2.runs.summoner.main_run.load_arcane_step_planner": Mock(return_value=(Mock(), Mock())),
                "diablo2.runs.summoner.main_run.AutomaticBuffDriver": Mock(return_value=driver),
                "diablo2.runs.summoner.main_run.ensure_automap_on": Mock(return_value={}),
                "diablo2.runs.summoner.main_run.return_to_act1_and_exit": Mock(return_value={}),
                "diablo2.runs.summoner.main_run.maintain_paused_field": Mock(),
                "diablo2.common.capture.ScreenCapture": Mock(return_value=capture),
                "diablo2.common.survival_vision.SurvivalHudObserver": Mock(),
                "diablo2.common.capture.resolve_window_from_config": Mock(return_value=window),
                "diablo2.common.capture.USER32": SimpleNamespace(GetForegroundWindow=lambda: 7, IsIconic=lambda _: False),
                "diablo2.actions.run_lifecycle.RunLifecycleSession": Mock(),
                "diablo2.common.conditional_loot.ConditionalLootController": Mock(),
                "diablo2.runs.summoner.camera_ground.CalibratedCameraGroundObserver": Mock(),
                "diablo2.runs.summoner.continuous_navigation.run_continuous_route": Mock(side_effect=navigate),
                "keyboard.add_hotkey": Mock(side_effect=lambda key, callback: callback),
                "keyboard.remove_hotkey": Mock(),
            }
            mocks = {name: stack.enter_context(patch(name, replacement)) for name, replacement in replacements.items()}
            camera = mocks["diablo2.runs.summoner.camera_ground.CalibratedCameraGroundObserver"].return_value
            camera.calibrations = camera.projections = 0
            camera.center, camera.authorized, camera.reset_events = (962, 578), True, []
            loot = mocks["diablo2.common.conditional_loot.ConditionalLootController"].return_value
            loot.prepare.return_value, loot.records, loot.labels_state = {}, [], "off"
            output = Path(directory) / "room"
            result = run_once(
                config,
                live=True,
                resume_arcane=False,
                casts=1,
                difficulty="hell",
                output=output,
                continuous=True,
                segments=2,
                finish_room=True,
                character_key="flash",
                emit=lambda _: None,
            )
            report = json.loads((output / "report.json").read_text(encoding="utf-8"))
            mocks["diablo2.common.survival_vision.SurvivalHudObserver"].assert_called_once_with(
                allow_high_orb_fallback=True, allow_partial_orb_fallback=True, observe_mercenary=True
            )
            mocks["diablo2.runs.summoner.main_run.maintain_paused_field"].assert_not_called()
            if failure:
                self.assertEqual(2, result)
                self.assertEqual("failed", report["status"])
                self.assertEqual([failure], report["errors"])
                self.assertEqual("mercenary_portrait_unverified", report["operator_review_required"])
                self.assertTrue(report["field_retained"])
                self.assertNotIn("safe_exit", report)
                mocks["diablo2.runs.summoner.main_run.return_to_act1_and_exit"].assert_not_called()
                return
            self.assertEqual(0, result, report["errors"])
            self.assertEqual("north_room_closed", report["status"])
            self.assertEqual([], report["errors"])
            self.assertEqual(2, len(intervals))
            self.assertEqual(dict(blocked=False, samples=3, score=0.5), report["navigation"]["diagnostic_flags"])
            callbacks = [call[0][1] for call in mocks["keyboard.add_hotkey"].call_args_list]
            self.assertEqual(len(callbacks), len(set(callbacks)))
            mocks["diablo2.runs.summoner.main_run.return_to_act1_and_exit"].assert_called_once()

    def test_room_exception_cannot_leave_cycle_summary_running_or_hide_unknown_json_objects(self):
        with tempfile.TemporaryDirectory(prefix="summoner-run-test-") as directory:
            output = Path(directory) / "cycles"
            with patch("diablo2.runs.summoner.main_run.run_once", side_effect=TypeError("unserializable diagnostic")):
                with self.assertRaisesRegex(TypeError, "unserializable diagnostic"):
                    run(Mock(), north_cycles=1, live=True, continuous=True, output=output)
            summary = json.loads((output / "report.json").read_text(encoding="utf-8"))
            self.assertEqual("failed", summary["status"])
            self.assertEqual(0, summary["completed"])
            self.assertEqual("unserializable diagnostic", summary["error"])
        with self.assertRaises(TypeError):
            json.dumps(dict(unknown=object()), default=_json_scalar)

    def test_paused_resource_recovery_preempts_defense_and_reobserves_before_continuing(self):
        now, stop = [0.0], threading.Event()
        frame = object()
        capture, hud, backend = Mock(), Mock(), DryFieldInput()
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
        low = SimpleNamespace(life=ResourceReading(900, 1000), mana=ResourceReading(50, 1000), belt_visibility="closed")
        high = SimpleNamespace(life=ResourceReading(900, 1000), mana=ResourceReading(900, 1000), belt_visibility="closed")

        def grab():
            now[0] += 0.1
            if now[0] >= 0.3:
                stop.set()
            return SimpleNamespace(frame=frame)

        capture.grab.side_effect = grab
        hud.observe.side_effect = [low, high, high]
        recovery, report = Mock(return_value=True), {}
        maintain_paused_field(
            capture,
            hud,
            lambda: True,
            profile,
            stop,
            report=report,
            evidence=[],
            backend=backend,
            engage_monsters=False,
            recover_resources=recovery,
            clock=lambda: now[0],
        )
        recovery.assert_called_once_with(frame, low, 0.0)
        self.assertGreater(hud.observe.call_count, 1)
        self.assertEqual("operator_stop", report["paused_survival_guard"]["status"])
        self.assertFalse(backend.events)

    def test_same_process_renewal_requires_new_safety_dispatch_and_effect_before_new_deadline(self):
        profile = SimpleNamespace(
            survival=SimpleNamespace(buffs=[SimpleNamespace(key="s", name="BO", duration_seconds=150, renew_before_seconds=10)])
        )
        dispatched = dict(
            status="dispatched_unconfirmed", buff_dispatches=[dict(key="s", cast_request_at=1000, cast_request_monotonic=100)]
        )
        safety = dict(area="arcane_sanctuary", observed_at=1001)
        effect = dict(safety, effect_verified=True)
        accepted = []

        def wait(signals, **kwargs):
            signal = safety if kwargs["phase"] == "field_safety" else effect
            kwargs["accept"](signal)
            accepted.append(kwargs["phase"])
            return signal

        for dispatch_ok, effect_ok, now in ((True, True, 101), (False, True, 101), (True, False, 101), (True, True, 240)):
            accepted[:] = []
            record, backend = {}, DryFieldInput()
            with patch("diablo2.runs.summoner.main_run.wait_probe_signal", side_effect=wait), patch(
                "diablo2.runs.summoner.main_run.validate_buff_site_permit"
            ), patch("diablo2.runs.summoner.main_run.BuffSiteGuard") as guard, patch(
                "diablo2.runs.summoner.main_run.dispatch_buffs", return_value=dispatched if dispatch_ok else dict(status="interrupted")
            ), patch(
                "diablo2.runs.summoner.main_run.confirm_initial_probe", side_effect=None if effect_ok else ValueError("effect missing")
            ), patch(
                "diablo2.runs.summoner.main_run.time.monotonic", return_value=now
            ):
                guard.return_value.observations = []
                call = lambda: renew_supervised_field_buffs(
                    Mock(),
                    Mock(),
                    Mock(),
                    backend,
                    lambda: True,
                    profile,
                    threading.Event(),
                    Mock(),
                    Mock(),
                    record=record,
                    evidence=[],
                    prefix="renewal",
                    emit=lambda message: None,
                )
                if dispatch_ok and effect_ok and now < 240:
                    proof, deadlines = call()
                    self.assertIs(effect, proof)
                    self.assertEqual([240], deadlines)
                    self.assertEqual(["field_safety", "initial_effect"], accepted)
                else:
                    with self.assertRaises((RuntimeError, ValueError)):
                        call()
                    self.assertNotIn("deadlines", record)
                self.assertFalse(backend.events)
        with self.assertRaisesRegex(RuntimeError, "missing"):
            buff_renewal_deadlines(dict(buff_dispatches=[]), profile)

    def test_paused_unknown_appearance_uses_current_hover_panel_before_nova(self):
        for hostile, combat, expires in ((True, True, 10), (False, True, 10), (True, False, 10), (True, True, 0)):
            now, loops = [0.0], [0]
            capture, hud, hazards, target = Mock(), Mock(), Mock(), Mock()
            capture.target = dict(left=0, top=0)
            capture.grab.return_value = SimpleNamespace(frame=SimpleNamespace(shape=(1140, 1922, 3)))
            hud.observe.return_value = SimpleNamespace(
                life=ResourceReading(900, 1000), mana=ResourceReading(800, 1000), belt_visibility="closed"
            )
            hazards.observe.return_value = ()
            target.observe.return_value = object() if hostile else None
            ground = SimpleNamespace(observe=lambda *args, **kwargs: GroundReading((962, 580), "test"))
            stop = Mock(wraps=threading.Event())

            def wait(seconds):
                now[0] += 0.1
                loops[0] += 1
                if loops[0] >= 4:
                    stop.set()

            stop.wait.side_effect = wait
            profile = SimpleNamespace(
                survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1),
                actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"),
            )
            backend, report = DryFieldInput(), {}
            maintain_paused_field(
                capture,
                hud,
                lambda: True,
                profile,
                stop,
                report=report,
                evidence=[],
                clock=lambda: now[0],
                backend=backend,
                ground=ground,
                hazards=hazards,
                buff_expires_at=expires,
                engage_monsters=combat,
                target_vision=target,
            )
            self.assertEqual(hostile and combat and expires > 0, ("down", "f4") in backend.events)
            self.assertNotIn(("down", "f2"), backend.events)
            if not combat or expires == 0:
                self.assertFalse(backend.events)
                target.observe.assert_not_called()

    def test_buff_guard_prepares_camera_with_the_same_current_hud_and_frame(self):
        frame = object()
        capture = Mock()
        capture.grab.return_value = SimpleNamespace(frame=frame)
        reading = SimpleNamespace(life=ResourceReading(800, 1000), mana=ResourceReading(900, 1000), belt_visibility="closed")
        hud, hazards = Mock(), Mock()
        hud.observe.return_value, hazards.observe.return_value = reading, ()
        context = []
        ground = SimpleNamespace(
            prepare_frame=lambda *args: context.append(args),
            observe=lambda seen, captured_at: GroundReading(
                (962, 580) if context == [(seen, captured_at, reading, True)] else None, "camera"
            ),
        )
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
        guard = BuffSiteGuard(capture, hud, lambda: True, profile, threading.Event(), ground, hazards, clock=lambda: 10)
        self.assertIsNone(guard.check())
        self.assertEqual([(frame, 10, reading, True)], context)
        context.clear()
        capture.reset_mock()
        hud.reset_mock()
        self.assertIsNone(guard.wait_check(frame, 10, reading))
        self.assertEqual([(frame, 10, reading, True)], context)
        capture.grab.assert_not_called()
        hud.observe.assert_not_called()
        self.assertEqual("screen_stale", guard.check(frame, 9.7, reading))
        self.assertEqual("screen_stale", guard.wait_check(frame, 9.7, reading))
        capture.grab.assert_not_called()
        hud.observe.assert_not_called()

    def test_paused_nearby_defense_requires_current_pose_frame_buff_and_combat_toggle(self):
        from pathlib import Path

        profile = load_config(Path(__file__).resolve().parents[1] / "config").characters["flash"]
        for condition in ("near", "far", "off", "expired", "pose_unknown", "stale"):
            with self.subTest(condition=condition):
                now, loops = [0.0], [0]
                capture, hud, hazards = Mock(), Mock(), Mock()
                stop = Mock(wraps=threading.Event())

                def wait(seconds):
                    now[0] += 0.05
                    loops[0] += 1
                    if loops[0] >= 3:
                        stop.set()

                stop.wait.side_effect = wait
                capture.grab.return_value = SimpleNamespace(frame=object())
                hud.observe.return_value = SimpleNamespace(
                    life=ResourceReading(900, 1000), mana=ResourceReading(800, 1000), belt_visibility="closed"
                )
                ground = SimpleNamespace(
                    observe=lambda frame, captured_at: GroundReading(None if condition == "pose_unknown" else (960, 580), "test")
                )

                def scan(*args):
                    if condition == "stale":
                        now[0] += 0.3
                    return (SimpleNamespace(ground=(1500, 300) if condition == "far" else (1000, 600)),)

                hazards.observe.side_effect = scan
                backend, report = DryFieldInput(), {}
                maintain_paused_field(
                    capture,
                    hud,
                    lambda: True,
                    profile,
                    stop,
                    report=report,
                    evidence=[],
                    clock=lambda: now[0],
                    backend=backend,
                    ground=ground,
                    hazards=hazards,
                    buff_expires_at=0 if condition == "expired" else 10,
                    engage_monsters=condition != "off",
                )
                self.assertNotIn(("down", "f2"), backend.events)
                if condition == "near":
                    self.assertIn(("down", "f4"), backend.events)
                    self.assertEqual(("up", "f4"), backend.events[-1])
                else:
                    self.assertFalse(backend.events)
                if condition in ("off", "pose_unknown"):
                    hazards.observe.assert_not_called()

    def test_paused_route_keeps_monitoring_and_detects_actual_low_resources(self):
        now = [0.0]
        capture, hud, stop = Mock(), Mock(), Mock()
        capture.grab.return_value = SimpleNamespace(frame=object())
        stop.is_set.return_value = False
        stop.wait.side_effect = lambda seconds: now.__setitem__(0, now[0] + seconds)
        safe = SimpleNamespace(life=ResourceReading(900, 1000), mana=ResourceReading(800, 1000), belt_visibility="closed")
        low = SimpleNamespace(life=ResourceReading(400, 1000), mana=ResourceReading(800, 1000), belt_visibility="closed")
        hud.observe.side_effect = [safe, safe, low]
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
        report, evidence = {}, []
        with self.assertRaisesRegex(RuntimeError, "resource_low"):
            maintain_paused_field(capture, hud, lambda: True, profile, stop, report=report, evidence=evidence, clock=lambda: now[0])
        self.assertEqual(3, report["paused_survival_guard"]["observations"])
        self.assertEqual("resource_low", report["paused_survival_guard"]["status"])
        self.assertEqual(1, len(evidence))

    def test_paused_guard_does_not_abandon_unreadable_hud_silently(self):
        now = [0.0]
        capture, hud, stop = Mock(), Mock(), Mock()
        capture.grab.return_value = SimpleNamespace(frame=object())
        stop.is_set.return_value = False
        stop.wait.side_effect = lambda seconds: now.__setitem__(0, now[0] + seconds)
        hud.observe.return_value = SimpleNamespace(life=ResourceReading(), mana=ResourceReading(), belt_visibility="closed")
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
        report = {}
        with self.assertRaisesRegex(RuntimeError, "paused_hud_unverified"):
            maintain_paused_field(capture, hud, lambda: True, profile, stop, report=report, evidence=[], clock=lambda: now[0])
        self.assertGreaterEqual(now[0], 2)
        self.assertEqual("hud_unverified", report["paused_survival_guard"]["status"])

    def test_paused_guard_hands_control_back_on_focus_loss_or_explicit_stop(self):
        for focused, stopped, expected in ((False, False, "focus_handoff"), (True, True, "operator_stop")):
            capture, stop, report = Mock(), Mock(), {}
            stop.is_set.return_value = stopped
            maintain_paused_field(capture, Mock(), lambda: focused, Mock(), stop, report=report, evidence=[])
            capture.grab.assert_not_called()
            self.assertEqual(expected, report["paused_survival_guard"]["status"])

    def test_nearby_buff_permit_allows_distant_enemy_only_with_explicit_scoped_proof(self):
        from pathlib import Path

        policy = load_config(Path(__file__).resolve().parents[1] / "config").characters["flash"].survival
        proof = dict(
            character_id="flash",
            field=True,
            monsters_clear=False,
            belt_closed=True,
            potions_remaining=3,
            observed_at=100,
            source="supervised_visual",
            buff_safety_scope="nearby",
            nearby_monsters_clear=True,
        )
        validate_buff_site_permit(proof, policy, 110)
        for update in (
            dict(nearby_monsters_clear=False),
            dict(source="key_dispatch"),
            dict(observed_at=94),
            dict(buff_safety_scope="unknown"),
        ):
            with self.assertRaises(ValueError):
                validate_buff_site_permit(dict(proof, **update), policy, 110)

    def test_buff_stability_uses_current_ratios_and_vetoes_drop_or_nearby_enemy(self):
        now = [0.0]
        capture, hud, ground, hazards = Mock(), Mock(), Mock(), Mock()

        def grab():
            now[0] += 0.1
            return SimpleNamespace(frame=object())

        capture.grab.side_effect = grab
        hud.observe.return_value = SimpleNamespace(
            life=ResourceReading(800, 1000), mana=ResourceReading(1800, 2000), belt_visibility="closed"
        )
        ground.observe.return_value = GroundReading((960, 580), "test")
        hazards.observe.return_value = [SimpleNamespace(ground=(1500, 300))]
        profile = SimpleNamespace(survival=SimpleNamespace(life_potion_below=0.5, mana_potion_below=0.1))
        guard = BuffSiteGuard(capture, hud, lambda: True, profile, threading.Event(), ground, hazards, clock=lambda: now[0])
        result = guard.prepare()
        self.assertGreaterEqual(result["window_seconds"], 1)
        # 장비/버프에 따라 최대값이 달라져도 같은 비율이면 허용한다.
        hud.observe.return_value = SimpleNamespace(
            life=ResourceReading(1600, 2000), mana=ResourceReading(900, 1000), belt_visibility="closed"
        )
        self.assertIsNone(guard.check())
        hud.observe.return_value = SimpleNamespace(
            life=ResourceReading(1600, 2000), mana=ResourceReading(lower_bound=0.9), belt_visibility="closed"
        )
        self.assertIsNone(guard.check())
        hud.observe.return_value = SimpleNamespace(
            life=ResourceReading(1400, 2000), mana=ResourceReading(900, 1000), belt_visibility="closed"
        )
        self.assertEqual("buff_resources_unstable", guard.check())
        hazards.observe.return_value = [SimpleNamespace(ground=(1000, 600))]
        self.assertEqual("buff_nearby_hostile", guard.check())
        hazards.observe.return_value = [SimpleNamespace(ground=None)]
        self.assertEqual("buff_nearby_hostile", guard.check())

    def test_buff_site_nova_repeats_and_releases_without_claiming_clear(self):
        now = [0.0]
        capture = Mock()

        def grab():
            now[0] += 0.1
            return SimpleNamespace(frame=object())

        capture.grab.side_effect = grab
        backend = DryFieldInput()
        profile = SimpleNamespace(survival=object(), actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"))
        with patch(
            "diablo2.runs.summoner.main_run.resource_guard", side_effect=lambda *args: "observation_unverified" if now[0] < 0.3 else None
        ):
            result = clear_buff_site(
                capture, Mock(), backend, lambda: True, profile, threading.Event(), 0.32, evidence=[], clock=lambda: now[0]
            )
        self.assertEqual("attack_finished_clear_unconfirmed", result["status"])
        self.assertGreater(list(backend.events).count(("down", "f4")), 1)
        self.assertNotIn(("down", "f2"), backend.events)
        self.assertEqual(("up", "f4"), list(backend.events)[-1])

    def test_same_scene_buff_resume_retains_cast_timer_and_rejects_expiry_or_clock_change(self):
        profile = SimpleNamespace(survival=SimpleNamespace(buffs=[SimpleNamespace(key="s", duration_seconds=180, renew_before_seconds=40)]))
        report = dict(
            supervised_effect=dict(
                character_id="flash",
                source="supervised_visual",
                observed_at=1010,
                effect_verified=True,
                equipment_restored=True,
                sequence_effects_verified=True,
            ),
            buff=dict(status="dispatched_unconfirmed", buff_dispatches=[dict(key="s", cast_request_at=1000, cast_request_monotonic=100)]),
        )
        self.assertEqual([240], reusable_buffs(report, profile, wall_now=1100, monotonic_now=200)[1])
        self.assertIsNone(reusable_buffs(report, profile, wall_now=1135, monotonic_now=235))
        self.assertIsNone(reusable_buffs(report, profile, wall_now=1100, monotonic_now=102))
        self.assertIsNone(reusable_buffs(dict(report, safe_exit="character_select_observed"), profile, wall_now=1100, monotonic_now=200))
        report["supervised_effect"]["effect_verified"] = False
        self.assertIsNone(reusable_buffs(report, profile, wall_now=1100, monotonic_now=200))

    def segment(self, landings, *, stop=None, readable=True, focused=True, hazards=None):
        capture = Mock()
        capture.target = dict(left=100, top=200)
        capture.grab.return_value = SimpleNamespace(frame=object())
        profile = SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object())
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.resource_guard", return_value=None if readable else "observation_unverified"
        ), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step",
            return_value=(PathChoice((1060, 528), "connected_floor"), GroundReading((960, 578), "test"), object()),
        ) as plan, patch(
            "diablo2.runs.summoner.main_run.dispatch_planned_step", return_value=True
        ) as dispatch, patch(
            "diablo2.runs.summoner.main_run.observe_step_landing", side_effect=landings
        ) as landing:
            error = None
            try:
                result = run_north_segment(
                    capture,
                    Mock(),
                    Mock(),
                    lambda: focused,
                    profile,
                    stop or threading.Event(),
                    (Mock(), Mock()),
                    dict(observed_at=time.time()),
                    3,
                    0.32,
                    evidence=[],
                    emit=lambda value: None,
                    hazards=hazards,
                )
            except RuntimeError as exc:
                result, error = None, str(exc)
            return result, plan.call_count, dispatch.call_count, landing.call_count, error

    def test_main_exposes_bounded_explicit_live_and_resume_switches(self):
        parser = build_parser()
        args = parser.parse_args(["--summoner-run"])
        self.assertFalse(args.live)
        self.assertFalse(args.resume_arcane)
        self.assertEqual(3, args.north_casts)
        self.assertEqual(1, args.north_segments)
        args = parser.parse_args(["--summoner-run", "--live", "--resume-arcane", "--north-casts", "1"])
        self.assertEqual("on", args.automap)
        self.assertEqual("preserve", parser.parse_args(["--summoner-run", "--automap", "preserve"]).automap)
        self.assertTrue(args.live and args.resume_arcane)
        self.assertEqual(2, parser.parse_args(["--summoner-run", "--north-cycles", "2"]).north_cycles)

    def test_next_segment_preserves_position_history_and_unique_evidence_names(self):
        capture = Mock()
        capture.target = dict(left=0, top=0)
        capture.grab.return_value = SimpleNamespace(frame=object())
        progress = dict(steps=[], position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)])
        evidence = []
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.resource_guard", return_value=None
        ), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step",
            return_value=(PathChoice((1060, 528), "test"), GroundReading((960, 578), "test"), object()),
        ) as plan, patch(
            "diablo2.runs.summoner.main_run.dispatch_planned_step", return_value=True
        ), patch(
            "diablo2.runs.summoner.main_run.observe_step_landing", return_value=dict(status="arrival_observed", displacement=[80, -40])
        ):
            for _ in range(2):
                result = run_north_segment(
                    capture,
                    Mock(),
                    Mock(),
                    lambda: True,
                    SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object()),
                    threading.Event(),
                    (Mock(), Mock()),
                    {},
                    1,
                    0.32,
                    evidence=evidence,
                    progress=progress,
                    emit=lambda _: None,
                )
        self.assertEqual([160.0, -80.0], result["position"])
        self.assertEqual([0, 1], [step["index"] for step in result["steps"]])
        self.assertEqual(((0.0, 0.0), (80.0, -40.0)), plan.call_args[1]["visited"])
        self.assertEqual(4, len({name for name, _ in evidence}))

    def test_three_arrivals_replan_each_step_in_one_main_segment(self):
        result, plans, casts, landings, error = self.segment([dict(status="arrival_observed", displacement=[80, -40])] * 3)
        self.assertEqual("north_segment_observed", result["status"])
        self.assertEqual((3, 3, 3, None), (plans, casts, landings, error))

    def retry_transient_plan(self, reason):
        capture = Mock()
        capture.target = dict(left=0, top=0)
        frames = [object(), object(), object()]
        capture.grab.side_effect = [SimpleNamespace(frame=frame) for frame in frames]
        choice = PathChoice((1060, 528), "connected_floor")
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.resource_guard", return_value=None
        ), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step",
            side_effect=[RuntimeError(reason), (choice, GroundReading((960, 578), "test"), object())],
        ) as plan, patch(
            "diablo2.runs.summoner.main_run.dispatch_planned_step", return_value=True
        ) as dispatch, patch(
            "diablo2.runs.summoner.main_run.observe_step_landing", return_value=dict(status="arrival_observed", displacement=[80, -40])
        ):
            profile = SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object())
            result = run_north_segment(
                capture, Mock(), Mock(), lambda: True, profile, threading.Event(), (Mock(), Mock()), {}, 1, 0.32, evidence=[]
            )
        self.assertEqual("north_segment_observed", result["status"])
        self.assertEqual(2, plan.call_count)
        self.assertIs(frames[1], plan.call_args[0][0])
        self.assertEqual(1, dispatch.call_count)

    def test_transient_ground_or_floor_occlusion_waits_for_new_frame_before_cast(self):
        for reason in ("ground_unverified", "no_observed_progress", "player_floor_unobserved"):
            with self.subTest(reason=reason):
                self.retry_transient_plan(reason)

    def test_planning_reserves_time_for_post_pointer_capture_and_hover(self):
        now, calls = [100.0], [0]
        capture = Mock()
        capture.target = dict(left=0, top=0)
        before, current, after = object(), object(), object()
        capture.grab.side_effect = [SimpleNamespace(frame=frame) for frame in (before, current, after)]

        def plan(*args, **kwargs):
            calls[0] += 1
            if calls[0] == 1:
                now[0] += 0.14
            return PathChoice((1060 + calls[0], 528), "test"), GroundReading((960, 578), "test"), object()

        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.time.monotonic", side_effect=lambda: now[0]
        ), patch("diablo2.runs.summoner.main_run.resource_guard", return_value=None), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step", side_effect=plan
        ), patch(
            "diablo2.runs.summoner.main_run.dispatch_planned_step", return_value=True
        ) as dispatch, patch(
            "diablo2.runs.summoner.main_run.observe_step_landing", return_value=dict(status="arrival_observed", displacement=[80, -40])
        ):
            result = run_north_segment(
                capture,
                Mock(),
                Mock(),
                lambda: True,
                SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object()),
                threading.Event(),
                (Mock(), Mock()),
                {},
                1,
                0.32,
                evidence=[],
                emit=lambda _: None,
            )
        self.assertEqual("north_segment_observed", result["status"])
        self.assertEqual(2, calls[0])
        self.assertEqual(1, dispatch.call_count)
        self.assertEqual((1062, 528), dispatch.call_args[0][3])
        self.assertAlmostEqual(100.14, dispatch.call_args[0][5])

    def test_verified_motion_updates_world_position_and_next_plan_breadcrumbs(self):
        capture = Mock()
        capture.target = dict(left=0, top=0)
        capture.grab.return_value = SimpleNamespace(frame=object())
        progress = dict(steps=[], position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)])
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.resource_guard", return_value=None
        ), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step",
            side_effect=[(PathChoice((1060, 528), "test"), GroundReading((960, 578), "test"), object()), RuntimeError("floor_unobserved")],
        ) as plan, patch(
            "diablo2.runs.summoner.main_run.dispatch_planned_step", return_value=True
        ) as dispatch, patch(
            "diablo2.runs.summoner.main_run.observe_step_landing", return_value=dict(status="arrival_observed", displacement=[80, -40])
        ):
            evidence = []
            with self.assertRaisesRegex(RuntimeError, "floor_unobserved"):
                run_north_segment(
                    capture,
                    Mock(),
                    Mock(),
                    lambda: True,
                    SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object()),
                    threading.Event(),
                    (Mock(), Mock()),
                    {},
                    2,
                    0.32,
                    evidence=evidence,
                    progress=progress,
                    emit=lambda _: None,
                )
        self.assertEqual([80.0, -40.0], progress["position"])
        self.assertEqual(((0.0, 0.0), (80.0, -40.0)), plan.call_args[1]["visited"])
        self.assertEqual((80.0, -40.0), plan.call_args[1]["position"])
        self.assertEqual(1, dispatch.call_count)
        self.assertEqual("floor_unobserved", progress["planning_failure"]["reason"])
        self.assertEqual("north-01-planning-failed.png", evidence[-1][0])

    def test_uncertain_first_arrival_cannot_send_second_cast(self):
        result, plans, casts, landings, _ = self.segment([dict(status="arrival_unverified", displacement=[0, 0])])
        self.assertEqual("north_failed", result["status"])
        self.assertEqual((1, 1, 1), (plans, casts, landings))
        self.assertEqual([0.0, 0.0], result["position"])
        self.assertEqual([(0.0, 0.0)], result["breadcrumbs"])

    def test_expired_buff_cannot_start_navigation_after_supervision_wait(self):
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.time.monotonic", return_value=200
        ), patch("diablo2.runs.summoner.main_run.plan_arcane_step") as plan:
            with self.assertRaisesRegex(RuntimeError, "buff_renewal_required"):
                run_north_segment(
                    Mock(),
                    Mock(),
                    Mock(),
                    lambda: True,
                    SimpleNamespace(survival=object()),
                    threading.Event(),
                    (Mock(), Mock()),
                    {},
                    1,
                    0.32,
                    evidence=[],
                    buff_expires_at=199,
                )
        plan.assert_not_called()

    def test_planning_retry_cannot_extend_the_whole_segment_deadline(self):
        now, calls = [100.0], [0]
        capture = Mock()
        capture.target = dict(left=0, top=0)
        capture.grab.return_value = SimpleNamespace(frame=object())

        def plan(*args, **kwargs):
            calls[0] += 1
            if calls[0] == 2:
                now[0] = 112.02
                raise RuntimeError("ground_unverified")
            return PathChoice((1060, 528), "test"), GroundReading((960, 578), "test"), object()

        def land(*args, **kwargs):
            now[0] = 111.9
            return dict(status="arrival_observed", displacement=[80, -40])

        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.time.monotonic", side_effect=lambda: now[0]
        ), patch("diablo2.runs.summoner.main_run.resource_guard", return_value=None), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step", side_effect=plan
        ), patch(
            "diablo2.runs.summoner.main_run.dispatch_planned_step", return_value=True
        ) as dispatch, patch(
            "diablo2.runs.summoner.main_run.observe_step_landing", side_effect=land
        ) as landing:
            with self.assertRaisesRegex(RuntimeError, "ground_unverified"):
                run_north_segment(
                    capture,
                    Mock(),
                    Mock(),
                    lambda: True,
                    SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object()),
                    threading.Event(),
                    (Mock(), Mock()),
                    {},
                    2,
                    0.32,
                    evidence=[],
                    emit=lambda _: None,
                )
        self.assertEqual(2, calls[0])
        self.assertEqual(1, dispatch.call_count)
        self.assertEqual(112, dispatch.call_args[1]["expires_at"])
        self.assertEqual(112, landing.call_args[1]["expires_at"])

    def test_current_monster_candidate_stops_before_navigation_dispatch(self):
        context = SceneContext(ObservationStamp("flash", "test", 0, 0), "arcane-sanctuary", 0, (1922, 1140), (0, 0, 1922, 1140))
        candidate = VisualCandidate(context, "ghoul-alive", "ghoul-lord", "monster", "alive", 0.95, (700, 400, 800, 560), (770, 550))
        hazards = Mock()
        hazards.observe.return_value = (candidate,)
        result, plans, casts, landings, error = self.segment([], hazards=hazards)
        self.assertEqual("north_hazard_review", result["status"])
        self.assertEqual((1, 0, 0, None), (plans, casts, landings, error))

    def test_main_checks_hover_on_new_post_pointer_frame_before_cast(self):
        capture, backend, hover = Mock(), Mock(), Mock()
        capture.target = dict(left=100, top=200)
        before, aimed = object(), object()
        capture.grab.side_effect = [SimpleNamespace(frame=before), SimpleNamespace(frame=aimed)]
        hover.observe.return_value = (HoverCandidate("coffin", (1040, 500, 1100, 550), 0.95),)
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.resource_guard", return_value=None
        ), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step",
            return_value=(PathChoice((1060, 528), "test"), GroundReading((960, 578), "test"), object()),
        ):
            result = run_north_segment(
                capture,
                Mock(),
                backend,
                lambda: True,
                SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object()),
                threading.Event(),
                (Mock(), Mock()),
                {},
                1,
                0.32,
                evidence=[],
                hover_vision=hover,
                emit=lambda _: None,
            )
        self.assertEqual("north_failed", result["status"])
        hover.observe.assert_called_once_with(aimed, (1060, 528))
        backend.move.assert_called_once_with(1160, 728)
        backend.press.assert_not_called()
        self.assertEqual("coffin", result["steps"][0]["aim_check"]["hover"][0]["kind"])

    def test_current_and_landing_hazard_result_is_required_before_any_cast(self):
        path_started = threading.Event()
        frame = object()
        capture, ground, hazards = Mock(), Mock(), Mock()
        capture.target = dict(left=0, top=0)
        capture.grab.return_value = SimpleNamespace(frame=frame)
        ground.observe.return_value = GroundReading((960, 578), "test")
        context = SceneContext(ObservationStamp("flash", "test", 0, 0), "arcane-sanctuary", 0, (1922, 1140), (0, 0, 1922, 1140))
        candidate = VisualCandidate(context, "ghoul-alive", "ghoul-lord", "monster", "alive", 0.95, (700, 400, 800, 560), (770, 550))

        def scan(current, point, captured_at, now, *, aim):
            self.assertIs(frame, current)
            self.assertEqual((960, 578), point)
            self.assertEqual((1060, 528), aim)
            self.assertTrue(path_started.is_set())
            return (candidate,)

        def plan(current, *args, **kwargs):
            self.assertIs(frame, current)
            path_started.set()
            return PathChoice((1060, 528), "test"), ground.observe.return_value, object()

        hazards.observe.side_effect = scan
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.resource_guard", return_value=None
        ), patch("diablo2.runs.summoner.main_run.plan_arcane_step", side_effect=plan), patch(
            "diablo2.runs.summoner.main_run.dispatch_planned_step"
        ) as dispatch:
            result = run_north_segment(
                capture,
                Mock(),
                Mock(),
                lambda: True,
                SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object()),
                threading.Event(),
                (ground, Mock()),
                {},
                1,
                0.32,
                evidence=[],
                hazards=hazards,
                emit=lambda _: None,
            )
        self.assertEqual("north_hazard_review", result["status"])
        dispatch.assert_not_called()

    def test_transient_post_pointer_hud_miss_replans_without_repeating_a_cast(self):
        capture, backend, hover = Mock(), Mock(), Mock()
        capture.target = dict(left=0, top=0)
        frames = [object() for _ in range(5)]
        capture.grab.side_effect = [SimpleNamespace(frame=frame) for frame in frames]
        hover.observe.return_value = ()
        evidence = []
        with patch("diablo2.runs.summoner.main_run.validate_permit"), patch(
            "diablo2.runs.summoner.main_run.resource_guard", side_effect=(None, "observation_unverified", None, None)
        ), patch(
            "diablo2.runs.summoner.main_run.plan_arcane_step",
            return_value=(PathChoice((1060, 528), "test"), GroundReading((960, 578), "test"), object()),
        ) as plan, patch(
            "diablo2.runs.summoner.main_run.observe_step_landing", return_value=dict(status="arrival_observed", displacement=[80, -40])
        ):
            result = run_north_segment(
                capture,
                Mock(),
                backend,
                lambda: True,
                SimpleNamespace(actions=SimpleNamespace(movement_skill_key="f2"), survival=object()),
                threading.Event(),
                (Mock(), Mock()),
                {},
                1,
                0.32,
                evidence=evidence,
                hover_vision=hover,
                emit=lambda _: None,
            )
        self.assertEqual("north_segment_observed", result["status"])
        self.assertEqual(2, plan.call_count)
        self.assertIs(frames[2], plan.call_args[0][0])
        self.assertEqual(2, backend.move.call_count)
        backend.press.assert_called_once_with("f2")
        self.assertEqual(1, len(result["steps"]))
        self.assertEqual("observation_unverified", result["steps"][0]["dispatch_retries"][0]["reason"])
        self.assertEqual(3, len(evidence))
        self.assertIs(frames[2], evidence[0][1])

    def test_unreadable_resources_and_unfocused_state_send_no_cast(self):
        for kwargs in (dict(readable=False), dict(focused=False)):
            result, _, casts, _, error = self.segment([], **kwargs)
            self.assertIsNone(result)
            self.assertEqual(0, casts)
            self.assertIsNotNone(error)

    def test_explicit_stop_sends_no_cast(self):
        stop = threading.Event()
        stop.set()
        _, _, casts, _, error = self.segment([], stop=stop)
        self.assertEqual(0, casts)
        self.assertIn("interrupted", error)

    def test_entry_failure_does_not_proceed_as_arrival(self):
        session = Mock()
        session.is_running = False
        session.events.empty.return_value = True
        session.last_result = "failed"
        with self.assertRaisesRegex(RuntimeError, "did not pass"):
            enter_arcane(session, "hell", lambda value: None)
        session.start.assert_called_once_with(1, "hell", town_loop=True, destination="arcane", arrival_only=True, reset_act2_start=True)

    def test_primary_weapon_set_recovery_switches_only_after_current_II_proof(self):
        capture, backend, observer = Mock(), Mock(), Mock()
        observer.observe.side_effect = (None, 2, 2, 1)
        result = ensure_weapon_set(capture, backend, lambda: True, threading.Event(), observer, 1)
        self.assertEqual(dict(before=2, after=1, swapped=True), result)
        self.assertEqual(["i", "w", "i"], [call[0][0] for call in backend.press.call_args_list])

    def test_current_primary_set_never_toggles_weapon(self):
        backend, observer = Mock(), Mock()
        observer.observe.return_value = 1
        ensure_weapon_set(Mock(), backend, lambda: True, threading.Event(), observer, 1)
        self.assertEqual(["i", "i"], [call[0][0] for call in backend.press.call_args_list])

    def test_unknown_weapon_set_does_not_send_a_speculative_swap(self):
        backend, observer = Mock(), Mock()
        observer.observe.return_value = None
        with self.assertRaisesRegex(RuntimeError, "unverified"):
            ensure_weapon_set(Mock(), backend, lambda: True, threading.Event(), observer, 1, timeout=0.03)
        backend.press.assert_called_once_with("i")


if __name__ == "__main__":
    unittest.main()
