"""실제 판정/held-input/life 코드를 가짜 화면·시계·작업 생성자로 재생한다."""
import json
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner import encounter_execution as encounter
from diablo2.runs.summoner.objective_vision import ObjectiveReading
from diablo2.runs.summoner.scene_vision import GroundReading, HostileTarget
from diablo2.runs.summoner.navigation import PathChoice
from diablo2.runs.summoner.life_vision import SummonerLifeTracker
from diablo2.common.survival import CharacterSurvivalPolicy
from diablo2.common.survival_vision import HudReading, ResourceReading


class ImmediateFuture:
    def __init__(self, work, args, kwargs):
        try:
            self.value, self.error = work(*args, **kwargs), None
        except Exception as error:
            self.value, self.error = None, error

    def result(self):
        if self.error is not None:
            raise self.error
        return self.value


class ImmediatePool:
    def __init__(self, **kwargs):
        pass

    def submit(self, work, *args, **kwargs):
        return ImmediateFuture(work, args, kwargs)

    def shutdown(self, wait):
        pass


class NoThread:
    def __init__(self, **kwargs):
        pass

    def start(self):
        pass

    def join(self, timeout):
        pass


def exercise(case, policy):
    now, index, pointer, events, emits = [10.0], [-1], [None], [], []
    samples = case.get("frames", [{}])

    def sample():
        return samples[min(max(index[0], 0), len(samples) - 1)]

    def advance(amount):
        now[0] += amount

    stop = SimpleNamespace(is_set=lambda: now[0] >= case.get("stop_at", 1e9), set=lambda: case.update(stop_at=now[0]), wait=advance)

    def focused():
        events.append(["focus", now[0]])
        return now[0] < case.get("focus_lost_at", 1e9)

    def grab():
        index[0] += 1
        events.append(["capture", index[0], now[0]])
        advance(sample().get("capture_delay", 0.1))
        return SimpleNamespace(frame=SimpleNamespace(index=index[0], shape=(1140, 1922, 3)))

    def hud(frame):
        events.append(["hud", index[0]])
        advance(sample().get("hud_delay", 0))

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

    def ground(frame, captured_at):
        events.append(["ground", index[0], captured_at])
        advance(sample().get("ground_delay", 0))
        point = sample().get("player", [750, 580])
        return GroundReading(None if point is None else tuple(point), "test")

    def prepare(frame, acquired, reading, focus):
        events.append(["prepare", index[0], acquired, focus])

    def platform(frame):
        events.append(["platform", index[0]])
        advance(sample().get("platform_delay", 0))
        if sample().get("platform", True) is None:
            return None
        return ObjectiveReading(
            "summoner_platform",
            None,
            None,
            ((650, 400, 850, 500),),
            "runes",
            ground_point=tuple(sample().get("platform_ground", [800, 580])),
        )

    def appearance(frame, platform, state):
        events.append([state, index[0]])
        advance(sample().get(state + "_delay", 0))
        value = sample().get(state, state == "alive")
        if not value:
            return None
        return encounter.SummonerAppearance((790, 540, 810, 580), tuple(sample().get(state + "_ground", [800, 580])), 1.0, state, state)

    def path(frame, **kwargs):
        events.append(["path", index[0], kwargs["sequence"], kwargs["captured_at"], list(kwargs["anchor"])])
        advance(sample().get("path_delay", 0))
        if sample().get("path_error"):
            raise RuntimeError("path observation failed")
        return index[0]

    def choose(view, wing, destination):
        events.append(["choose", view, wing, list(destination)])
        aim = sample().get("aim", [1300, 580])
        return PathChoice(None if aim is None else tuple(aim), "test", ())

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

    class Probe:
        def __init__(self, *args, **kwargs):
            self.pending = None

        def reset(self):
            events.append(["probe_reset"])
            self.pending = None

        def observe(self, frame, player, acquired, points=None):
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
            dict(status="inspection_complete", bounded_reason="two_frames_without_eligible_candidate", movement_allowed=True, clicks=[]),
        )

    observer = SimpleNamespace(observe=ground)
    if case.get("camera"):
        observer.prepare_frame = prepare
    capture = SimpleNamespace(grab=grab, target=dict(left=-1922, top=0))
    evidence, result, error = [], None, None
    with patch.object(encounter, "ThreadPoolExecutor", ImmediatePool), patch.object(encounter.threading, "Thread", NoThread), patch.object(
        encounter, "NearbyHostileProbe", Probe
    ), patch.object(encounter, "nearby_actor_proposals", proposals):
        try:
            result = encounter.run_platform_encounter(
                capture,
                SimpleNamespace(observe=hud),
                Backend(),
                focused,
                SimpleNamespace(survival=policy, actions=SimpleNamespace(movement_skill_key="f2", primary_attack_skill_key="f4")),
                stop,
                (observer, SimpleNamespace(observe=path, choose=choose)),
                SimpleNamespace(observe=platform),
                SimpleNamespace(
                    observe=lambda frame, platform: appearance(frame, platform, "alive"),
                    observe_corpse=lambda frame, platform: appearance(frame, platform, "corpse"),
                ),
                buff_expires_at=case.get("buff", 100),
                reference_seconds=0.32,
                evidence=evidence,
                emit=emits.append,
                clock=lambda: now[0],
                target_observer=object() if case.get("probe") else None,
                recover_resources=recover,
                loot_controller=SimpleNamespace(movement_allowed=case.get("labels_off", True), after_combat=inspect)
                if case.get("loot")
                else None,
                life_tracker=SummonerLifeTracker("flash", "room") if case.get("life") else None,
            )
        except (ValueError, RuntimeError) as caught:
            error = str(caught)
    return dict(
        result=result,
        error=error,
        events=events,
        emits=emits,
        evidence=[dict(file=name, frame=frame.index) for name, frame in evidence],
        now=now[0],
    )


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("encounter execution baseline changed")
    request = json.load(sys.stdin)
    policy = CharacterSurvivalPolicy.from_dict(request["policy"])
    print(json.dumps([exercise(case, policy) for case in request["cases"]], ensure_ascii=False, allow_nan=False))


if __name__ == "__main__":
    main()
