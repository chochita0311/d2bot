import json
import tempfile
import unittest
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.navigation import ArcanePathObserver, PathSettings
from diablo2.runs.summoner.route_graph import ArcaneRouteGraph
from diablo2.runs.summoner.scene_vision import ArcaneHubObserver
from diablo2.runs.summoner.terrain_patterns import ArcaneTerrainPatterns
from diablo2.runs.summoner.terminal_vision import ArcaneTerminalObserver


class ArcaneTerrainPatternTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parents[1]
        self.owner = self.root / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        self.patterns = ArcaneTerrainPatterns(self.owner / "navigation-patterns.json")

    def test_shared_reference_supplies_reviewed_tread_and_translates_with_scene(self):
        manifest = json.loads((self.owner / "navigation-patterns.json").read_text(encoding="utf-8"))
        frame = cv.imread(str(self.root / manifest["reference"]))
        if frame is None:
            self.fail("Required shared reviewed scene unavailable")
        for dx, dy in ((0, 0), (120, -60)):
            shifted = cv.warpAffine(frame, np.float32([[1, 0, dx], [0, 1, dy]]), (1922, 1140))
            observed = self.patterns.observe(shifted)
            for rule in manifest["patterns"]:
                expected = tuple(value + delta for value, delta in zip(rule["source_point"], (dx, dy)))
                self.assertTrue(any(item.kind == rule["id"] and item.point == expected for item in observed))

    def test_blank_void_is_not_a_stair_landing(self):
        self.assertEqual((), self.patterns.observe(np.zeros((1140, 1922, 3), np.uint8)))

    def test_reorganized_templates_still_reject_traversal_and_unreviewed_subtrees(self):
        for observer, filename, parts in (
            (ArcaneTerrainPatterns, "navigation-patterns.json", "patterns"),
            (ArcaneHubObserver, "hub-patterns.json", "parts"),
            (ArcaneTerminalObserver, "terminal-patterns.json", "parts"),
        ):
            for invalid_path in ("../outside.png", "templates/unreviewed/part.png"):
                with self.subTest(observer=observer.__name__, path=invalid_path), tempfile.TemporaryDirectory() as temporary:
                    manifest = json.loads((self.owner / filename).read_text(encoding="utf-8"))
                    manifest[parts][0]["path"] = invalid_path
                    path = Path(temporary) / filename
                    path.write_text(json.dumps(manifest), encoding="utf-8")
                    with self.assertRaisesRegex(ValueError, "outside"):
                        observer(path)

    def test_actual_small_cap_uses_observed_east_stair_without_widening_floor_palette(self):
        manifest = json.loads((self.owner / "navigation-patterns.json").read_text(encoding="utf-8"))
        frame = cv.imread(str(self.root / manifest["reference"]))
        if frame is None:
            self.fail("Required shared reviewed scene unavailable")
        path = ArcanePathObserver(
            PathSettings(8, 0.5, 96, 120, 35, 90, 12), teleport=dict(maximum_distance=600, preferred_distance=600, clearance_pixels=32)
        )
        view = path.from_mask(np.zeros((840, 1872), np.uint8), 1, 0, 1, (1922, 1140), (24, 55, 1896, 895), (962, 578))
        graph = ArcaneRouteGraph()
        graph.observe(None, view.anchor, None, 1)
        choice = graph.choose(path, view, "north", 1, landings=self.patterns.observe(frame))
        self.assertEqual((1278, 846), choice.aim)
        self.assertEqual("graph_observed_exit_east", choice.reason)


if __name__ == "__main__":
    unittest.main()
