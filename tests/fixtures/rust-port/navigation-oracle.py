"""경로 격자·선택의 개발용 비교. 화면 캡처·입력·실행기를 사용하지 않는다."""
import hashlib
import json
import sys
from dataclasses import asdict
from pathlib import Path

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner.navigation import ArcanePathObserver, PathSettings


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    source = next(line.split(":", 1)[1].strip() for line in cv.getBuildInformation().splitlines() if "Version control:" in line)
    if (sys.version.split()[0], np.__version__, cv.__version__, source) != (
        baseline["python"],
        baseline["numpy"],
        baseline["opencv"],
        baseline["opencv_version_control"],
    ):
        raise RuntimeError("navigation oracle baseline changed")
    result = []
    for group in json.load(sys.stdin):
        observer = ArcanePathObserver(
            PathSettings(**group["settings"]),
            terrain=group.get("terrain"),
            teleport=group.get("teleport"),
            automap_overlay=group.get("automap_overlay"),
        )
        rows = []
        for case in group["cases"]:
            anchor = tuple(case["anchor"])
            if case.get("path") or case.get("automap_reference_frame"):
                if case.get("path"):
                    frame = cv.imread(str(ROOT / case["path"]))
                else:
                    directory = ROOT / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
                    refs = json.loads((directory / "automap-floor-references.json").read_text(encoding="utf-8"))
                    frame = np.zeros((1140, 1922, 3), np.uint8)
                    for reference in refs["references"]:
                        l, t, r, b = reference["box"]
                        frame[t:b, l:r] = cv.imread(str(directory / reference["path"]))
                view = observer.observe(frame, sequence=1, generation=0, captured_at=1, viewport=tuple(case["viewport"]), anchor=anchor)
            else:
                width, height = case["size"]
                mask = np.full((height, width), case.get("fill", False), bool)
                for l, t, r, b, value in case.get("rects", []):
                    mask[t:b, l:r] = value
                view = observer.from_mask(mask, 1, 0, 1, (width, height), (0, 0, width, height), anchor)
            choices = []
            for options in case["choices"]:
                options = dict(options)
                wing = options.pop("wing")
                try:
                    choices.append(asdict(observer.choose(view, wing, **options)))
                except ValueError as error:
                    choices.append(dict(error=str(error)))
            rows.append(
                dict(
                    size=[view.floor.shape[1], view.floor.shape[0]],
                    floor=hashlib.sha256(view.floor.tobytes()).hexdigest(),
                    excluded=None if view.excluded_pixels is None else hashlib.sha256(view.excluded_pixels.tobytes()).hexdigest(),
                    choices=choices,
                )
            )
        result.append(rows)
    json.dump(result, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
