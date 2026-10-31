"""현재 이동 계획·단발 입력의 개발용 비교. 모든 입력·시계·대기는 가짜 경계다."""
import json
import math
import sys
from dataclasses import asdict
from hashlib import sha256
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch as override

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner.navigation_execution import load_arcane_step_planner, plan_arcane_step, dispatch_planned_step
from diablo2.runs.summoner.scene_vision import GroundReading
from diablo2.common.scene_motion import CameraMotion
from diablo2.common.survival_vision import ResourceReading


def number(value):
    return {"nan": float("nan"), "inf": float("inf"), "-inf": float("-inf")}.get(value, value) if isinstance(value, str) else value


def clean(value):
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, dict):
        return {key: clean(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [clean(item) for item in value]
    return value


def dispatch_cases(cases):
    result = []
    for case in cases:
        trace, rows, scripts, counts = [], [], {}, {}
        pointer = [None if case.get("pointer") is None else tuple(case["pointer"])]
        preparation = dict(case.get("preparation") or {})
        if "aim" in preparation:
            preparation["aim"] = tuple(preparation["aim"])
        diagnostics = {}

        def scripted(name, fallback):
            values = scripts.get(name, [fallback])
            index = counts.get(name, 0)
            counts[name] = index + 1
            value = number(values[min(index, len(values) - 1)])
            trace.append([name, value])
            return value

        class Stop:
            def is_set(self):
                return scripted("stop", False)

            def wait(self, seconds):
                trace.append(["wait", seconds])

        class Backend:
            def move(self, x, y):
                trace.append(["move", x, y])
                if scripts.get("failure") == "move":
                    raise RuntimeError("move failed")
                pointer[0] = (x, y)

            def press(self, key):
                trace.append(["press", key])
                if scripts.get("failure") == "press":
                    raise RuntimeError("press failed")

            def pointer_matches(self, aim):
                matches = pointer[0] == aim
                trace.append(["pointer_matches", list(aim), matches])
                return matches

        for operation in case["operations"]:
            scripts, counts = operation, {}
            start = len(trace)
            outcome, error = None, None
            try:
                outcome = dispatch_planned_step(
                    Backend(),
                    lambda: scripted("focus", True),
                    Stop(),
                    tuple(operation.get("aim", (100, 100))),
                    "f2",
                    number(operation["at"]),
                    clock=lambda: scripted("clock", 100.2),
                    verify_aim=None if "verify" not in operation else lambda: scripted("verify", True),
                    expires_at=None if operation.get("expires") is None else number(operation["expires"]),
                    diagnostics=diagnostics,
                    pointer_preparation=preparation if case.get("prepared") else None,
                )
            except RuntimeError as caught:
                error = str(caught)
            rows.append(
                clean(
                    dict(
                        outcome=outcome,
                        error=error,
                        diagnostics=dict(diagnostics),
                        preparation=None if not case.get("prepared") or not preparation else dict(preparation),
                        trace=trace[start:],
                    )
                )
            )
        result.append(rows)
    return result


def plan_cases(cases):
    result = []
    for case in cases:
        loaded_ground, path = load_arcane_step_planner(automap=case.get("automap", False), character_class=case.get("class", "sorceress"))
        if case.get("walking"):
            path.teleport = None
        frame = np.zeros((1140, 1922, 3), np.uint8) if case.get("path") is None else cv.imread(str(ROOT / case["path"]))
        point = case.get("ground", [962, 580])
        reading = GroundReading(None if point is None else tuple(point), "ground_unverified" if point is None else "test_current_ground")
        calls = []

        class Ground:
            def observe(self, frame, *, captured_at=None):
                calls.append(captured_at)
                return reading

        outcome, error = None, None
        try:
            choice, found, view = plan_arcane_step(
                frame,
                case.get("wing", "north"),
                loaded_ground if case.get("loader_ground") else Ground(),
                path,
                case.get("at", 100),
                clock=lambda: case.get("now", 100.1),
                hub_point=case.get("hub"),
                prefer_frontier=case.get("frontier", False),
                allow_no_aim=case.get("allow_no_aim", False),
                ground_reading=reading if case.get("provided") else None,
            )
            outcome = dict(
                choice=asdict(choice),
                ground=asdict(found),
                viewport=view.viewport,
                anchor=view.anchor,
                size=view.floor.shape[::-1],
                floor=sha256(view.floor.astype(np.uint8).tobytes()).hexdigest(),
            )
        except RuntimeError as caught:
            error = str(caught)
        result.append(dict(outcome=outcome, error=error, calls=calls, original_settings=asdict(path.settings)))
    return result


def landing_cases(cases, policy):
    from diablo2.runs.summoner.navigation_execution import observe_step_landing

    result = []
    for case in cases:
        now, captures, calls = [100.0], [0], [0]
        hints = []

        class Capture:
            def grab(self):
                captures[0] += 1
                return SimpleNamespace(frame=None)

        class Hud:
            def observe(self, frame):
                now[0] += case.get("processing", 0)
                life = None if case.get("unknown") or captures[0] in case.get("unknown_at", ()) else case.get("life", 100)
                mana = case.get("mana", 100)
                return SimpleNamespace(
                    life=ResourceReading(life, None if life is None else 100),
                    mana=ResourceReading(mana, None if mana is None else 100),
                    belt_visibility=case.get("belt", "closed"),
                    mercenary_status=case.get("mercenary"),
                )

        class Ground:
            def observe(self, frame, *, captured_at):
                point = (
                    None
                    if case.get("no_ground") or (case.get("ground_at") is not None and captures[0] != case["ground_at"])
                    else (960, 578)
                )
                return GroundReading(point, "test")

        class Stop:
            def is_set(self):
                return captures[0] >= case.get("stop_at", 100)

            def wait(self, seconds):
                now[0] += case.get("wait", 0.05)

        class PathObserver:
            def observe(self, *args, **kwargs):
                return object()

        class Motion:
            def observe(self, frame, view, *, overlap_hint=None):
                calls[0] += 1
                hints.append(overlap_hint)
                delta = None if calls[0] == 1 else tuple(case.get("delta", (80, -40))) if calls[0] == 2 else (0, 0)
                if calls[0] in case.get("motion_missing_at", ()):
                    delta = None
                return CameraMotion(delta, 20, "background_translation")

        outcome, error = None, None
        try:
            with override("diablo2.runs.summoner.navigation.BackgroundMotionObserver", return_value=Motion()):
                outcome = observe_step_landing(
                    Capture(),
                    Hud(),
                    lambda: captures[0] < case.get("focus_lost_at", 100),
                    SimpleNamespace(**policy),
                    Stop(),
                    None,
                    SimpleNamespace(anchor=(960, 578), viewport=(600, 358, 1320, 798), generation=0),
                    SimpleNamespace(aim=(1060, 528)),
                    Ground(),
                    PathObserver(),
                    reference_seconds=number(case.get("reference", 0.32)),
                    clock=lambda: now[0],
                    expires_at=case.get("expires"),
                )
        except ValueError as caught:
            error = str(caught)
        result.append(dict(outcome=outcome, error=error, captures=captures[0], motion_hints=hints))
    return result


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("navigation execution oracle baseline changed")
    request = json.load(sys.stdin)
    if request["kind"] == "dispatch":
        result = dispatch_cases(request["cases"])
    elif request["kind"] == "landing":
        result = landing_cases(request["cases"], request["policy"])
    else:
        result = plan_cases(request["cases"])
    json.dump(clean(result), sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
