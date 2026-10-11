import json
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np

from diablo2.actions.gem_summing import GemSummingSession
from diablo2.common.config import CaptureConfig


class GemAssetTests(unittest.TestCase):
    def session(self):
        return GemSummingSession(CaptureConfig())

    def test_constructor_uses_only_shared_runtime_assets(self):
        loaded = []
        load_image = GemSummingSession._load_image

        def track_load(session, path):
            loaded.append(path)
            return load_image(session, path)

        with patch.object(GemSummingSession, "_load_image", track_load):
            self.session()
        self.assertTrue(loaded)
        self.assertTrue(all("private" not in path.parts for path in loaded))
        self.assertIn(GemSummingSession.DIGIT_ATLAS_PATH, loaded)
        self.assertIn(GemSummingSession.STASH_HEADER_TEMPLATE_PATH, loaded)

    def test_digit_atlas_keeps_all_variant_banks_and_exact_glyph_pixels(self):
        session = self.session()
        manifest = json.loads(session.DIGIT_ATLAS_MANIFEST_PATH.read_text(encoding="utf-8"))
        atlas = session._load_image(session.DIGIT_ATLAS_PATH)[:, :, 0]
        banks = {"ones": session._ones_templates, "tens": session._tens_templates, "blank": session._blank_left_templates}
        offsets = {}
        for entry in manifest["entries"]:
            bank, variant, digit = entry["bank"], entry["variant"], entry["digit"]
            key = (bank, variant, digit)
            index = offsets.get(key, 0)
            glyphs = banks[bank][variant] if bank == "blank" else banks[bank][variant][digit]
            x, y, width, height = entry["box"]
            np.testing.assert_array_equal(atlas[y : y + height, x : x + width], glyphs[index])
            offsets[key] = index + 1
        variants = [name for name, _ in session.PREPROCESS_VARIANTS]
        self.assertEqual(variants, list(session._ones_templates))
        self.assertEqual(variants, list(session._tens_templates))
        self.assertEqual(variants, list(session._blank_left_templates))
        for variant in variants:
            self.assertEqual(set("0123456789"), set(session._ones_templates[variant]))
            self.assertTrue(session._tens_templates[variant])
            self.assertTrue(session._blank_left_templates[variant])

    def test_stash_header_matching_preserves_full_panel_origin_and_coordinates(self):
        session = self.session()
        width, height = session.STASH_PANEL_SIZE
        frame = np.zeros((height + 60, width + 40, 3), dtype=np.uint8)
        header = session._stash_header_template
        frame[30 : 30 + header.shape[0], 20 : 20 + header.shape[1]] = header
        result = session._locate_stash_panel(frame)
        self.assertIsNotNone(result)
        self.assertEqual((20, 30), result.top_left)
        self.assertEqual((width, height), (result.width, result.height))
        self.assertEqual((height, width, 3), session._extract_region(frame, result).shape)
        target = {"left": 100, "top": 200}
        self.assertEqual((380, 321), session._relative_point(target, result, *session.GEM_TAB_POINT))

    def test_stash_header_cannot_authorize_a_truncated_panel(self):
        session = self.session()
        self.assertIsNone(session._locate_stash_panel(session._stash_header_template.copy()))

    def test_actual_closed_and_empty_cube_panels_do_not_match_stash(self):
        session = self.session()
        paths = (
            Path("assets/ui/survival/references/closed.png"),
            Path("assets/ui/horadric-cube/references/empty-panel-ko-588x757.png"),
        )
        for path in paths:
            with self.subTest(path=path.name):
                self.assertTrue(path.exists(), "Required shared negative panel is missing.")
                self.assertIsNone(session._locate_stash_panel(session._load_image(path)))

    def test_invalid_atlas_contract_is_rejected_clearly(self):
        session = GemSummingSession.__new__(GemSummingSession)
        manifest = dict(schema=2, tile_size=[12, 14], entries=[])
        with patch.object(Path, "read_text", return_value=json.dumps(manifest)):
            with self.assertRaisesRegex(RuntimeError, "digit atlas"):
                session._load_digit_templates()


if __name__ == "__main__":
    unittest.main()
