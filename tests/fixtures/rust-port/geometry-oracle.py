"""개발 전용: 기존 순수 기하 API의 결과를 입력 없이 비교한다."""

import json
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.common.frame_geometry import FrameGeometry, ReferenceLayout, desktop_to_frame, frame_to_desktop, geometry_is_current


def outcome(call):
    try:
        return {"value": call()}
    except Exception as error:
        return {"error_type": type(error).__name__, "error_message": str(error)}


def evaluate(request):
    if request.get("mode") == "layout":
        if "path" in request:
            layout = ReferenceLayout.load(request["path"])
        else:
            with patch.object(Path, "read_text", return_value=json.dumps(request["layout"])):
                layout = ReferenceLayout.load("oracle-memory.json")
        return dict(frame_size=layout.frame_size, client_box=layout.client_box, aspect_tolerance=layout.aspect_tolerance)
    layout = request.get("layout")
    if layout is not None:
        layout = ReferenceLayout(tuple(layout["frame_size"]), tuple(layout["client_box"]), layout.get("aspect_tolerance", 0.01))
    target = request["target"]
    geometry = FrameGeometry.create(target, layout, request.get("client"))
    results = []
    for query in request.get("queries", []):
        capture = SimpleNamespace(
            target=target,
            frame_geometry=None if request.get("legacy") else geometry,
            current_target=lambda: query.get("current_target", target),
        )
        if not query.get("missing_client_query"):
            capture.current_client_box = lambda: query.get("current_client", geometry.source_client)
        point = tuple(float(value) for value in query.get("point", (0, 0)))
        operations = {
            "to": lambda: geometry.to_desktop(point),
            "from": lambda: geometry.from_desktop(point),
            "frame_to": lambda: frame_to_desktop(capture, point),
            "desktop_to": lambda: desktop_to_frame(capture, point),
            "current": lambda: geometry_is_current(capture),
        }
        results.append(outcome(operations[query["op"]]))
    return {
        "geometry": geometry.as_dict(),
        "native": geometry.frame_size == geometry.source_target[2:] and geometry.source_client == geometry.layout.client_box,
        "queries": results,
    }


print(json.dumps(outcome(lambda: evaluate(json.load(sys.stdin))), ensure_ascii=False, allow_nan=False))
