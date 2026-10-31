"""설치된 고정 OpenCV와 Rust 주석 픽셀을 비교한다. 화면/입력에는 접근하지 않는다."""
import hashlib
import json
import sys

import cv2 as cv
import numpy as np


def inspect(case):
    width, height = case["size"]
    frame = np.arange(width * height * 3, dtype=np.uint32)
    frame = ((frame * 37 + 11) % 256).astype(np.uint8).reshape(height, width, 3)
    for operation in case["operations"]:
        color = tuple(operation["color"])
        if operation["kind"] == "rectangle":
            cv.rectangle(frame, tuple(operation["first"]), tuple(operation["last"]), color, 2)
        elif operation["kind"] == "circle":
            cv.circle(frame, tuple(operation["center"]), operation["radius"], color, -1 if operation["filled"] else 2)
        else:
            cv.putText(
                frame, operation["text"], tuple(operation["origin"]), cv.FONT_HERSHEY_SIMPLEX, operation["scale"], color, 2, cv.LINE_AA
            )
    expected = frame.reshape(-1)
    actual = np.asarray(case["actual"], dtype=np.uint8)
    different = np.flatnonzero(expected != actual)
    return {
        "equal": len(different) == 0,
        "different": len(different),
        "first": [[int(n), int(expected[n]), int(actual[n])] for n in different[:20]],
        "sha256": hashlib.sha256(frame.tobytes()).hexdigest(),
    }


if __name__ == "__main__":
    assert cv.__version__ == "4.13.0"
    print(json.dumps([inspect(case) for case in json.load(sys.stdin)]))
