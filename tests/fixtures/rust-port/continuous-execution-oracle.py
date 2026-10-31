"""연속 실행기의 의존 경계를 고정해 실제 Python 흐름/held 입력을 비교한다."""
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner import continuous_navigation as continuous
from diablo2.runs.summoner.navigation import PathChoice
from diablo2.runs.summoner.scene_vision import GroundReading, HostileTarget, HoverCandidate
from diablo2.runs.summoner.objective_vision import ObjectiveReading
from diablo2.runs.summoner.terminal_vision import TerminalReading
from diablo2.common.survival import CharacterSurvivalPolicy, ObservationStamp
from diablo2.common.survival_vision import HudReading, ResourceReading
from diablo2.common.encounters import SceneContext, VisualCandidate
from diablo2.common.scene_motion import CameraMotion
from importlib.util import spec_from_file_location, module_from_spec

spec = spec_from_file_location("encounter_fixture", Path(__file__).with_name("encounter-execution-oracle.py"))
fixture = module_from_spec(spec)
spec.loader.exec_module(fixture)


class Pool(fixture.ImmediatePool):
    def __enter__(self):
        return self

    def __exit__(self, *args):
        pass


class Frame(np.ndarray):
    def __new__(cls, index, shade):
        result = np.broadcast_to(np.array([shade] * 3, np.uint8), (1140, 1922, 3)).view(cls)
        result.index = index
        return result

    def __array_finalize__(self, source):
        self.index = getattr(source, "index", 0)


@dataclass(frozen=True)
class View:
    index: int
    sequence: int
    captured_at: float
    anchor: tuple
    cell_pixels: int = 12


