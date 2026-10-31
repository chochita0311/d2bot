"""개발 전용 OpenCV 수치 oracle; 앱/게임/입력은 실행하지 않는다."""

import json
import hashlib
import sys
from pathlib import Path

import cv2 as cv
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.common.frame_geometry import FrameGeometry, ReferenceLayout
from diablo2.common.detectors import TemplateMatcher
from diablo2.common.config import TemplateRule
from dataclasses import asdict


def evaluate(request):
    operation = request["op"]
    if operation == "audit_decodes":
        differences = []
        for item in request["images"]:
            image = cv.imdecode(
                np.fromfile(item["path"], dtype=np.uint8), cv.IMREAD_GRAYSCALE if item.get("mode") == "gray" else cv.IMREAD_UNCHANGED
            )
            actual = hashlib.sha256(image.tobytes()).hexdigest()
            if actual != item["sha256"]:
                differences.append(item["path"])
        return {"count": len(request["images"]), "differences": differences}
    if operation == "decode_compare":
        original = cv.imdecode(np.fromfile(request["path"], dtype=np.uint8), cv.IMREAD_COLOR)
        actual = cv.imdecode(np.frombuffer(bytes.fromhex(request["png_hex"]), dtype=np.uint8), cv.IMREAD_COLOR)
        return {"max_difference": int(np.max(np.abs(original.astype(np.int16) - actual.astype(np.int16))))}
    if operation == "jpeg_reference_decisions":
        from diablo2.actions.town_loop_vision import TownLoopVision

        vision = TownLoopVision()
        results = []
        for item in request["images"]:
            old = cv.imdecode(np.fromfile(item["original"], dtype=np.uint8), cv.IMREAD_COLOR)
            new = cv.imdecode(np.fromfile(item["decoded"], dtype=np.uint8), cv.IMREAD_COLOR)
            differences = np.abs(old.astype(np.int16) - new.astype(np.int16))
            readings = []
            for image in (old, new):
                gray = vision.normalize(image)
                readings.append({name: vision.find(gray, name) for name in vision.templates})
            changed = []
            maximum_score_delta = 0.0
            for name in vision.templates:
                a, b = readings[0][name], readings[1][name]
                if a is not None and b is not None:
                    maximum_score_delta = max(maximum_score_delta, abs(a.get("score", 0) - b.get("score", 0)))
                a = {key: value for key, value in a.items() if key != "score"} if a is not None else None
                b = {key: value for key, value in b.items() if key != "score"} if b is not None else None
                if a != b:
                    changed.append({"template": name, "old": a, "new": b})
            results.append(
                {
                    "path": item["original"],
                    "maximum_pixel_difference": int(differences.max()),
                    "mean_pixel_difference": float(differences.mean()),
                    "channels_over_two": int((differences > 2).sum()),
                    "total_channels": int(differences.size),
                    "decision_checks": len(vision.templates),
                    "differences": changed,
                    "maximum_score_delta": maximum_score_delta,
                }
            )
        return results
    if operation == "decode":
        flag = {"unchanged": cv.IMREAD_UNCHANGED, "color": cv.IMREAD_COLOR, "gray": cv.IMREAD_GRAYSCALE}[request.get("mode", "unchanged")]
        image = cv.imdecode(np.fromfile(request["path"], dtype=np.uint8), flag)
        if image is None:
            raise ValueError("image decode failed")
    else:
        source = np.asarray(request["pixels"], dtype=request.get("dtype", "uint8"))
        channels = request.get("channels", 3)
        shape = (request["height"], request["width"]) + ((channels,) if channels != 1 else ())
        source = source.reshape(shape)
        if operation == "resize":
            mode = {"nearest": cv.INTER_NEAREST, "linear": cv.INTER_LINEAR, "area": cv.INTER_AREA}[request["mode"]]
            image = cv.resize(source, tuple(request["size"]), interpolation=mode)
        elif operation in ("gray", "hsv", "lab"):
            image = cv.cvtColor(source, {"gray": cv.COLOR_BGR2GRAY, "hsv": cv.COLOR_BGR2HSV, "lab": cv.COLOR_BGR2LAB}[operation])
        elif operation == "normalize":
            raw = request.get("layout")
            layout = ReferenceLayout(tuple(raw["frame_size"]), tuple(raw["client_box"]), raw.get("aspect_tolerance", 0.01)) if raw else None
            geometry = FrameGeometry.create(request["target"], layout, request.get("client"))
            image = geometry.normalize(source)
        elif operation == "gaussian":
            image = cv.GaussianBlur(source, (request["kernel"], request["kernel"]), request["sigma"])
        elif operation == "box":
            image = cv.boxFilter(source, -1, tuple(request["kernel"]))
        elif operation == "threshold":
            value, image = cv.threshold(
                source, request.get("threshold", 0), 255, cv.THRESH_BINARY | (cv.THRESH_OTSU if request.get("otsu") else 0)
            )
            return {"threshold": value, "pixels": image.ravel().tolist()}
        elif operation == "morphology":
            kernel = (
                cv.getStructuringElement(cv.MORPH_CROSS, (3, 3))
                if request.get("cross")
                else np.ones((request["kernel"][1], request["kernel"][0]), np.uint8)
            )
            operations = {"erode": cv.MORPH_ERODE, "dilate": cv.MORPH_DILATE, "open": cv.MORPH_OPEN, "close": cv.MORPH_CLOSE}
            image = cv.morphologyEx(source, operations[request["mode"]], kernel)
        elif operation == "components":
            count, labels, stats, centers = cv.connectedComponentsWithStats(source, connectivity=request["connectivity"])
            return {
                "labels": labels.ravel().tolist(),
                "stats": [
                    dict(
                        label=index,
                        area=int(stats[index, 4]),
                        rect=[
                            int(stats[index, 0]),
                            int(stats[index, 1]),
                            int(stats[index, 0] + stats[index, 2]),
                            int(stats[index, 1] + stats[index, 3]),
                        ]
                        if stats[index, 4]
                        else None,
                        centroid=centers[index].tolist() if stats[index, 4] else None,
                    )
                    for index in range(count)
                ],
            }
        elif operation == "distance":
            image = cv.distanceTransform(source, cv.DIST_L2, 5)
        elif operation == "match":
            template = np.asarray(request["template_pixels"], dtype=source.dtype).reshape(
                (request["template_height"], request["template_width"]) + ((channels,) if channels != 1 else ())
            )
            method = {
                "correlation": cv.TM_CCORR,
                "normalized_correlation": cv.TM_CCORR_NORMED,
                "normalized_coefficient": cv.TM_CCOEFF_NORMED,
            }[request["method"]]
            image = cv.matchTemplate(source, template, method)
            _, score, _, location = cv.minMaxLoc(image)
            return {
                "width": image.shape[1],
                "height": image.shape[0],
                "pixels": image.ravel().tolist(),
                "peak": {"score": score, "location": location},
            }
        elif operation == "detector":
            matcher = TemplateMatcher([TemplateRule(**rule) for rule in request["rules"]])
            return [asdict(match) for match in matcher.scan(source)]
        else:
            raise ValueError("unknown operation")
    return {
        "width": image.shape[1],
        "height": image.shape[0],
        "channels": image.shape[2] if image.ndim == 3 else 1,
        "pixels": image.ravel().tolist(),
    }


request = json.load(sys.stdin)
result = [evaluate(item) for item in request] if isinstance(request, list) else evaluate(request)
print(json.dumps(result, ensure_ascii=False, allow_nan=False))
