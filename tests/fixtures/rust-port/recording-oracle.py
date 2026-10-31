"""개발 전용 OpenCV 독립 디코딩과 동일 입력 XVID 비교. 게임 효과 없음."""
import hashlib
import json
import math
from pathlib import Path
import sys
import time

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
assert cv.__version__ == "4.13.0"
assert np.__version__ == "1.21.6"


def frame_at(images, pattern, index):
    source = images[(index // 8) % len(images)] if pattern == "cuts" else images[0]
    return np.roll(source, index * 3 % source.shape[1], axis=1) if pattern == "pan" else source


def inspect(path, expected=None, pixels=False, quality_crop=None):
    reader = cv.VideoCapture(str(path))
    if not reader.isOpened():
        raise RuntimeError("Independent AVI decoder could not open: " + str(path))
    result = {
        "backend": reader.getBackendName(),
        "width": int(reader.get(cv.CAP_PROP_FRAME_WIDTH)),
        "height": int(reader.get(cv.CAP_PROP_FRAME_HEIGHT)),
        "fps": reader.get(cv.CAP_PROP_FPS),
        "reported_frames": int(reader.get(cv.CAP_PROP_FRAME_COUNT)),
        "fourcc": int(reader.get(cv.CAP_PROP_FOURCC)),
    }
    frames = []
    hashes = []
    frame_errors = []
    count, squared, maximum, elements = 0, 0.0, 0, 0
    try:
        while True:
            ok, frame = reader.read()
            if not ok:
                break
            if expected is not None:
                source = expected(count)
                compared = frame if quality_crop is None else frame[: quality_crop[1], : quality_crop[0]]
                if compared.shape != source.shape:
                    raise RuntimeError("Decoded frame dimensions differ from input")
                error = cv.norm(compared, source, cv.NORM_L2SQR)
                squared += error
                frame_errors.append(error / source.size)
                maximum = max(maximum, int(cv.absdiff(compared, source).max()))
                elements += source.size
            if pixels:
                frames.append(frame.tolist())
            hashes.append(hashlib.sha256(frame.tobytes()).hexdigest())
            count += 1
        result["decoded_frames"] = count
        result["seek"] = []
        # 고정 OpenCV/FFmpeg는 자기 writer의 단일 프레임 파일도 EOF 뒤 재탐색에 실패한다.
        if count == 1:
            reader.release()
            reader = cv.VideoCapture(str(path))
            result["single_frame_reader_reopened"] = True
        for index in sorted(set((0, count // 2, count - 1))):
            if index < 0:
                continue
            positioned = reader.set(cv.CAP_PROP_POS_FRAMES, index)
            ok, frame = reader.read()
            same = ok and hashlib.sha256(frame.tobytes()).hexdigest() == hashes[index]
            result["seek"].append({"index": index, "positioned": positioned, "decoded": ok, "matches_sequential": same})
            if not same:
                raise RuntimeError("AVI seek differs from sequential decoding")
        if elements:
            mse = squared / elements
            result.update(mse=mse, psnr_db=10.0 * math.log10(255.0**2 / mse) if mse else None, max_channel_error=maximum)
            result["frame_mse"] = frame_errors
        if pixels:
            result["pixels"] = frames
        return result
    finally:
        reader.release()


def source_comparison(path, expected, count):
    # 원본 OpenCV writer는 홀수 크기를 잘라내므로 복제 테두리로 짝수 패딩한 뒤 공통 영역만 비교한다.
    first = expected(0)
    height, width = first.shape[:2]
    path = Path(str(path) + ".source-xvid.avi")
    if path.exists():
        raise RuntimeError("Comparison output already exists")
    padded_width, padded_height = (width + 1) // 2 * 2, (height + 1) // 2 * 2
    writer = cv.VideoWriter(str(path), cv.VideoWriter_fourcc(*"XVID"), 8.0, (padded_width, padded_height))
    if not writer.isOpened():
        raise RuntimeError("Source comparison writer could not open")
    try:
        for index in range(count):
            frame = expected(index)
            writer.write(cv.copyMakeBorder(frame, 0, padded_height - height, 0, padded_width - width, cv.BORDER_REPLICATE))
    finally:
        writer.release()
    return inspect(path, expected, quality_crop=(width, height))


def benchmark(manifest_path):
    manifest_path = Path(manifest_path)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    images = [cv.imread(str(ROOT / path), cv.IMREAD_COLOR) for path in manifest["source_paths"]]
    count = manifest["frame_count"]
    rows = []
    for pattern in ("static", "pan", "cuts"):
        path = manifest_path.parent / ("python-" + pattern + "-xvid.avi")
        if path.exists():
            raise RuntimeError("Comparison output already exists")
        height, width = images[0].shape[:2]
        started = time.perf_counter()
        writer = cv.VideoWriter(str(path), cv.VideoWriter_fourcc(*"XVID"), 8.0, (width, height))
        elapsed = time.perf_counter() - started
        if not writer.isOpened():
            raise RuntimeError("Source XVID writer could not open")
        backend = writer.getBackendName()
        digest = hashlib.sha256()
        times = []
        try:
            for index in range(count):
                frame = frame_at(images, pattern, index)
                digest.update(frame.tobytes())
                started = time.perf_counter()
                writer.write(frame)
                seconds = time.perf_counter() - started
                elapsed += seconds
                times.append(seconds * 1000)
        finally:
            started = time.perf_counter()
            writer.release()
            elapsed += time.perf_counter() - started
        input_hash = digest.hexdigest()
        times.sort()
        expected = lambda index: frame_at(images, pattern, index)
        row = {
            "path": str(path),
            "pattern": pattern,
            "codec": "XVID",
            "writer_backend": backend,
            "bytes": path.stat().st_size,
            "encode_io_ms": elapsed * 1000,
            "write_p50_ms": times[count // 2],
            "write_p95_ms": times[min(count * 95 // 100, count - 1)],
            "input_sha256": input_hash,
            "decode": inspect(path, expected),
        }
        rows.append(row)
        for clip in manifest["clips"]:
            if clip["pattern"] != pattern:
                continue
            if clip["input_sha256"] != input_hash:
                raise RuntimeError("Rust/Python benchmark input hashes differ")
            result = dict(clip)
            result.update(
                codec=clip.get("codec", "MJPG"), ratio_to_xvid=clip["bytes"] / row["bytes"], decode=inspect(clip["path"], expected)
            )
            if result["decode"]["decoded_frames"] != count:
                raise RuntimeError("Decoded AVI frame count differs")
            rows.append(result)
    return {"frames": count, "width": width, "height": height, "fps": 8, "seconds": count / 8, "rows": rows}


request = json.load(sys.stdin)
if "benchmark" in request:
    result = benchmark(request["benchmark"])
else:
    expected_frames = request.get("expected_frames")
    expected = None
    if expected_frames:
        expected = lambda index: np.array(expected_frames[index]["bgr"], dtype=np.uint8).reshape(
            expected_frames[index]["height"], expected_frames[index]["width"], 3
        )
    result = [inspect(path, expected=expected, pixels=request.get("pixels", False)) for path in request["paths"]]
    if request.get("compare_source"):
        for path, row in zip(request["paths"], result):
            row["source_xvid"] = source_comparison(path, expected, len(expected_frames))
json.dump(result, sys.stdout, allow_nan=False)
