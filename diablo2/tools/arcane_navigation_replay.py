from __future__ import annotations

import argparse
import json
import time
from dataclasses import asdict
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.runs.summoner.navigation import ArcanePathObserver, BackgroundMotionObserver, PathSettings


def replay(manifest_path, calibration, *, overlays=None):
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    width, height = manifest["frame_size"]
    if any(type(value) is not int or value <= 0 for value in (width, height)) or width * height > 8_000_000:
        raise ValueError("bounded native frame size required")
    if calibration.get("mode") != "input_free_calibration" or calibration["frame_size"] != [width, height]:
        raise ValueError("explicit input-free calibration for this frame size required")
    cases = manifest["cases"]
    if not isinstance(cases, list) or not 1 <= len(cases) <= 100:
        raise ValueError("one to 100 source cases required")
    terrain = calibration["terrain"] if "terrain" in calibration else manifest["terrain"]
    observer = ArcanePathObserver(PathSettings(**calibration["path"]), terrain=terrain, teleport=calibration.get("teleport"))
    motion = BackgroundMotionObserver(**calibration["motion"])
    if overlays is not None:
        overlays.mkdir(parents=True, exist_ok=False)
    results, previous = [], None
    for sequence, case in enumerate(cases):
        viewport = tuple(case["viewport"])
        x0, y0, x1, y1 = viewport
        if not 0 <= x0 < x1 <= width or not 0 <= y0 < y1 <= height:
            raise ValueError("case viewport outside native frame")
        crop = cv.imread(str(manifest_path.parent / case["path"]))
        if crop is None or crop.shape != (y1 - y0, x1 - x0, 3):
            raise ValueError("source crop and viewport must match without rescaling")
        source = case["provenance"]
        source_time = source["seconds_from_record_start"]
        if previous is None or previous["episode"] != source["episode"] or source_time <= previous["seconds_from_record_start"]:
            motion.reset()
        frame = np.zeros((height, width, 3), np.uint8)
        frame[y0:y1, x0:x1] = crop
        began = time.monotonic()
        view = observer.observe(
            frame,
            sequence=sequence,
            generation=0,
            captured_at=source_time,
            viewport=viewport,
            anchor=tuple(calibration["anchor"]),
        )
        choice = observer.choose(view, calibration["wing"])
        displacement = motion.observe(frame, view)
        results.append(
            {
                "case": case["id"],
                "layout": case.get("layout"),
                "source": source,
                "choice": asdict(choice),
                "motion": asdict(displacement),
                "vision_seconds": time.monotonic() - began,
            }
        )
        if overlays is not None:
            # 그림은 후보 검토용이며 이동/안전/실제 도착의 정답 표시가 아니다.
            marked = crop.copy()
            anchor = tuple(int(value - offset) for value, offset in zip(view.anchor, (x0, y0)))
            cv.circle(marked, anchor, 10, (0, 0, 255), 2)
            for point in choice.path:
                cv.circle(marked, (round(point[0] - x0), round(point[1] - y0)), 3, (255, 0, 255), -1)
            if choice.aim is not None:
                cv.circle(marked, (choice.aim[0] - x0, choice.aim[1] - y0), 8, (255, 255, 0), 2)
            if not cv.imwrite(str(overlays / ("case-%03d.png" % sequence)), marked):
                raise RuntimeError("diagnostic image write failed")
        previous = source
    return {"mode": "input_free_calibration", "inputs_sent": 0, "revision": manifest["revision"], "results": results}


def main(argv=None):
    parser = argparse.ArgumentParser(description="Input-free Arcane terrain and background-motion calibration; no gameplay input")
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--calibration", type=Path, required=True)
    parser.add_argument("--overlays", type=Path)
    args = parser.parse_args(argv)
    if max(args.manifest.stat().st_size, args.calibration.stat().st_size) > 1024 * 1024:
        parser.error("JSON inputs must be at most 1 MiB")
    calibration = json.loads(args.calibration.read_text(encoding="utf-8"))
    print(json.dumps(replay(args.manifest, calibration, overlays=args.overlays), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
