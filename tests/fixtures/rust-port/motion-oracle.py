"""Development-only OpenCV motion model comparison, without capture or input."""

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import cv2 as cv
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from diablo2.common.scene_motion import BackgroundMotionObserver


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    actual = (sys.version.split()[0], np.__version__, cv.__version__)
    expected = (baseline["python"], baseline["numpy"], baseline["opencv"])
    version_control = next(line.split(":", 1)[1].strip() for line in cv.getBuildInformation().splitlines() if "Version control:" in line)
    if actual != expected or version_control != baseline["opencv_version_control"]:
        raise RuntimeError("motion oracle baseline changed; review motion-baseline.json before comparing")
    request = json.load(sys.stdin)
    cv.setRNGSeed(42)
    if request["kind"] == "scale":
        gray = np.frombuffer(bytes.fromhex(request["gray"]), np.uint8).reshape(request["height"], request["width"])
        gray = cv.resize(gray, None, fx=request["scale"], fy=request["scale"])
        result = dict(size=[gray.shape[1], gray.shape[0]], gray=gray.tobytes().hex())
    elif request["kind"] == "observe":
        observer = BackgroundMotionObserver(**request["limits"])
        result = []
        for item in request["frames"]:
            if item.get("reset"):
                observer.reset()
            if "path" in item:
                frame = cv.imread(str(Path(__file__).resolve().parents[3] / item["path"]), cv.IMREAD_COLOR)
                if frame is None:
                    raise ValueError("motion fixture could not be decoded")
            else:
                frame = np.frombuffer(bytes.fromhex(item["bgr"]), np.uint8).reshape(item["height"], item["width"], 3)
            hint = None if item.get("hint") is None else tuple(item["hint"])
            if "view" in item:
                view = item["view"]
                view = SimpleNamespace(
                    sequence=item["sequence"],
                    captured_at=item["captured_at"],
                    frame_size=(item["width"], item["height"]),
                    viewport=tuple(view["viewport"]),
                    anchor=tuple(view["anchor"]),
                    cell_pixels=view["cell_pixels"],
                    floor=np.asarray(view["floor"], bool),
                    excluded_pixels=None if view.get("excluded_pixels") is None else np.asarray(view["excluded_pixels"], bool),
                )
                motion = observer.observe(frame, view, overlap_hint=hint)
            else:
                mask = np.frombuffer(bytes.fromhex(item["mask"]), np.uint8).reshape(frame.shape[:2])
                motion = observer.observe_mask(frame, mask, sequence=item["sequence"], captured_at=item["captured_at"], overlap_hint=hint)
            features = observer.features
            result.append(
                dict(
                    motion=vars(motion),
                    previous_sequence=None if observer.previous is None else observer.previous[0],
                    features=None
                    if features is None
                    else dict(
                        sequence=features.sequence,
                        captured_at=features.captured_at,
                        frame_size=features.frame_size,
                        points=features.points.tolist(),
                        descriptors=None if features.descriptors is None else features.descriptors.tolist(),
                    ),
                )
            )
    elif request["kind"] == "extract":
        image = np.frombuffer(bytes.fromhex(request["gray"]), np.uint8).reshape(request["height"], request["width"])
        mask = None if request.get("mask") is None else np.frombuffer(bytes.fromhex(request["mask"]), np.uint8).reshape(image.shape)
        keypoints, descriptors = cv.ORB_create(600).detectAndCompute(image, mask)
        result = dict(
            keypoints=[dict(point=k.pt, size=k.size, angle=k.angle, response=k.response, octave=k.octave) for k in keypoints],
            descriptors=[] if descriptors is None else descriptors.tolist(),
        )
    elif request["kind"] == "affine":
        source = np.asarray(request["source"], np.float32).reshape(-1, 2)
        target = np.asarray(request["target"], np.float32).reshape(-1, 2)
        model, mask = cv.estimateAffinePartial2D(source, target, method=cv.RANSAC, ransacReprojThreshold=request["threshold"])
        result = (
            None
            if model is None
            else dict(
                a=float(model[0, 0]), b=float(model[1, 0]), translation=model[:, 2].tolist(), inliers=mask.ravel().astype(bool).tolist()
            )
        )
    elif request["kind"] == "measure":
        observer = BackgroundMotionObserver(**request["limits"])
        source, target = request["source"], request["target"]
        source_points = [cv.KeyPoint(float(point[0]), float(point[1]), 31) for point in source["points"]]
        target_points = [cv.KeyPoint(float(point[0]), float(point[1]), 31) for point in target["points"]]
        result = vars(
            observer._measure(
                source_points,
                np.asarray(source["descriptors"], np.uint8).reshape(-1, 32),
                target_points,
                np.asarray(target["descriptors"], np.uint8).reshape(-1, 32),
            )
        )
    else:
        raise ValueError("unsupported oracle kind")
    json.dump(result, sys.stdout, ensure_ascii=True, allow_nan=False)


if __name__ == "__main__":
    main()
