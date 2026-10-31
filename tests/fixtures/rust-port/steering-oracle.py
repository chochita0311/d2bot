"""조향·우회 흐름의 개발용 상태 비교. 화면이나 입력 장치에 접근하지 않는다."""
import json
import math
import sys
from dataclasses import asdict
from pathlib import Path

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner.navigation import ArcanePathObserver, PathSettings, PathView, PathChoice
from diablo2.runs.summoner.steering import ArcaneRouteFlow, ContinuousSteering


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("steering oracle baseline changed")
    result = []
    for group in json.load(sys.stdin):
        teleport = dict(maximum_distance=600, clearance_pixels=32)
        if group.get("preferred") is not None:
            teleport["preferred_distance"] = group["preferred"]
        path = ArcanePathObserver(PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=teleport)
        flow = ArcaneRouteFlow(group.get("detour"))
        steering = ContinuousSteering(0.32, 32, aim=group.get("aim"))
        rows = []
        for op in group["operations"]:
            if op.get("lines"):
                mask = np.zeros((800, 1600), np.uint8)
                for x0, y0, x1, y1, thickness in op["lines"]:
                    cv.line(mask, (x0, y0), (x1, y1), 1, thickness)
                view = path.from_mask(mask, 1, 0, op["at"], (1600, 800), (0, 0, 1600, 800), (800, 400))
            else:
                floor = np.full((100, 200), not op.get("blank", False), bool)
                if op.get("closed"):
                    floor[15:35, 140:160] = False
                if op.get("corner"):
                    floor[:, :100] = False
                view = PathView(1, 0, op["at"], (1600, 800), (0, 0, 1600, 800), (800, 400), 8, floor)
            choice = PathChoice(None if op.get("aim") is None else tuple(op["aim"]), op.get("reason", "floor"))
            now = op.get("now", op["at"])
            if group["kind"] == "steering":
                outcome = steering.update(view, choice, now)
                state = {key: getattr(steering, key) for key in ("aim", "steered_at", "observed_at", "pending")}
            else:
                outcome = asdict(
                    flow.choose(
                        path,
                        view,
                        op.get("wing", "north"),
                        choice,
                        now,
                        position=op.get("position", (0, 0)),
                        visited=op.get("visited", ()),
                        excluded=op.get("excluded", ()),
                    )
                )
                state = {key: getattr(flow, key) for key in ("detour", "forward_frames", "observed_at", "reason")}
            for key in ("steered_at", "observed_at"):
                if key in state and not math.isfinite(state[key]):
                    state[key] = None
            rows.append(dict(floor=view.floor.astype(np.uint8).tobytes().hex(), outcome=outcome, state=state))
        result.append(rows)
    json.dump(result, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
