"""기존 경로 그래프 테스트의 입력·수동 상태 변경·결과를 기록한다. 실제 입력은 없다."""
import copy
import importlib.util
import json
import sys
import unittest
from dataclasses import asdict
from pathlib import Path
from unittest.mock import patch as override

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.common.scene_motion import SceneFeatures
from diablo2.runs.summoner.route_graph import ArcaneRouteGraph

FIELDS = (
    "entry",
    "exit_intent",
    "pending_exit",
    "pending_retreat",
    "traversed",
    "failed_exits",
    "escape_retreats",
    "places",
    "place_charts",
)


def encoded(value):
    if isinstance(value, dict):
        return [[key, count] for key, count in sorted(value.items(), key=lambda item: repr(item[0]))]
    if isinstance(value, set):
        return sorted(value, key=repr)
    return value


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("route graph oracle baseline changed")
    events = []
    next_id = [0]

    class RecordedGraph(ArcaneRouteGraph):
        def __init__(self, **kwargs):
            super().__init__(**kwargs)
            self.trace_id = next_id[0]
            next_id[0] += 1
            self.trace_before = self.snapshot()
            events.append(dict(id=self.trace_id, kind="new", options=kwargs))

        def snapshot(self):
            return {name: copy.deepcopy(getattr(self, name)) for name in FIELDS}

        def begin(self, kind, **data):
            current = self.snapshot()
            # 이전 결과와의 차이만 원래 테스트가 직접 바꾼 상태로 전달한다.
            changes = {name: encoded(value) for name, value in current.items() if value != self.trace_before[name]}
            return dict(id=self.trace_id, kind=kind, patch=changes, **data)

        def finish(self, event, result):
            event.update(outcome=result, summary=self.summary())
            events.append(event)
            self.trace_before = self.snapshot()
            return result

        def observe(self, features, anchor, displacement, captured_at):
            # 실제 ORB 경계의 native 좌표는 float32이다. 합성 테스트도 같은 입력으로 비교한다.
            payload = None
            if features is not None:
                features = SceneFeatures(
                    features.sequence,
                    features.captured_at,
                    features.frame_size,
                    np.asarray(features.points, dtype=np.float32),
                    features.descriptors,
                )
                payload = dict(
                    sequence=features.sequence,
                    captured_at=features.captured_at,
                    frame_size=features.frame_size,
                    points=features.points.tolist(),
                    descriptors=None if features.descriptors is None else features.descriptors.tolist(),
                )
            event = self.begin("observe", features=payload, anchor=anchor, displacement=displacement, at=captured_at)
            return self.finish(event, super().observe(features, anchor, displacement, captured_at))

        def fail_landing(self, aim, anchor):
            event = self.begin("fail", aim=aim, anchor=anchor)
            return self.finish(event, super().fail_landing(aim, anchor))

        def choose(self, path, view, wing, now, **kwargs):
            event = self.begin(
                "choose",
                settings=asdict(path.settings),
                teleport=copy.deepcopy(path.teleport),
                wing=wing,
                now=now,
                view=dict(
                    sequence=view.sequence,
                    generation=view.generation,
                    captured_at=view.captured_at,
                    frame_size=view.frame_size,
                    viewport=view.viewport,
                    anchor=view.anchor,
                    cell_pixels=view.cell_pixels,
                    size=view.floor.shape[::-1],
                    floor=view.floor.astype(np.uint8).tobytes().hex(),
                ),
                excluded=kwargs.get("excluded", ()),
                landings=[dict(kind=item.kind, point=item.point, score=item.score) for item in kwargs.get("landings", ())],
                hub=kwargs.get("hub"),
                prefer_goal_exit=kwargs.get("prefer_goal_exit", False),
            )
            result = super().choose(path, view, wing, now, **kwargs)
            self.finish(event, asdict(result))
            return result

    spec = importlib.util.spec_from_file_location("route_graph_original_tests", ROOT / "tests/test_arcane_route_graph.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(module.ArcaneRouteGraphTests)
    result = unittest.TestResult()
    with override.object(module, "ArcaneRouteGraph", RecordedGraph):
        suite.run(result)
    if not result.wasSuccessful():
        raise RuntimeError(str(result.errors + result.failures))

    def features(at, seed=42, count=100, scale=1.0, inliers=None, frame_size=(1922, 1140)):
        random = np.random.RandomState(seed)
        points = random.uniform((120, 180), (1720, 890), (count, 2)).astype(np.float32)
        descriptors = random.randint(0, 256, (count, 32)).astype(np.uint8)
        points = (points * scale).astype(np.float32)
        if inliers is not None:
            points[inliers:] = random.uniform((120, 180), (1720, 890), (count - inliers, 2))
        return SceneFeatures(at, float(at), frame_size, points, descriptors)

    # 기존 사례에 없는 대응 수·비율·선형 오차·반복 타일 거부와 참조 순환도 비교한다.
    for count in (23, 24):
        graph = RecordedGraph()
        graph.observe(features(1, count=count), (960, 580), None, 1)
        graph.observe(features(2, count=count), (960, 580), None, 2)
    for scale in (1.029, 1.031):
        graph = RecordedGraph()
        graph.observe(features(1), (960, 580), None, 1)
        graph.observe(features(2, scale=scale), (960, 580), None, 2)
    for inliers in (69, 70):
        graph = RecordedGraph()
        graph.observe(features(1), (960, 580), None, 1)
        graph.observe(features(2, inliers=inliers), (960, 580), None, 2)
    graph = RecordedGraph()
    graph.observe(features(1), (960, 580), None, 1)
    graph.observe(features(2, frame_size=(1900, 1140)), (960, 580), None, 2)
    graph = RecordedGraph()
    graph.observe(features(1), (960, 580), None, 1)
    graph.observe(features(2), (960, 580), (400, -200), 2)
    graph.observe(features(3), (960, 580), None, 3)
    graph = RecordedGraph(maximum_references=6)
    for at in range(1, 7):
        graph.observe(features(at, seed=at), (960, 580), (400, -200), at)
    for at, seed in ((7, 1), (8, 3), (9, 2), (10, 2)):
        graph.observe(features(at, seed=seed), (960, 580), None, at)
    json.dump(dict(cases=result.testsRun, extra_sequences=9, graphs=next_id[0], events=events), sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
