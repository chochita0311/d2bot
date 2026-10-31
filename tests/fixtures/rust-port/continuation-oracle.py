"""현재 경로 인계와 정착 상태만 비교하며 화면·입력 장치에 접근하지 않는다."""
import json
import sys
from pathlib import Path

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner.continuous_navigation import (
    ArrivalDwell,
    current_hub_landing,
    near_ground,
    restore_route_flow,
    validate_hub_departure,
    validate_live_continuation,
    validate_terminal_return,
)


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("continuation oracle baseline changed")
    result = []
    for case in json.load(sys.stdin):
        kind = case["kind"]
        if kind == "saved":
            outcome, error = None, None
            try:
                detour, measured = restore_route_flow(
                    case["report"],
                    cv.imread(case["frame"]),
                    wing=case["wing"],
                    return_from=case.get("return_from"),
                    maximum_pending_distance=case.get("maximum", 600),
                )
                outcome = dict(detour=detour, motion=measured)
            except RuntimeError as caught:
                error = str(caught)
            result.append(dict(outcome=outcome, error=error))
        elif kind in ("live", "terminal", "hub"):
            error = None
            try:
                dict(live=validate_live_continuation, terminal=validate_terminal_return, hub=validate_hub_departure)[kind](
                    case["progress"], case["wing"], case.get("return_from"), case["now"]
                )
            except ValueError as caught:
                error = str(caught)
            result.append(error)
        elif kind == "dwell":
            dwell, rows = ArrivalDwell(case.get("reference", 0.32)), []
            for op in case["operations"]:
                if op.get("reset"):
                    dwell.reset()
                    outcome = None
                else:
                    outcome = dwell.update(op.get("near", True), op.get("displacement"), op["at"])
                rows.append(dict(outcome=outcome, state=vars(dwell).copy()))
            result.append(rows)
        elif kind == "landing":
            progress, rows = {}, []
            for op in case["operations"]:
                if op.get("begin"):
                    progress["hub_approach_origin"] = dict(relative=tuple(a - b for a, b in zip(op["hub"], op["player"])), at=op["at"])
                    outcome = None
                else:
                    outcome = current_hub_landing(
                        progress,
                        op["hub"],
                        op["player"],
                        None if op.get("evidence") is None else dict(kind=op["evidence"]),
                        op.get("displacement"),
                        op["at"],
                        op["sequence"],
                    )
                rows.append(dict(outcome=outcome, state=json.loads(json.dumps(progress))))
            result.append(rows)
        else:
            error, outcome = False, None
            try:
                outcome = near_ground(case.get("point"), case.get("player", (0, 0)), case.get("radius", (300, 180)))
            except ZeroDivisionError:
                error = True
            result.append(dict(outcome=outcome, error=error))
    json.dump(result, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
