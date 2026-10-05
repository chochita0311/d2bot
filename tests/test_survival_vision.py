import copy
import io
import json
import unittest
import sys
from contextlib import redirect_stdout, redirect_stderr
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace

import cv2 as cv
import numpy as np

from diablo2.common.config import load_config
from diablo2.common.survival import CharacterSurvivalState, ObservationStamp
from diablo2.common.survival_vision import DEFAULT_ASSETS, ResourceReading, SurvivalHudObserver, glyph_parts, text_mask
from diablo2.tools.survival_observe import build_parser, main


ROOT = Path(__file__).resolve().parents[1]


class SurvivalVisionTests(unittest.TestCase):
    def setUp(self):
        self.observer = SurvivalHudObserver()
        self.policy = load_config(ROOT / "config").characters["flash"].survival

    def reference(self, name):
        reference = next(value for value in self.observer.manifest["references"] if value["path"] == f"references/{name}.png")
        width, height = self.observer.frame_size
        frame = np.zeros((height, width, 3), dtype=np.uint8)
        left, top, right, bottom = reference["canvas_box"]
        frame[top:bottom, left:right] = cv.imread(str(DEFAULT_ASSETS / reference["path"]))
        return frame

    def decision(self, reading):
        state = CharacterSurvivalState("test-profile", "test-room", self.policy)
        stamp = ObservationStamp("test-profile", "test-room", 0, 10)
        contents = reading.belt_contents(self.policy)
        if contents is not None:
            self.assertTrue(state.observe_belt(stamp, contents))
        return state.decide(reading.safety_observation(stamp), 10, max_observation_age_seconds=1, max_belt_age_seconds=1)

    def paint_number(self, frame, key, value):
        box = self.observer.manifest["resources"][key]["number_box"]
        left, top, right, bottom = box
        frame[top:bottom, left:right] = 0
        x = left + 4
        for character in value:
            glyph = self.observer.glyphs[character]
            ys, xs = np.nonzero(glyph)
            tight = glyph[ys.min() : ys.max() + 1, xs.min() : xs.max() + 1]
            if character == "/":
                source_box = self.observer.manifest["resources"]["life"]["number_box"]
                mask = glyph_parts(text_mask(self.observer._crop(self.reference("expanded"), source_box)))[3] * 255
                height, width = mask.shape
            elif character == "4":
                original = cv.imread(str(ROOT / "assets/character/play/status.png"))[863:895, 1415:1535]
                mask = glyph_parts(text_mask(original))[0] * 255
                height, width = mask.shape
            else:
                height = 14
                width = max(1, round(tight.shape[1] * height / tight.shape[0]))
                mask = cv.resize(tight, (width, height), interpolation=cv.INTER_NEAREST) * 255
            y = top + (3 if character == "/" else 4)
            self.assertLessEqual(x + width, right)
            frame[y : y + height, x : x + width] = mask[:, :, None]
            x += width + 4

    def test_actual_obscured_separator_remains_unknown(self):
        import hashlib

        for reference in self.observer.manifest["negative_references"]:
            with self.subTest(path=reference["path"]):
                path = self.observer.asset_directory / reference["path"]
                self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), reference["sha256"])
                frame = np.zeros((1140, 1922, 3), np.uint8)
                left, top, right, bottom = reference["canvas_box"]
                frame[top:bottom, left:right] = cv.imread(str(path))
                reading = self.observer.observe(frame)
                self.assertEqual((reading.life.current, reading.life.maximum), tuple(reference["life"]))
                self.assertEqual(reading.mana.reason, reference["expected_mana_reason"])
                self.assertIsNone(reading.mana.ratio)

    def test_actual_held_nova_reads_reduced_mana_with_buffed_maximum(self):
        reading = self.observer.observe(self.reference("field-nova-mana"))
        self.assertEqual((reading.life.current, reading.life.maximum), (1580, 1580))
        self.assertEqual((reading.mana.current, reading.mana.maximum), (1457, 1543))
        self.assertEqual(reading.mana.ratio, 1457 / 1543)
        self.assertEqual(reading.belt_visibility, "closed")
        self.assertIsNone(reading.belt_contents(self.policy))

    def test_actual_captured_references_have_reviewed_numbers_and_counts(self):
        for name, expected in (("expanded", [4, 4, 4, 4]), ("eleven", [3, 4, 4, 4]), ("restored", [4, 4, 4, 4])):
            with self.subTest(name=name):
                reading = self.observer.observe(self.reference(name))
                self.assertEqual(
                    (reading.life.current, reading.life.maximum, reading.mana.current, reading.mana.maximum), (985, 985, 1075, 1075)
                )
                self.assertEqual(reading.belt_status, "read")
                self.assertEqual([value[1] for value in reading.belt_contents(self.policy).values()], expected)
                self.assertEqual(self.decision(reading).reason, "buff_requires_field")

    def test_actual_closed_belt_does_not_infer_the_hidden_rows(self):
        reading = self.observer.observe(self.reference("closed"))
        self.assertEqual((reading.life.ratio, reading.mana.ratio), (1, 1))
        self.assertIsNone(reading.belt_contents(self.policy))
        self.assertEqual(self.decision(reading).reason, "belt_unknown")

    def test_wrong_layout_and_non_hud_screen_hold_without_fake_zero(self):
        for frame in (np.zeros((1140, 1922, 3), dtype=np.uint8), np.zeros((753, 1267, 3), dtype=np.uint8)):
            reading = self.observer.observe(frame)
            self.assertIsNone(reading.life.ratio)
            self.assertIsNone(reading.mana.ratio)
            self.assertIsNone(reading.belt_contents(self.policy))

    def test_occluded_resource_and_slot_stay_unknown(self):
        frame = self.reference("expanded")
        left, top, right, bottom = self.observer.manifest["resources"]["life"]["number_box"]
        frame[top:bottom, left:right] = 0
        reading = self.observer.observe(frame)
        self.assertIsNone(reading.life.ratio)
        self.assertEqual(self.decision(reading).reason, "hud_unknown")
        frame = self.reference("expanded")
        left, top = self.observer.manifest["belt"]["origin"]
        top += self.observer.manifest["belt"]["step"][1]
        frame[top : top + 57, left : left + 60] = 255
        reading = self.observer.observe(frame)
        self.assertEqual(reading.belt_status, "slot_unverified")
        self.assertIsNone(reading.belt_contents(self.policy))

    def test_number_parser_rejects_bad_ranges_and_malformed_pairs(self):
        for value in ("1/0", "100/99", "1", "1//2", "-1/2", "1.5/2", "01/2", "1/02", "x/2"):
            self.assertIsNone(ResourceReading.from_text(value, 1).ratio)
        self.assertEqual(ResourceReading.from_text("0/100", 1).ratio, 0)

    def test_synthetic_maximum_changes_refresh_ratio_and_thresholds(self):
        first = self.observer.observe(self.reference("expanded"))
        for current, maximum, action in ((985, 985, "hold"), (985, 1970, "hold"), (984, 1970, "use_potion")):
            with self.subTest(current=current, maximum=maximum):
                frame = self.reference("expanded")
                self.paint_number(frame, "life", f"{current}/{maximum}")
                reading = self.observer.observe(frame)
                self.assertEqual((reading.life.current, reading.life.maximum), (current, maximum))
                self.assertEqual(self.decision(reading).action, action)
        self.assertEqual(first.life.maximum, 985)
        for current, action in ((108, "hold"), (107, "use_potion")):
            frame = self.reference("expanded")
            self.paint_number(frame, "mana", f"{current}/1080")
            reading = self.observer.observe(frame)
            self.assertEqual((reading.mana.current, reading.mana.maximum), (current, 1080))
            self.assertEqual(self.decision(reading).action, action)

    def test_unreadable_new_maximum_does_not_reuse_previous_ratio(self):
        self.assertEqual(self.observer.observe(self.reference("expanded")).life.ratio, 1)
        frame = self.reference("expanded")
        self.paint_number(frame, "life", "985/0")
        reading = self.observer.observe(frame)
        self.assertIsNone(reading.life.ratio)
        self.assertEqual(self.decision(reading).reason, "hud_unknown")

    def test_synthetic_empty_slots_distinguish_replenishment_and_exit_request(self):
        original = self.observer.observe(self.reference("expanded"))
        for count in (6, 0):
            slots = tuple(
                replace(slot, kind="empty") if slot.column <= 3 and (slot.row - 1) * 3 + slot.column > count else slot
                for slot in original.slots
            )
            reading = replace(original, slots=slots)
            decision = self.decision(reading)
            self.assertTrue(decision.replenishment_required)
            self.assertEqual(decision.action, "exit" if count == 0 else "hold")
        self.assertEqual(self.decision(original).action, "hold")

    def test_wrong_kind_or_capacity_does_not_confirm_a_belt(self):
        reading = self.observer.observe(self.reference("expanded"))
        slots = tuple(replace(slot, kind="town_portal_scroll") if slot.column == 1 and slot.row == 1 else slot for slot in reading.slots)
        self.assertIsNone(replace(reading, slots=slots).belt_contents(self.policy))
        columns = tuple(
            replace(column, capacity=2, target_count=2) if column.kind == "purple_potion" else column for column in self.policy.belt_columns
        )
        self.assertIsNone(reading.belt_contents(replace(self.policy, belt_columns=columns)))
        slots = (reading.slots[0],) + reading.slots[1:-1] + (reading.slots[0],)
        self.assertIsNone(replace(reading, slots=slots).belt_contents(self.policy))

    def test_live_minimized_window_is_rejected_and_capture_is_closed(self):
        closed = []

        class FakeCapture:
            def __init__(self, config):
                self.config = config

            def grab(self):
                raise AssertionError("Minimized window must not be read")

            def close(self):
                closed.append(True)

        capture_api = SimpleNamespace(
            ScreenCapture=FakeCapture,
            USER32=SimpleNamespace(IsIconic=lambda handle: True, IsWindowVisible=lambda handle: True),
            resolve_window_from_config=lambda config: SimpleNamespace(handle=1),
        )
        with patch.dict(sys.modules, {"diablo2.common.capture": capture_api}), self.assertRaises(RuntimeError):
            main(["--live"])
        self.assertEqual(closed, [True])

    def test_observer_does_not_claim_field_safety_or_buff_effect(self):
        reading = self.observer.observe(self.reference("expanded"))
        observation = reading.safety_observation(ObservationStamp("another", "room", 1, 10))
        self.assertEqual(observation.location, "unknown")
        self.assertIsNone(observation.monsters_clear)
        self.assertEqual((observation.stamp.character_id, observation.stamp.room_id), ("another", "room"))

    def test_invalid_frame_types_and_bad_manifest_limits_fail(self):
        for frame in (None, np.zeros((10, 10), dtype=np.uint8), np.zeros((1140, 1922, 3), dtype=np.float32)):
            with self.assertRaises(ValueError):
                self.observer.observe(frame)
        for value in (float("nan"), float("inf"), True, -1, 0, 1):
            raw = copy.deepcopy(self.observer.manifest)
            raw["matching"]["glyph_score"] = value
            with patch.object(Path, "read_text", return_value=json.dumps(raw)), self.assertRaises(ValueError):
                SurvivalHudObserver()

    def test_cli_offline_is_input_free_and_policy_context_is_explicit(self):
        output = io.StringIO()
        with patch("diablo2.tools.survival_observe.SurvivalHudObserver", return_value=self.observer), patch(
            "diablo2.tools.survival_observe.cv.imread", return_value=self.reference("expanded")
        ), redirect_stdout(output):
            self.assertEqual(main(["--image", "fixture.png", "--character", "flash", "--room", "test-room", "--age-limit", "1"]), 0)
        result = json.loads(output.getvalue())
        self.assertEqual((result["mode"], result["inputs_sent"]), ("observation_only", 0))
        self.assertEqual(result["reports"][0]["source"], "offline_image")
        self.assertEqual(result["reports"][0]["decision"]["action"], "hold")
        with redirect_stderr(io.StringIO()):
            for arguments in (
                ["--live", "--frames", "6"],
                ["--live", "--character", "flash"],
                ["--live", "--character", "flash", "--room", "room"],
            ):
                with self.assertRaises(SystemExit):
                    main(arguments)
            with self.assertRaises(SystemExit):
                build_parser().parse_args(["--live", "--image", "fixture.png"])


if __name__ == "__main__":
    unittest.main()
