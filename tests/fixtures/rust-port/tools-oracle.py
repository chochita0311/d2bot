"""입력 없는 기존 도구의 출력/오류와 Rust 생성 파일을 비교한다."""
import contextlib
import argparse
import hashlib
import io
import json
import sys
from pathlib import Path
from unittest.mock import patch

import cv2 as cv
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from diablo2.common.config import load_config


def inspect(request):
    try:
        if request["op"] == "probe-options":
            from diablo2.tools import field_input_probe

            class Parsed(Exception):
                pass

            captured = {}
            original = argparse.ArgumentParser.parse_args

            def parse(parser, argv=None):
                result = original(parser, argv)
                captured.update(vars(result))
                return result

            with patch.object(argparse.ArgumentParser, "parse_args", parse), patch.object(
                field_input_probe, "load_config", side_effect=Parsed
            ), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                try:
                    field_input_probe.main(request["arguments"])
                except Parsed:
                    return {"ok": True, "value": captured}
        if request["op"] == "field":
            from diablo2.tools.field_replay import replay

            return {"ok": True, "value": replay(load_config(Path("config")), "flash", "demo", request["scenario"])}
        if request["op"] == "survival":
            from diablo2.tools.survival_observe import main

            stream = io.StringIO()
            with contextlib.redirect_stdout(stream), contextlib.redirect_stderr(io.StringIO()):
                code = main(request["arguments"])
            return {"ok": True, "exit": code, "value": json.loads(stream.getvalue())}
        if request["op"] in ("town-calibrate", "town-scene"):
            from diablo2.actions.town_loop_vision import calibrate, calibrate_scene

            directory = Path(request["directory"])
            if request["op"] == "town-calibrate":
                calibrate(Path(request["evidence"]), directory)
            else:
                calibrate_scene(Path(request["frame"]), request["scene"], directory)
            manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
            actual = json.loads((Path(request["rust_directory"]) / "manifest.json").read_text(encoding="utf-8"))
            equal, hashes = [], []
            for name, definition in manifest["templates"].items():
                paths = [definition["file"]] + definition.get("variants", [])
                for filename in paths:
                    first = directory / filename
                    second = Path(request["rust_directory"]) / filename
                    equal.append(
                        bool(np.array_equal(cv.imread(str(first), cv.IMREAD_UNCHANGED), cv.imread(str(second), cv.IMREAD_UNCHANGED)))
                    )
                    if filename in definition.get("variant_asset_sha256", {}) and request["op"] == "town-scene" and name == "play":
                        hashes.append(
                            actual["templates"][name]["variant_asset_sha256"][filename] == hashlib.sha256(second.read_bytes()).hexdigest()
                        )
                    if request["op"] == "town-calibrate":
                        hashes.append(actual["templates"][name]["asset_sha256"] == hashlib.sha256(second.read_bytes()).hexdigest())
                definition.pop("asset_sha256", None)
                definition.pop("variant_asset_sha256", None)
                actual["templates"][name].pop("asset_sha256", None)
                actual["templates"][name].pop("variant_asset_sha256", None)
            result = {
                "ok": True,
                "metadata_equal": actual == manifest,
                "pixels_equal": all(equal),
                "hashes_valid": all(hashes),
                "templates": len(manifest["templates"]),
            }
            if actual != manifest:
                result["differences"] = [
                    {"name": name, "python": definition, "rust": actual["templates"].get(name)}
                    for name, definition in manifest["templates"].items()
                    if actual["templates"].get(name) != definition
                ]
            return result
        if request["op"] == "town-step":
            from diablo2.actions import town_loop_vision

            stream = io.StringIO()
            with patch.object(sys, "argv", ["town-loop-vision"] + request["arguments"]), patch.object(
                town_loop_vision.time, "time", return_value=request["now"]
            ), contextlib.redirect_stdout(stream):
                code = town_loop_vision.main()
            return {"ok": True, "exit": code, "value": json.loads(stream.getvalue())}
        from diablo2.tools.arcane_navigation_replay import replay

        overlays = Path(request["overlays"])
        value = replay(Path(request["manifest"]), request["calibration"], overlays=overlays)
        equal = []
        for path in sorted(overlays.glob("*.png")):
            original = cv.imread(str(path))
            actual = cv.imread(str(Path(request["rust_overlays"]) / path.name))
            equal.append(bool(actual is not None and np.array_equal(original, actual)))
        return {"ok": True, "value": value, "overlays_equal": equal}
    except SystemExit as error:
        return {"ok": False, "exit": error.code}
    except (ValueError, TypeError, KeyError, OSError) as error:
        return {"ok": False, "error": type(error).__name__, "message": str(error)}


if __name__ == "__main__":
    print(json.dumps([inspect(request) for request in json.load(sys.stdin)], ensure_ascii=False, default=str))
