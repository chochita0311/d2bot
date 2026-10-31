"""실제 독립 탐색 어댑터를 기존 테스트의 가짜 캡처·공통 제어기로 실행한다."""
import importlib.util
import json
import sys
from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import PropertyMock, patch

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
spec = importlib.util.spec_from_file_location("arcane_source_tests", ROOT / "tests/test_arcane_search_runtime.py")
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)


def snapshot(test):
    view = test.search.last_view
    return deepcopy(
        dict(
            search=dict(
                phase=test.search.phase,
                wing=test.search.wing,
                generation=test.search.expected_generation,
                view=None
                if view is None
                else dict(sequence=view.sequence, generation=view.generation, captured_at=view.captured_at, anchor=view.anchor),
            ),
            field=dict(
                action=test.loop.last.action,
                reason=test.loop.last.reason,
                stopped=test.loop.stopped,
                paused=test.loop.paused,
                held=sorted(test.loop.inputs.held),
            ),
            closed=test.capture.closed,
            events=list(test.backend.events),
        )
    )


def exercise(case):
    test = source.SearchRuntimeTests()
    test.setUp()
    rows = []
    if case.get("native", False):
        test.loop.inputs.backend = SimpleNamespace(
            move=test.backend.move, press=test.backend.press, key_down=test.backend.key_down, key_up=test.backend.key_up
        )
    try:
        for row in case["ops"]:
            test.at = row.get("now", test.at + row.get("dt", 0))
            for key in ("enemy", "terminal", "focus"):
                if key in row:
                    setattr(test, key, row[key])
            if "serial" in row:
                test.serial = row["serial"]
            if "motion" in row:
                test.delta = row["motion"]
            if "belt" in row:
                test.reading = replace(test.reading, belt_visibility=row["belt"])
            for key in ("life", "mana"):
                if key in row:
                    test.reading = replace(test.reading, **{key: source.ResourceReading(row[key], 100)})
            if "pause" in row:
                test.field.pause(row["pause"])

            def scene(frame, stamp):
                if row.get("wrong_stamp"):
                    stamp = replace(stamp, room_id="other")
                scene = test.scene(frame, stamp)
                if row.get("held_aim"):
                    return replace(scene, field=replace(scene.field, travel_aim=(500, 300)))
                hover = row.get("hover")
                return replace(
                    scene,
                    player_ground=None if row.get("ground") is None else tuple(row["ground"]),
                    hover=None if hover is None else (hover["kind"], tuple(hover["bounds"])),
                )

            def hud():
                test.at += row.get("age", 0)
                if row.get("move_hud"):
                    test.capture.current["top"] += 1
                if "hud_focus" in row:
                    test.focus = row["hud_focus"]

            test.driver.scene_reader = scene
            test.on_hud = hud
            test.capture.on_grab = lambda: test.capture.current.update(left=301) if row.get("move_grab") else None
            try:
                if row.get("op") == "close":
                    test.driver.close()
                    result = None
                elif case.get("native") and case.get("running"):
                    with patch.object(source.FieldControlRuntime, "running", new_callable=PropertyMock, return_value=True):
                        result = vars(test.driver.step())
                else:
                    result = vars(test.driver.step())
                value = dict(result=result)
            except Exception as error:
                value = dict(error=str(error))
            value.update(snapshot(test))
            rows.append(value)
    finally:
        test.tearDown()
    return rows


if __name__ == "__main__":
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("search runtime baseline changed")
    print(json.dumps([exercise(case) for case in json.load(sys.stdin)], allow_nan=False))
