"""저장된 그리드·가짜 시계로 기존 독립 탐색 상태 전이를 비교한다."""
import json
import sys
from dataclasses import asdict
from copy import deepcopy
from pathlib import Path

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner.navigation import ArcanePathObserver, CameraMotion, PathSettings
from diablo2.runs.summoner.search import ArcaneSearch, SearchLimits, SearchObservation, run_search

LIMITS = dict(
    screen_age=0.5,
    landing_seconds=2,
    wing_seconds=20,
    return_seconds=20,
    total_seconds=100,
    minimum_movement=4,
    breadcrumb_radius=8,
    maximum_casts=100,
    maximum_hover_attempts=2,
    settled_frames=2,
    settled_motion_pixels=2,
)


def state(search):
    return deepcopy(current_state(search))


def current_state(search):
    view = search.last_view
    return dict(
        phase=search.phase,
        wing=search.wing,
        position=search.position,
        breadcrumbs=search.breadcrumbs,
        return_points=search.return_points,
        checked_wings=search.checked_wings,
        expected_generation=search.expected_generation,
        pending=None if search.pending is None else asdict(search.pending),
        cursor=search.cursor,
        landing=search.landing,
        hover_attempts=search.hover_attempts,
        cast_count=search.cast_count,
        serial=search.serial,
        landing_stable=search.landing_stable,
        review_reason=search.review_reason,
        interval=search.interval,
        stage_at=search.stage_at,
        last_now=search.last_now,
        last_view=None
        if view is None
        else dict(
            sequence=view.sequence,
            generation=view.generation,
            captured_at=view.captured_at,
            frame_size=view.frame_size,
            viewport=view.viewport,
        ),
    )


def exercise(case):
    if "runner" in case:
        return runner(case["runner"])
    limits = dict(LIMITS, **case.get("limits", {}))
    observer = ArcanePathObserver(PathSettings(4, 0.5, 20, 32, 40, 8, 8))
    try:
        search = ArcaneSearch(observer, SearchLimits(**limits), reference_seconds=case.get("reference", 0.32), started_at=0)
    except Exception as error:
        return dict(error=dict(kind=type(error).__name__, message=str(error)))
    now, sequence, last, rows = 0.0, 0, None, []
    for op in case["ops"]:
        now = op.get("now", now + op.get("dt", 0))
        try:
            kind = op.get("op", "step")
            if kind == "step":
                sequence = op.get("sequence", sequence + 1)
                width, height = op.get("size", [200, 200])
                mask = np.full((height, width), not op.get("empty", False), bool)
                view = observer.from_mask(
                    mask,
                    sequence,
                    op.get("generation", search.expected_generation or 0),
                    op.get("captured", now),
                    (width, height),
                    tuple(op.get("viewport", [0, 0, width, height])),
                    (100, 100),
                )
                hover = op.get("hover")
                observation = SearchObservation(
                    view,
                    CameraMotion(op.get("motion", (0, 0)), 20, "synthetic"),
                    op.get("allowed", True),
                    op.get("terminal"),
                    op.get("hub", False),
                    None if hover is None else (hover["kind"], tuple(hover["bounds"])),
                )
                last = search.step(observation, now, interrupted=op.get("interrupted", False), focused=op.get("focused", True))
                result = asdict(last)
            elif kind == "tick":
                last = search.tick(now, interrupted=op.get("interrupted", False), focused=op.get("focused", True))
                result = asdict(last)
            elif kind == "ack":
                request = search.pending if op.get("pending", False) else last
                if op.get("wrong", False):
                    from dataclasses import replace

                    request = replace(request, request_id=request.request_id + 1)
                result = search.acknowledge(request, now, op.get("success", True))
            elif kind == "defer":
                result = search.defer(last)
            else:
                last = search.fail(op["reason"])
                result = asdict(last)
            rows.append(dict(result=result, state=state(search)))
        except Exception as error:
            rows.append(dict(error=dict(kind=type(error).__name__, message=str(error)), state=state(search)))
    return rows


def runner(case):
    observer = ArcanePathObserver(PathSettings(4, 0.5, 20, 32, 40, 8, 8))
    search = ArcaneSearch(observer, SearchLimits(**LIMITS), reference_seconds=0.32, started_at=0)
    now, index, focus, events, actions = [0.0], [0], [True], [], []

    def observe(generation):
        if index[0] >= len(case["rows"]):
            return None
        row = case["rows"][index[0]]
        index[0] += 1
        now[0] = row.get("now", now[0] + row.get("dt", 0))
        if row.get("error"):
            raise RuntimeError(row["error"])
        view = observer.from_mask(np.ones((200, 200), bool), index[0], generation or 0, now[0], (200, 200), (0, 0, 200, 200), (100, 100))
        now[0] += row.get("age", 0)
        return SearchObservation(view, CameraMotion((0, 0), 20, "synthetic"), row.get("allowed", True), row.get("terminal"))

    class Backend:
        def move(self, x, y):
            events.append(["move", [x, y]])
            now[0] += case.get("move_delay", 0)
            if case.get("move_error"):
                raise RuntimeError("move failed")
            focus[0] = not case.get("lose_focus", False)
            return not case.get("move_rejected", False)

        def press(self, key):
            events.append(["press", key])
            if case.get("press_error"):
                raise RuntimeError("press failed")
            return not case.get("press_rejected", False)

        def key_up(self, key):
            events.append(["up", key])
            if case.get("release_error"):
                raise RuntimeError("release failed")

    def wait(seconds):
        events.append(["wait", seconds])
        now[0] += seconds
        if case.get("wait_error"):
            raise RuntimeError("wait failed")
        return case.get("wait_stop", False) or index[0] >= len(case["rows"])

    def emit(action):
        actions.append(asdict(action))
        if case.get("emit_error"):
            raise RuntimeError("emit failed")

    try:
        action = run_search(
            search,
            observe=observe,
            backend=Backend(),
            movement_key="f2",
            focused=lambda: focus[0],
            interrupted=lambda: False,
            clock=lambda: now[0],
            wait=wait,
            on_action=emit,
        )
        result = dict(result=asdict(action))
    except Exception as error:
        result = dict(error=str(error))
    result.update(state=state(search), events=events, actions=actions)
    return result


if __name__ == "__main__":
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("search baseline changed")
    print(json.dumps([exercise(case) for case in json.load(sys.stdin)], allow_nan=False))
