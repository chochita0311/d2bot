"""실제 협곡 마감 흐름을 가짜 현재 화면·포인터·시계로 비교한다."""
import json
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner import canyon_finish as canyon
from diablo2.common.survival import CharacterSurvivalPolicy
from diablo2.common.survival_vision import HudReading, ResourceReading


def exercise(case, policy):
    now, index, pointer, events, emits = [10.0], [-1], [None], [], []
    frames = case.get("frames", [{}])

    def sample():
        return frames[min(max(index[0], 0), len(frames) - 1)]

    def advance(seconds):
        now[0] += seconds

    def delay(key):
        advance(sample().get(key, 0))

    def focused():
        events.append(["focus", now[0]])
        return now[0] < case.get("focus_lost_at", 1e9)

    stop = SimpleNamespace(is_set=lambda: now[0] >= case.get("stop_at", 1e9), wait=advance)

    def grab():
        index[0] += 1
        events.append(["capture", index[0], now[0]])
        advance(sample().get("capture_delay", 0.01))
        frame = index[0]
        if case.get("operation") == "exit":
            width, height = case.get("frame_size", [1922, 1140])
            frame = SimpleNamespace(index=index[0], shape=(height, width, 3))
        return SimpleNamespace(frame=frame)

    def hud(frame):
        events.append(["hud", index[0]])
        delay("hud_delay")

        def resource(key):
            value = sample().get(key, 90)
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
        def press(self, key):
            events.append(["press", key, now[0]])
            advance(case.get("press_delay", 0))

        def move(self, x, y):
            events.append(["move", [x, y], now[0]])
            pointer[0] = (x, y)
            advance(case.get("move_delay", 0))

        def pointer_matches(self, point):
            return not case.get("pointer_drift", False) and pointer[0] == tuple(point)

        def click(self):
            events.append(["click", now[0]])
            advance(case.get("click_delay", 0))

    class Town:
        def normalize(self, frame):
            events.append(["town_frame", index[0]])
            delay("town_delay")
            return frame

        def find(self, frame, name):
            events.append(["find", name, index[0]])
            if case.get("operation") == "exit":
                advance(sample().get("menu_delay", 0.08))
                point = sample().get(name, [600, 300] if name == "save_exit" else [600, 650])
                return None if point is None else dict(x=point[0], y=point[1])
            area = sample().get("area", "arcane")
            yes = {
                "area_canyon": area == "canyon",
                "area_arcane": area == "arcane",
                "hud": sample().get("hud", True),
                "world_canyon_waypoint": sample().get("waypoint", False),
                "save_exit": sample().get("save_exit", False),
                "waypoint_header": sample().get("panel", False),
                "npc_cancel": sample().get("npc", False),
            }[name]
            return (1, 1) if yes else None

    def observe(name):
        def read(frame):
            events.append([name, index[0]])
            delay(name + "_delay")
            return sample().get(name)

        return read

    def ground(frame, captured_at):
        events.append(["ground", index[0], captured_at])
        delay("ground_delay")
        return SimpleNamespace(point=sample().get("player", [961, 578]))

    def prepare(frame, acquired, reading, focus):
        events.append(["prepare", index[0], acquired, focus])
        delay("prepare_delay")

    def path_observe(frame, **kwargs):
        events.append(["path", index[0], kwargs["sequence"], kwargs["captured_at"], kwargs["anchor"]])
        delay("path_delay")
        return None

    def landing(path, view, journal, player):
        events.append(["landing", index[0], journal, player])
        delay("landing_delay")
        return sample().get("landing", [1098, 450])

    observer = SimpleNamespace(observe=ground)
    if case.get("camera"):
        observer.prepare_frame = prepare
    result, error, evidence, diagnostics = None, None, [], {}
    planner_data = dict(case.get("planner", dict(phase="destination_arrival")))

    class Planner:
        data = planner_data

        def step(self, frame):
            events.append(["plan", index[0]])
            delay("plan_delay")
            if sample().get("plan_error"):
                raise RuntimeError(sample()["plan_error"])
            planner_data["phase"] = sample().get("phase", "done")
            return sample().get("action")

    def shift(frame, planner, width):
        events.append(["cursor_shift", index[0]])
        return sample().get("shift")

    def dispatch(capture, frame, action, size):
        events.append(["dispatch", index[0], dict(action), now[0]])
        delay("dispatch_delay")
        if sample().get("dispatch_error"):
            raise RuntimeError(sample()["dispatch_error"])

    def release():
        events.append(["release"])
        if case.get("release_error"):
            raise RuntimeError("mouse release failed")

    with patch.object(canyon, "CanyonApproachObserver", lambda: SimpleNamespace(observe=observe("stairs"))), patch.object(
        canyon, "HorazonJournalObserver", lambda: SimpleNamespace(observe=observe("journal"))
    ), patch.object(canyon, "red_portal", observe("portal")), patch.object(canyon, "journal_landing", landing), patch.object(
        canyon, "frame_to_desktop", lambda capture, p: (round(p[0] - 1922), round(p[1]))
    ):
        try:
            if case.get("operation") == "town":
                from diablo2.runs.summoner import main_run
                from diablo2.actions import town_loop_vision

                session = SimpleNamespace(_accept_waypoint_cursor_shift=shift, _dispatch_town_action=dispatch, _release_town_mouse=release)
                with patch.object(town_loop_vision, "TownLoopPlanner", lambda *args, **kwargs: Planner()), patch.object(
                    main_run.time, "monotonic", lambda: now[0]
                ):
                    result = main_run.return_to_act1_and_exit(
                        session,
                        SimpleNamespace(grab=grab, target=dict(width=1922)),
                        focused,
                        stop,
                        Town(),
                        evidence=evidence,
                        emit=emits.append,
                    )
            elif case.get("operation") == "exit":
                from diablo2.runs.summoner import main_run

                with patch.object(main_run, "frame_to_desktop", lambda capture, p: (round(p[0] - 1922), round(p[1]))):
                    result = main_run.exit_to_character(
                        SimpleNamespace(grab=grab),
                        focused,
                        stop,
                        Town(),
                        Backend(),
                        evidence=evidence,
                        diagnostics=diagnostics,
                        clock=lambda: now[0],
                    )
            else:
                result = canyon.enter_canyon(
                    Backend(),
                    SimpleNamespace(grab=grab),
                    focused,
                    stop,
                    SimpleNamespace(observe=hud),
                    SimpleNamespace(survival=policy, actions=SimpleNamespace(movement_skill_key="f2")),
                    (observer, SimpleNamespace(observe=path_observe)),
                    Town(),
                    kill_confirmed=case.get("kill", True),
                    loot_complete=case.get("loot", True),
                    evidence=evidence,
                    buff_expires_at=case.get("buff", 100),
                    emit=emits.append,
                    clock=lambda: now[0],
                    reference_seconds=case.get("reference", 0.32),
                )
        except (ValueError, RuntimeError) as caught:
            error = dict(kind=type(caught).__name__, message=str(caught))
    report = dict(
        result=result,
        error=error,
        events=events,
        emits=emits,
        evidence=[dict(file=n, frame=getattr(f, "index", f)) for n, f in evidence],
        now=now[0],
    )
    if case.get("operation") == "exit":
        report["diagnostics"] = diagnostics
    if case.get("operation") == "town":
        report["planner"] = planner_data
    return report


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("canyon execution baseline changed")
    request = json.load(sys.stdin)
    policy = CharacterSurvivalPolicy.from_dict(request["policy"])
    print(json.dumps([exercise(case, policy) for case in request["cases"]], ensure_ascii=False, allow_nan=False))


if __name__ == "__main__":
    main()