def exercise(case, policy):
    now, index, pointer, events, emits = [10.0], [-1], [None], [], []
    samples = case.get("frames", [{}])

    def sample():
        return samples[min(max(index[0], 0), len(samples) - 1)]

    def advance(amount):
        now[0] += amount

    def delay(key):
        advance(sample().get(key, 0))

    def focused():
        events.append(["focus", now[0]])
        return now[0] < case.get("focus_lost_at", 1e9)

    stop = SimpleNamespace(is_set=lambda: now[0] >= case.get("stop_at", 1e9), set=lambda: case.update(stop_at=now[0]), wait=advance)

    def grab():
        index[0] += 1
        events.append(["capture", index[0], now[0]])
        advance(sample().get("capture_delay", 0.1))
        return SimpleNamespace(frame=Frame(index[0], sample().get("shade", 0)))

    def hud(frame):
        events.append(["hud", index[0]])
        delay("hud_delay")

        def resource(name):
            value = sample().get(name, 90)
            return ResourceReading() if value is None else ResourceReading(value, 100, "read")

        return HudReading(
            "test",
            "1",
            resource("life"),
            resource("mana"),
            "read",
            belt_visibility=sample().get("belt", "closed"),
            mercenary_status=sample().get("merc"),
        )

    class Backend:
        def key_down(self, key):
            events.append(["down", key, now[0]])

        def key_up(self, key):
            events.append(["up", key, now[0]])

        def press(self, key):
            events.append(["press", key, now[0]])
            advance(case.get("press_delay", 0))

        def move(self, x, y):
            events.append(["move", [x, y], now[0]])
            pointer[0] = (x, y)
            advance(case.get("move_delay", 0))

        def pointer_matches(self, point):
            return not case.get("pointer_drift", False) and pointer[0] == tuple(point)

    def ground(frame, captured_at):
        events.append(["ground", index[0], captured_at])
        delay("ground_delay")
        value = sample().get("player", [750, 580])
        return GroundReading(None if value is None else tuple(value), sample().get("ground_reason", "test"))

    def prepare(frame, acquired, reading, focus):
        events.append(["prepare", index[0], acquired, focus])

    def hazards(frame, player, acquired, processed, aim=None):
        events.append(["hazards", index[0], list(player), acquired, processed, aim])
        delay("hazard_delay")
        if not sample().get("near"):
            return ()
        context = SceneContext(ObservationStamp("flash", "room", index[0], acquired), "arcane", index[0], (1922, 1140), (24, 55, 1900, 900))
        return (VisualCandidate(context, "hostile", "hostile", "monster", "alive", 1.0, (700, 500, 810, 610), (800, 580)),)

    def hovering(frame, aim):
        events.append(["hover", index[0], list(aim)])
        delay("hover_delay")
        return (HoverCandidate("chest", (1000, 400, 1050, 420), 1.0),) if sample().get("hover") else ()

    def target(frame):
        events.append(["target", index[0]])
        delay("target_delay")
        return HostileTarget("hostile", (700, 55, 800, 90), None) if sample().get("target") else None

    def choice(name="aim", reason="choice_reason"):
        aim = sample().get(name, [1300, 300])
        return PathChoice(None if aim is None else tuple(aim), sample().get(reason, "test"), ())

    def choose(view, wing, **kwargs):
        normalized = dict(
            position=[0.0, 0.0], visited=[], destination=None, excluded=[], allow_detour=False, heading=None, prefer_frontier=False
        )
        normalized.update(kwargs)
        events.append(["choose", view.index, wing, normalized])
        delay("choose_delay")
        return choice("choose_aim", "choose_reason")

    path = SimpleNamespace(teleport=dict(maximum_distance=600, clearance_pixels=24), choose=choose)

    def plan(frame, wing, ground_observer, path, acquired, **kwargs):
        events.append(["plan", index[0], acquired, list(kwargs["position"]), kwargs["excluded"]])
        delay("plan_delay")
        reading = kwargs["ground_reading"]
        if sample().get("plan_error") or reading.point is None:
            raise RuntimeError(sample().get("plan_error", reading.reason))
        if not 0 <= now[0] - acquired < 0.25:
            raise RuntimeError("planned step frame is stale")
        return choice(), reading, View(index[0], 1, acquired, reading.point)

    def terminal(frame):
        events.append(["terminal", index[0]])
        delay("terminal_delay")
        return read_terminal(sample().get("terminal"))

    def read_terminal(value):
        if not value:
            return None
        value = {} if value is True else value
        return TerminalReading(
            value.get("kind", "nonboss_chest_trio"),
            tuple(value.get("point", [800, 580])),
            (),
            (),
            provenance=value.get("provenance", "current_three_parts"),
            current_parts=(True, True, True),
        )

    def objective(frame):
        events.append(["objective", index[0]])
        delay("objective_delay")
        return (
            ObjectiveReading("summoner_platform", None, None, ((650, 400, 850, 500),), "runes", ground_point=(800, 580))
            if sample().get("objective")
            else None
        )

    class Hub:
        last_evidence = None

        def observe(self, frame):
            events.append(["hub", index[0]])
            delay("hub_delay")
            value = sample().get("hub", [750, 580])
            self.last_evidence = (
                None if value is None else dict(kind="blue_pair_and_floor_ornaments", point=value, parts=[], blue_evidence="test")
            )
            return None if value is None else tuple(value)

        def current_pair_near(self, frame, player):
            events.append(["pair", index[0], list(player)])
            value = sample().get("pair")
            return None if value is None else tuple(value)

    class Motion:
        def __init__(self, **kwargs):
            self.features = None

        def reset(self):
            events.append(["motion_reset"])

        def observe(self, frame, view, overlap_hint=None):
            events.append(["motion", index[0], view.sequence, overlap_hint])
            delay("motion_delay")
            value = sample().get("motion", [0, 0])
            return CameraMotion(
                None if value is None else tuple(value), 0, "background_translation" if value is not None else "correspondence_unavailable"
            )

    class TerminalTracker:
        def __init__(self, observer):
            self.reference = self.observed_at = self.approach = self.approach_displacement = None
            self.approach_static_pairs = 0

        def reset(self):
            events.append(["terminal_reset"])
            self.reference = self.observed_at = self.approach = self.approach_displacement = None
            self.approach_static_pairs = 0

        def require_relocalization(self):
            events.append(["terminal_relocalize"])
            self.approach = self.approach_displacement = None
            self.approach_static_pairs = 0

        def update(self, frame, reading, acquired, displacement):
            events.append(["terminal_track", index[0], acquired, displacement])
            if reading is not None:
                self.reference, self.observed_at = reading, acquired
            self.approach = read_terminal(sample().get("approach"))
            self.approach_displacement = sample().get("approach_motion")
            self.approach_static_pairs = sample().get("static_pairs", 0)
            return reading

    class HubTracker:
        def __init__(self, observer):
            pass

        def reset(self):
            events.append(["hub_reset"])

        def update(self, frame, point, evidence, acquired, displacement):
            events.append(["hub_track", index[0], acquired, displacement])
            return point, evidence

    class Steering:
        def __init__(self, reference, clearance, aim=None):
            self.aim = aim

        def update(self, view, selected, at):
            events.append(["steer", index[0], at])
            delay("steer_delay")
            changed = self.aim != selected.aim
            self.aim = selected.aim
            return sample().get("steer", changed), sample().get("steering_reason", "cast_reference" if changed else "hold_current_aim")

    class Flow:
        def __init__(self, detour):
            self.detour = detour
            self.reason = "global_flow"

        axis = staticmethod(continuous.ArcaneRouteFlow.axis)

        def choose(self, path, view, wing, selected, at, **kwargs):
            events.append(["flow", index[0], at])
            delay("flow_delay")
            self.reason = sample().get("flow_reason", "global_flow")
            if "flow_detour" in sample():
                self.detour = sample()["flow_detour"]
            return choice("route_aim", "route_reason") if "route_aim" in sample() else selected

    class Graph:
        def __init__(self):
            self.position = (0.0, 0.0)
            self.localized = True
            self.places = [(0.0, 0.0)]

        def observe(self, features, player, displacement, acquired):
            events.append(["graph_observe", index[0], list(player), displacement, acquired])
            delay("graph_observe_delay")
            self.position = tuple(sample().get("graph_position", [0.0, 0.0]))
            self.localized = sample().get("localized", True)
            self.places = sample().get("graph_places", [[0.0, 0.0]])
            return sample().get("relocated", False)

        def choose(self, path, view, wing, at, **kwargs):
            events.append(["graph_choose", index[0], wing, at, kwargs])
            delay("graph_delay")
            return choice("graph_aim", "graph_reason")

        def fail_landing(self, aim, player):
            events.append(["graph_fail", index[0], list(aim), list(player)])

        def summary(self):
            return dict(position=self.position, localized=self.localized, places=self.places)

    def floor(view, aim, clearance):
        events.append(["floor", index[0], list(aim)])
        return sample().get("floor", True)

    class Probe:
        def __init__(self, *args, **kwargs):
            self.pending = None
            self.next_proposal_at = 0

        def reset(self):
            events.append(["probe_reset"])
            self.pending = None

        def observe(self, frame, player, acquired, points=None):
            if player is None:
                self.reset()
                return None
            events.append(["probe", index[0], list(player), acquired, points])
            self.pending = ((750, 500), now[0])
            return HostileTarget("hostile", (700, 55, 800, 90), None) if sample().get("hostile") else None

    def proposals(frame, player):
        events.append(["proposals", index[0], list(player)])
        return sample().get("proposals", [])

    def recover(frame, reading, acquired):
        events.append(["recovery", index[0], acquired])
        return sample().get("recover", False)

    def inspect(**kwargs):
        events.append(["loot", kwargs])
        return case.get(
            "inspection",
            dict(
                status="inspection_complete",
                bounded_reason="two_frames_without_eligible_candidate",
                movement_allowed=True,
                clicks=[],
                teleports=[],
                labels_state="off",
            ),
        )

    def visible(frame, region):
        events.append(["map_visible", index[0]])
        return sample().get("map_visible", True)

    def panels(frame):
        events.append(["panels", index[0]])
        return sample().get("panels", False)

    def restore(*args, **kwargs):
        events.append(["restore_map", now[0]])
        advance(case.get("restore_delay", 0))
        if case.get("restore_error"):
            raise RuntimeError("map restore failed")
        return dict(status="on_observed")

    def defense():
        from diablo2.runs.summoner import main_run
        from diablo2.common import hostile_probe

        capture = SimpleNamespace(grab=grab, target=dict(left=-1922, top=0, width=1922, height=1140))
        profile = SimpleNamespace(survival=policy, actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4"))
        if case["operation"] == "clear":
            return main_run.clear_buff_site(
                capture, SimpleNamespace(observe=hud), Backend(), focused, profile, stop, 0.32, evidence=evidence, clock=lambda: now[0]
            )

        def renew():
            events.append(["renew", now[0]])
            advance(case.get("renew_delay", 0.1))
            if case.get("renew_error"):
                raise RuntimeError("buff renewal failed")
            return case.get("renewed", now[0] + 20)

        available = case.get("vision", True) and case.get("backend", True)
        with patch.object(hostile_probe, "NearbyHostileProbe", Probe):
            main_run.maintain_paused_field(
                capture,
                SimpleNamespace(observe=hud),
                focused,
                profile,
                stop,
                report=progress,
                evidence=evidence,
                clock=lambda: now[0],
                backend=Backend() if case.get("backend", True) else None,
                ground=observer if available else None,
                hazards=SimpleNamespace(observe=hazards) if available else None,
                buff_expires_at=case.get("buff"),
                engage_monsters=case.get("engage", True),
                target_vision=SimpleNamespace(observe=target) if case.get("probe") else None,
                recover_resources=recover,
                renew_buffs=renew if case.get("renew") else None,
                loot_controller=SimpleNamespace(after_combat=inspect) if case.get("loot") else None,
            )
        return None

    observer = SimpleNamespace(observe=ground, center=None, reset_events=[])
    if case.get("camera"):
        observer.prepare_frame = prepare
    evidence, result, error = [], None, None
    progress = case.get("progress", dict(steps=[], position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)]))
    if case.get("operation") and case.get("operation") != "legacy":
        progress = case.get("report", {})
    proof = case.get(
        "proof",
        dict(
            source="runtime_screen",
            area="arcane_sanctuary",
            panels_closed=True,
            belt_closed=True,
            nearby_candidates=0,
            safety_scope="current_nearby_candidates_and_stable_ratios",
            observed_at=1009.9,
            current_wing="north",
            wing_phase="outbound",
        ),
    )
    replacements = dict(
        ThreadPoolExecutor=Pool,
        BackgroundMotionObserver=Motion,
        ContinuousSteering=Steering,
        ArcaneRouteFlow=Flow,
        TerminalStabilityTracker=TerminalTracker,
        HubStabilityTracker=HubTracker,
        NearbyHostileProbe=Probe,
        nearby_actor_proposals=proposals,
        plan_arcane_step=plan,
        floor_supports_aim=floor,
        ensure_automap_on=restore,
    )
    with patch.multiple(continuous, **replacements), patch.object(continuous.threading, "Thread", fixture.NoThread), patch.object(
        continuous.time, "time", lambda: 1010.0
    ):
        try:
            if case.get("operation") == "legacy":
                from diablo2.runs.summoner import main_run
                from diablo2.runs.summoner.navigation_execution import dispatch_planned_step as native_dispatch

                def legacy_dispatch(*args, **kwargs):
                    return native_dispatch(*args, clock=lambda: now[0], **kwargs)

                def legacy_plan(frame, wing, ground_observer, path, acquired, **kwargs):
                    events.append(["plan", index[0], acquired, kwargs["hub_point"], kwargs["position"], kwargs["visited"]])
                    reading = ground(frame, acquired)
                    delay("plan_delay")
                    if sample().get("plan_error") or reading.point is None:
                        raise RuntimeError(sample().get("plan_error", reading.reason))
                    if not 0 <= now[0] - acquired < 0.25:
                        raise RuntimeError("planned step frame is stale")
                    return choice(), reading, View(index[0], 1, acquired, reading.point)

                def legacy_landing(*args, **kwargs):
                    events.append(["landing", index[0], kwargs["reference_seconds"], kwargs["expires_at"]])
                    delay("landing_delay")
                    return dict(
                        status=sample().get("landing_status", "arrival_observed"),
                        displacement=tuple(sample().get("displacement", [12.5, -8.0])),
                        observations=[],
                    )

                class LegacyHub:
                    def normalize(self, frame):
                        return frame

                    def find(self, frame, name):
                        events.append(["hub", index[0]])
                        p = sample().get("hub")
                        return None if p is None else dict(x=p[0] * 1267 / 1922, y=p[1] * 753 / 1140)

                with patch.object(main_run.time, "monotonic", lambda: now[0]), patch.object(
                    main_run, "plan_arcane_step", legacy_plan
                ), patch.object(main_run, "observe_step_landing", legacy_landing), patch.object(
                    main_run, "dispatch_planned_step", legacy_dispatch
                ):
                    result = main_run.run_north_segment(
                        SimpleNamespace(grab=grab, target=dict(left=-1922, top=0, width=1922, height=1140)),
                        SimpleNamespace(observe=hud),
                        Backend(),
                        focused,
                        SimpleNamespace(survival=policy, actions=SimpleNamespace(movement_skill_key="f2")),
                        stop,
                        (observer, path),
                        proof,
                        case.get("casts", 1),
                        case.get("reference", 0.32),
                        evidence=evidence,
                        emit=emits.append,
                        hub_vision=LegacyHub() if case.get("has_hub") else None,
                        hazards=None if case.get("hazards") is False else SimpleNamespace(observe=hazards),
                        progress=progress,
                        hover_vision=None if case.get("hover") is False else SimpleNamespace(observe=hovering),
                        buff_expires_at=case.get("buff"),
                    )
            elif case.get("operation"):
                result = defense()
            else:
                result = continuous.run_continuous_route(
                    SimpleNamespace(grab=grab, target=dict(left=-1922, top=0, width=1922, height=1140)),
                    SimpleNamespace(observe=hud),
                    Backend(),
                    focused,
                    SimpleNamespace(survival=policy, actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4")),
                    stop,
                    (observer, path),
                    proof,
                    case.get("seconds", 2),
                    evidence=evidence,
                    hazards=SimpleNamespace(observe=hazards),
                    hover_vision=SimpleNamespace(observe=hovering),
                    reference_seconds=0.32,
                    hub_vision=Hub() if case.get("has_hub") else None,
                    wing=case.get("wing", "north"),
                    return_from=case.get("return_from"),
                    target_vision=SimpleNamespace(observe=target) if case.get("probe") else None,
                    engage_monsters=case.get("engage", True),
                    progress=progress,
                    buff_expires_at=case.get("buff"),
                    emit=emits.append,
                    clock=lambda: now[0],
                    continue_current_field=case.get("continue", False),
                    transition_from=case.get("transition"),
                    route_memory=Graph() if case.get("graph") else None,
                    terminal_vision=SimpleNamespace(observe=terminal) if case.get("has_terminal") else None,
                    objective_vision=SimpleNamespace(observe=objective) if case.get("has_objective") else None,
                    recover_resources=recover,
                    loot_controller=SimpleNamespace(movement_allowed=case.get("labels_off", True), after_combat=inspect)
                    if case.get("loot")
                    else None,
                    automap_vision=SimpleNamespace(visible=visible) if case.get("has_map") else None,
                    panels_open=panels if case.get("has_map") else None,
                )
        except (ValueError, RuntimeError) as caught:
            error = str(caught)
    return dict(
        result=result,
        error=error,
        progress=progress,
        events=events,
        emits=emits,
        evidence=[dict(file=name, frame=frame.index) for name, frame in evidence],
        now=now[0],
    )


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("continuous execution baseline changed")
    request = json.load(sys.stdin)
    policy = CharacterSurvivalPolicy.from_dict(request["policy"])

    def scalar(value):
        if isinstance(value, np.generic):
            return value.item()
        raise TypeError(type(value).__name__)

    print(json.dumps([exercise(case, policy) for case in request["cases"]], ensure_ascii=False, allow_nan=False, default=scalar))


if __name__ == "__main__":
    main()
