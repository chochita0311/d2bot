"""개발용 Python 기준 성능 측정. --capture도 화면만 읽으며 입력하지 않는다."""
import hashlib
import json
import math
import sys
import time
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from diablo2.common.capture import ScreenCapture
from diablo2.common.buffing_vision import WeaponSetObserver
from diablo2.actions.town_loop_vision import TownLoopVision
from diablo2.common.config import CaptureConfig
from diablo2.common.frame_geometry import FrameGeometry
from diablo2.common.scene_motion import BackgroundMotionObserver
from diablo2.common.survival_vision import SurvivalHudObserver
from diablo2.runs.summoner.navigation_execution import load_arcane_step_planner
from diablo2.runs.summoner.camera_ground import CalibratedCameraGroundObserver
from diablo2.runs.summoner.scene_vision import ArcaneHazardObserver, ArcaneHubObserver, ArcaneHoverObserver
from diablo2.runs.summoner.terminal_vision import ArcaneTerminalObserver
from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver

SAMPLES, WARMUPS = 30, 3


def measure(operation):
    for _ in range(WARMUPS):
        operation()
    times = []
    for _ in range(SAMPLES):
        start = time.perf_counter()
        operation()
        times.append((time.perf_counter() - start) * 1000)
    times.sort()
    return dict(samples=SAMPLES, warmups=WARMUPS, p50_ms=times[math.ceil(SAMPLES * 0.5) - 1], p95_ms=times[math.ceil(SAMPLES * 0.95) - 1])


def digest(frame):
    return hashlib.sha256(frame.tobytes()).hexdigest()


def main():
    snapshot, output = Path(sys.argv[1]), Path(sys.argv[2])
    output.mkdir()
    source = ROOT / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation"
    paths = [snapshot, source / "north-step-before.png", source / "north-step-after.png"]
    frames = [cv.imread(str(path), cv.IMREAD_COLOR) for path in paths]
    if any(frame is None or frame.shape != (1140, 1922, 3) for frame in frames):
        raise ValueError("1922x1140 inputs required")
    current = frames[0]
    geometry = FrameGeometry.create(dict(left=0, top=0, width=1922, height=1140))
    hud = SurvivalHudObserver(
        ROOT / "assets/ui/survival", allow_high_orb_fallback=True, allow_partial_orb_fallback=True, observe_mercenary=True
    )
    ground, path = load_arcane_step_planner(automap=True, character_class="sorceress")
    motion = BackgroundMotionObserver(
        minimum_matches=12,
        minimum_inlier_ratio=0.65,
        maximum_error=3,
        scale=0.5,
        landmark_viewport=(120, 180, 1720, 890),
        actor_exclusion=(260, 230, 260, 120),
    )
    floor = np.zeros((142, 240), bool)
    last_motion = [None]
    weapon, town = WeaponSetObserver(), TownLoopVision()

    def panels(frame=current):
        if weapon.observe(frame) is not None:
            return True
        gray = town.normalize(frame)
        return any(town.find(gray, name) is not None for name in ("waypoint_header", "save_exit"))

    camera = CalibratedCameraGroundObserver(ground, panels, persist_reviewed_layout=True)
    reading = hud.observe(current)
    camera.confirm_runtime_viewport(
        current,
        reading,
        True,
        0.0,
        0.0,
        reviewed_layout=ROOT / "assets/ui/layouts/remastered-1922x1140/camera-ground.json",
    )
    hazards, hub = ArcaneHazardObserver(), ArcaneHubObserver()
    hover = ArcaneHoverObserver()
    terminal, objective = ArcaneTerminalObserver(), ArcaneObjectiveObserver()

    def observe_camera():
        camera.prepare_frame(current, 1.0, reading, True)
        return camera.observe(current, captured_at=1.0)

    def observe_path():
        return path.observe(frames[1], sequence=1, generation=0, captured_at=1, viewport=(24, 55, 1900, 900), anchor=(960, 580))

    def observe_motion():
        motion.reset()
        cv.setRNGSeed(42)
        for index, frame in enumerate(frames[1:]):
            view = SimpleNamespace(
                sequence=index + 1,
                captured_at=index * 0.1,
                frame_size=(1922, 1140),
                viewport=(0, 0, 1922, 1140),
                anchor=(960, 580),
                cell_pixels=8,
                floor=floor,
                excluded_pixels=None,
            )
            last_motion[0] = asdict(motion.observe(frame, view))

    def packet():
        cv.cvtColor(geometry.normalize(current), cv.COLOR_BGR2GRAY)
        hud.observe(current)
        observe_path()
        observe_motion()

    stages = dict(
        native_normalize_gray=measure(lambda: cv.cvtColor(geometry.normalize(current), cv.COLOR_BGR2GRAY)),
        preview_scale_0_7=measure(lambda: cv.resize(current, None, fx=0.7, fy=0.7)),
        hud=measure(lambda: hud.observe(current)),
        path=measure(observe_path),
        orb_pair=measure(observe_motion),
        panels=measure(panels),
        camera_ground=measure(observe_camera),
        near_hazards=measure(lambda: hazards.observe(current, (962, 580), 1.0, 1.0, aim=None)),
        hover_route_aim=measure(lambda: hover.observe(current, (1446, 211))),
        near_hazards_route_aim=measure(lambda: hazards.observe(current, (962, 580), 1.0, 1.0, aim=(1446, 211))),
        terminal=measure(lambda: terminal.observe(current)),
        objective=measure(lambda: objective.observe(current)),
        hub=measure(lambda: hub.observe(current)),
        legacy_hub=measure(lambda: town.find(town.normalize(current), "world_arcane_waypoint")),
        stored_packet=measure(packet),
    )
    video = output / "static.avi"
    writer = cv.VideoWriter(str(video), cv.VideoWriter_fourcc(*"XVID"), 8, (1922, 1140))
    if not writer.isOpened():
        raise RuntimeError("source XVID writer unavailable")
    try:
        stages["encode_write"] = measure(lambda: writer.write(current))
    finally:
        start = time.perf_counter()
        writer.release()
        finish_ms = (time.perf_counter() - start) * 1000
    native = None
    if "--capture" in sys.argv[3:]:
        settings = CaptureConfig(
            window_title="Diablo II: Resurrected", window_title_mode="exact", capture_backend="screen", reference_layout=None
        )
        capture = ScreenCapture(settings)
        try:
            native = measure(capture.grab)
            native["target"] = capture.target
        finally:
            capture.close()
    result = dict(
        implementation="Python " + sys.version.split()[0] + "/OpenCV " + cv.__version__,
        opencv_threads=cv.getNumThreads(),
        inputs=[dict(path=str(path), bgr_sha256=digest(frame)) for path, frame in zip(paths, frames)],
        width=1922,
        height=1140,
        samples=SAMPLES,
        warmups=WARMUPS,
        percentile="nearest-rank",
        stages=stages,
        capture_screen=native,
        gray_sha256=digest(cv.cvtColor(current, cv.COLOR_BGR2GRAY)),
        hud=asdict(hud.observe(current)),
        path_floor_sha256=digest(observe_path().floor),
        motion=last_motion[0],
        recording=dict(frames=SAMPLES + WARMUPS, fps=8, bytes=video.stat().st_size, finish_ms=finish_ms),
        inputs_sent=0,
    )
    (output / "result.json").write_text(json.dumps(result, ensure_ascii=True, indent=2, allow_nan=False), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=True, allow_nan=False))


if __name__ == "__main__":
    main()
