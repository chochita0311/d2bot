"""근접 색상 후보와 현재 호버 패널의 비교. 커서·입력·시계는 모두 가짜다."""
import json
import math
import sys
from pathlib import Path

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.common.hostile_probe import NearbyHostileProbe, nearby_actor_proposals


def image(case):
    if case.get("path"):
        return cv.imread(str(ROOT / case["path"]))
    width, height = case.get("size", (1922, 1140))
    frame = np.zeros((height, width, 3), np.uint8)
    for left, top, right, bottom, color in case.get("rectangles", ()):
        frame[top:bottom, left:right] = color
    return frame


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("hostile probe oracle baseline changed")
    result = []
    for case in json.load(sys.stdin):
        frame = image(case)
        if case["kind"] == "proposals":
            result.append(nearby_actor_proposals(frame, case.get("player")))
            continue
        now, pointer, events = [10.0], [None], []
        operation, clock_index = {}, [0]

        def clock():
            values = operation.get("clock")
            if values:
                now[0] = values[min(clock_index[0], len(values) - 1)]
                clock_index[0] += 1
            events.append(["clock", now[0]])
            return now[0]

        class Backend:
            def move(self, x, y):
                events.append(["move", x, y])
                if operation.get("move_error"):
                    raise RuntimeError("move failed")
                pointer[0] = (x, y)
                now[0] += operation.get("move_delay", 0)

            def pointer_matches(self, point):
                matches = operation.get("pointer_matches", pointer[0] == tuple(point))
                events.append(["pointer_matches", list(point), matches])
                return matches

        class Observer:
            def observe(self, frame):
                events.append(["observe"])
                now[0] += operation.get("observer_delay", 0)
                if operation.get("observer_error"):
                    raise RuntimeError("observer failed")
                return "current hostile" if operation.get("target", True) else None

        origin = case.get("origin", (0, 0))
        scale = case.get("scale")
        mapper = None if scale is None else lambda point: tuple(int(round(origin[i] + point[i] * scale[i])) for i in range(2))
        probe = NearbyHostileProbe(Observer(), Backend(), origin, clock=clock, to_desktop=mapper)
        rows = []
        for operation in case["operations"]:
            now[0], clock_index[0] = operation.get("now", 10.0), 0
            start = len(events)
            outcome, error = None, None
            try:
                if operation.get("reset"):
                    probe.reset()
                else:
                    outcome = probe.observe(
                        frame, operation.get("player", (962, 580)), operation.get("at", now[0]), points=operation.get("points")
                    )
            except RuntimeError as caught:
                error = str(caught)
            state = {key: getattr(probe, key) for key in ("pending", "pending_ground", "index", "next_proposal_at")}
            if not math.isfinite(state["next_proposal_at"]):
                state["next_proposal_at"] = None
            rows.append(dict(outcome=outcome, error=error, state=state, events=events[start:]))
        result.append(rows)
    json.dump(result, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
