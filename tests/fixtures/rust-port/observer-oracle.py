"""개발용 이미지 관찰 비교. 화면 캡처·포커스 전환·게임 입력을 수행하지 않는다."""
import hashlib
import json
import sys
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.common.automap_vision import AutomapObserver
from diablo2.common.label_occlusion import label_occlusion_mask, visible_template_peak
from diablo2.common.survival_vision import SurvivalHudObserver
from diablo2.common.buffing_vision import WeaponSetObserver


def image(raw, channels=3):
    if "terminal_scene" in raw:
        directory = ROOT / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        manifest = json.loads((directory / "terminal-patterns.json").read_text(encoding="utf-8"))
        frame = np.zeros((1140, 1922, 3), np.uint8)
        for part, (x, y) in zip(manifest["parts"], raw["terminal_scene"]):
            gray = cv.imread(str(directory / part["path"]), cv.IMREAD_GRAYSCALE)
            h, w = gray.shape
            frame[y : y + h, x : x + w] = cv.cvtColor(gray, cv.COLOR_GRAY2BGR)
    elif "target_scene" in raw:
        from diablo2.runs.summoner.scene_vision import ArcaneTargetObserver

        frame = np.zeros((1140, 1922, 3), np.uint8)
        scenario = raw["target_scene"]
        if scenario != "blank":
            frame[76:116, 880:1050] = (0, 0, 67)
        if scenario in ("ghoul", "two_panels"):
            title = ArcaneTargetObserver().titles[0][1]
            frame[83:109, 906:1017] = cv.cvtColor(title, cv.COLOR_GRAY2BGR)
        if scenario == "two_panels":
            frame[76:116, 1100:1230] = (0, 0, 67)
    elif "path" in raw:
        frame = cv.imread(str(ROOT / raw["path"]), cv.IMREAD_COLOR if channels == 3 else cv.IMREAD_GRAYSCALE)
        if frame is None:
            raise ValueError("observer fixture unreadable")
    else:
        shape = (raw["height"], raw["width"]) + ((3,) if channels == 3 else ())
        frame = np.frombuffer(bytes.fromhex(raw["pixels"]), np.uint8).reshape(shape).copy()
    if "resize" in raw:
        frame = cv.resize(frame, tuple(raw["resize"]), interpolation=cv.INTER_AREA)
    for left, top, right, bottom in raw.get("clear", []):
        frame[top:bottom, left:right] = 0
    if "shift" in raw:
        dx, dy = raw["shift"]
        frame = cv.warpAffine(frame, np.float32([[1, 0, dx], [0, 1, dy]]), (frame.shape[1], frame.shape[0]))
    if "keep" in raw:
        retained = np.zeros_like(frame)
        for left, top, right, bottom in raw["keep"]:
            retained[top:bottom, left:right] = frame[top:bottom, left:right]
        frame = retained
    for item in raw.get("copy", []):
        left, top, right, bottom = item["box"]
        x, y = item["at"]
        frame[y : y + bottom - top, x : x + right - left] = frame[top:bottom, left:right].copy()
    return frame


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    actual = (sys.version.split()[0], np.__version__, cv.__version__)
    expected = (baseline["python"], baseline["numpy"], baseline["opencv"])
    source = next(line.split(":", 1)[1].strip() for line in cv.getBuildInformation().splitlines() if "Version control:" in line)
    if actual != expected or source != baseline["opencv_version_control"]:
        raise RuntimeError("observer oracle baseline changed; review motion-baseline.json before comparing")
    request = json.load(sys.stdin)
    result = []
    if request["kind"] == "reduce":
        values = np.frombuffer(bytes.fromhex(request["pixels"]), np.uint8).astype(np.float32)
        result = dict(sum=float(values.sum()), distance=float(values.mean() / 255))
    elif request["kind"] == "grid_samples":
        for case in request["cases"]:
            frame = image(case["frame"])
            neutral = ((frame[:, :, 0] == frame[:, :, 1]) & (frame[:, :, 1] == frame[:, :, 2]) & (frame[:, :, 0] >= 30)).astype(np.float32)
            sampled = cv.resize(frame, tuple(case["output"]), interpolation=cv.INTER_AREA)
            neutral = cv.resize(neutral, tuple(case["output"]), interpolation=cv.INTER_AREA)
            result.append(dict(color=hashlib.sha256(sampled.tobytes()).hexdigest(), neutral=hashlib.sha256(neutral.tobytes()).hexdigest()))
    elif request["kind"] in ("lab_audit", "hsv_audit"):
        frame = np.empty((256, 256, 3), np.uint8)
        frame[:, :, 1] = np.arange(256, dtype=np.uint8)[:, None]
        frame[:, :, 2] = np.arange(256, dtype=np.uint8)[None, :]
        for blue in range(256):
            frame[:, :, 0] = blue
            conversion = cv.COLOR_BGR2LAB if request["kind"] == "lab_audit" else cv.COLOR_BGR2HSV
            result.append(hashlib.sha256(cv.cvtColor(frame, conversion).tobytes()).hexdigest())
    elif request["kind"] == "half":
        for case in request["cases"]:
            frame = image(case["frame"], case["channels"])
            outputs = [cv.resize(frame, None, fx=0.5, fy=0.5, interpolation=mode) for mode in (cv.INTER_LINEAR, cv.INTER_AREA)]
            result.append([dict(size=[a.shape[1], a.shape[0]], sha256=hashlib.sha256(a.tobytes()).hexdigest()) for a in outputs])
    elif request["kind"] == "terrain":
        from diablo2.runs.summoner.terrain_patterns import ArcaneTerrainPatterns

        observer = ArcaneTerrainPatterns()
        for case in request["cases"]:
            result.append([asdict(item) for item in observer.observe(image(case["frame"]))])
    elif request["kind"] == "region":
        from diablo2.common.encounter_vision import RegionObserver
        from diablo2.common.encounters import SceneContext
        from diablo2.common.survival import ObservationStamp

        directory = ROOT / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
        observer = RegionObserver(directory / "manifest.json")
        for case in request["cases"]:
            reference = next(item for item in manifest["cases"] if item["id"] == case["id"])
            frame = cv.imread(str(directory / reference["path"]))
            scene = SceneContext(
                ObservationStamp("character", "room", 10, 10),
                manifest["region_id"],
                0,
                tuple(manifest["frame_size"]),
                tuple(reference["viewport"]),
            )
            hits = observer.scan(frame, scene, now=case.get("now", 10), max_age=1, limit_per_template=3, workers=case.get("workers", 1))
            terrain = [
                asdict(
                    observer.terrain(frame, scene, tuple(box), now=case.get("now", 10), max_age=1, minimum_floor=0.75, maximum_void=0.15)
                )
                for box in case.get("terrain", [])
            ]
            result.append(dict(hits=[asdict(hit) for hit in hits], terrain=terrain))
    elif request["kind"] == "terminal_confirm":
        from diablo2.runs.summoner.terminal_vision import ArcaneTerminalObserver

        observer = ArcaneTerminalObserver()
        reference = observer.observe(image(request["reference"]))
        for case in request["cases"]:
            frame = image(case["frame"])
            if case["method"] == "static":
                reading = observer.confirm_static_parts(frame, reference)
            elif case["method"] == "approach":
                reading = observer.confirm_approach_parts(frame, reference, tuple(case["motion"]))
            else:
                reading = observer.confirm_relocalized_parts(frame, reference)
            result.append(None if reading is None else asdict(reading))
    elif request["kind"] == "terminal":
        from diablo2.runs.summoner.terminal_vision import ArcaneTerminalObserver, TerminalStabilityTracker

        observer = ArcaneTerminalObserver()
        tracker = TerminalStabilityTracker(observer)
        for case in request["cases"]:
            if case.get("reset"):
                tracker.reset()
                result.append(None)
                continue
            if case.get("relocalize"):
                tracker.require_relocalization()
                result.append(None)
                continue
            frame = image(case["frame"])
            reading = observer.observe(frame) if case.get("observe", True) else None
            if case.get("force_part") is not None:
                from dataclasses import replace

                raw = case["force_part"]
                ids, boxes = list(reading.part_ids), list(reading.evidence_boxes)
                index = ids.index(raw["old"])
                ids[index], boxes[index] = raw["new"], tuple(raw["box"])
                reading = observer.confirm_static_parts(frame, replace(reading, part_ids=tuple(ids), evidence_boxes=tuple(boxes)))
                reading = replace(reading, provenance="current_three_parts")
            if request.get("track"):
                tracked = tracker.update(frame, reading, case["at"], None if case.get("motion") is None else tuple(case["motion"]))
                keys = (
                    "reference",
                    "observed_at",
                    "approach",
                    "approach_only",
                    "landmark_relocalized",
                    "previous_current_full",
                    "approach_displacement",
                    "approach_static_pairs",
                )
                status = {key: getattr(tracker, key) for key in keys}
                for key in ("reference", "approach"):
                    if status[key] is not None:
                        status[key] = asdict(status[key])
                if status["previous_current_full"] is not None:
                    before, at = status["previous_current_full"]
                    status["previous_current_full"] = [asdict(before), at]
                result.append(
                    dict(
                        reading=None if reading is None else asdict(reading),
                        tracked=None if tracked is None else asdict(tracked),
                        status=status,
                    )
                )
            else:
                result.append(None if reading is None else asdict(reading))
    elif request["kind"] == "appearance":
        from diablo2.runs.summoner.encounter_execution import SummonerAppearanceObserver
        from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver

        observer = SummonerAppearanceObserver()
        objective = ArcaneObjectiveObserver()
        for case in request["cases"]:
            frame = image(case["frame"])
            platform = objective.observe(frame) if case.get("platform", True) else None
            alive = observer.observe(frame, platform)
            corpse = observer.observe_corpse(frame, platform)
            result.append(
                dict(
                    platform=None if platform is None else asdict(platform),
                    alive=None if alive is None else asdict(alive),
                    corpse=None if corpse is None else asdict(corpse),
                )
            )
    elif request["kind"] == "objective":
        from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver
        from diablo2.runs.summoner.journal_vision import HorazonJournalObserver

        observer = ArcaneObjectiveObserver()
        journal = HorazonJournalObserver()
        for case in request["cases"]:
            frame = image(case["frame"])
            reading = observer.observe(frame)
            result.append(
                dict(
                    reading=None if reading is None else asdict(reading),
                    matches=[asdict(m) for m in journal.matches(frame)],
                    point=journal.observe(frame),
                )
            )
    elif request["kind"] == "hazard":
        from diablo2.runs.summoner.scene_vision import ArcaneHazardObserver

        observer = ArcaneHazardObserver()
        directory = ROOT / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
        for case in request["cases"]:
            frame = np.zeros((1140, 1922, 3), np.uint8)
            for item in case.get("paste", []):
                rule = next(rule for rule in manifest["templates"] if rule["id"] == item["id"])
                template = cv.imread(str(directory / rule["path"]))
                x, y = item["at"]
                height, width = template.shape[:2]
                frame[y : y + height, x : x + width] = template
            hits = observer.observe(
                frame, tuple(case["ground"]), case["at"], case["now"], aim=None if case.get("aim") is None else tuple(case["aim"])
            )
            result.append([asdict(hit) for hit in hits])
    elif request["kind"] == "hub":
        from diablo2.runs.summoner.scene_vision import ArcaneHubObserver, HubStabilityTracker

        observer = ArcaneHubObserver()
        tracker = HubStabilityTracker(observer)
        for case in request["cases"]:
            frame = image(case["frame"])
            point = observer.observe(frame)
            current_evidence = observer.last_evidence
            tracked = None
            if request.get("track"):
                motion = case.get("motion", [0, 0])
                tracked = tracker.update(frame, point, current_evidence, case["at"], None if motion is None else tuple(motion))
            near = observer.current_pair_near(frame, tuple(case.get("player", [962, 578])))
            result.append(dict(point=point, evidence=current_evidence, pairs=observer.last_pair_points, near=near, tracked=tracked))
    elif request["kind"] == "camera":
        import diablo2.runs.summoner.camera_ground as module
        from diablo2.runs.summoner.scene_vision import GroundReading

        state = dict(reading=GroundReading(None, "ground_unverified"), open=False, calls=0)

        def ground(frame, *, captured_at):
            state["calls"] += 1
            return state["reading"]

        observer = module.CalibratedCameraGroundObserver(
            SimpleNamespace(observe=ground), lambda frame: state["open"], **request.get("limits", {})
        )
        frames = {
            "native": np.zeros((1140, 1922, 3), np.uint8),
            "clone": np.zeros((1140, 1922, 3), np.uint8),
            "small": np.zeros((900, 1600, 3), np.uint8),
        }
        for op in request["operations"]:
            state["open"] = op.get("panel", False)
            frame = frames[op.get("frame", "native")]
            hud = SimpleNamespace(
                belt_visibility=op.get("belt", "closed"),
                life=SimpleNamespace(ratio=op.get("life", 1.0)),
                mana=SimpleNamespace(ratio=op.get("mana", 0.8)),
            )
            outcome = None
            try:
                if op["action"] == "confirm":
                    layout = op.get("layout")
                    with patch.object(module, "Path", return_value=SimpleNamespace(read_text=lambda **_: json.dumps(layout))):
                        observer.confirm_closed_viewport(
                            op["proof"], op["now"], reviewed_layout=None if layout is None else "injected-camera.json"
                        )
                elif op["action"] == "runtime_confirm":
                    with patch.object(module, "Path", return_value=SimpleNamespace(read_text=lambda **_: json.dumps(op["layout"]))):
                        observer.confirm_runtime_viewport(
                            frame, hud, op.get("focused", True), op["at"], op["now"], reviewed_layout="injected-camera.json"
                        )
                elif op["action"] == "observe":
                    raw = op.get("reading", dict(point=None, reason="ground_unverified"))
                    state["reading"] = GroundReading(None if raw["point"] is None else tuple(raw["point"]), raw["reason"], raw.get("score"))
                    if op.get("prepare", True):
                        prepared = frames[op.get("prepare_frame", op.get("frame", "native"))]
                        observer.prepare_frame(prepared, op.get("prepare_at", op["at"]), hud, op.get("focused", True))
                    outcome = asdict(observer.observe(frame, captured_at=op["at"]))
                else:
                    raise ValueError("unknown camera operation")
            except (ValueError, RuntimeError) as error:
                outcome = dict(error=str(error))
            keys = (
                "authorized",
                "samples",
                "center",
                "last_strong_at",
                "last_at",
                "viewport_anchor",
                "reviewed_anchor",
                "calibrated_at",
                "projections",
                "calibrations",
                "seed_started",
                "reference_kind",
                "reviewed_projection_active",
                "reset_events",
            )
            status = {key: list(getattr(observer, key)) if key in ("samples", "reset_events") else getattr(observer, key) for key in keys}
            result.append(dict(outcome=outcome, status=status, calls=state["calls"]))
    elif request["kind"] == "ground_blur":
        frame = image(request["frame"])
        result = hashlib.sha256(cv.GaussianBlur(frame, (15, 15), 0).tobytes()).hexdigest()
    elif request["kind"] == "ground":
        from diablo2.runs.summoner.scene_vision import FlashGroundObserver

        marker = image(request["marker"])
        body = None if request.get("body") is None else image(request["body"])
        options = dict(search_box=(800, 480, 1120, 700), anchor_offset=(16, 10), tracking=request.get("tracking"))
        if body is not None:
            options.update(body_templates=(body,), body_box=(-24, -120, 24, -40))
        observer = FlashGroundObserver(marker, **options)
        for case in request["cases"]:
            frame = np.zeros((1140, 1922, 3), np.uint8)
            for item in case.get("points", []):
                x, y = item["point"]
                if item.get("marker", True):
                    frame[y - 10 : y + 10, x - 16 : x + 16] = marker
                if item.get("shield", False):
                    frame[y - 163 : y - 136, x - 13 : x + 14] = (35, 100, 125)
                if item.get("body", False):
                    frame[y - 120 : y - 40, x - 24 : x + 24] = body
                if item.get("shape", False):
                    frame[y - 120 : y - 40, x - 6 : x + 7] = (55, 75, 110)
                if item.get("fog", False):
                    frame[y - 120 : y - 40, x - 48 : x + 48] = (55, 75, 110)
                if item.get("tail", False):
                    frame[y - 150 : y - 149, x + 13 : x + 33] = (35, 100, 125)
            if case.get("layout_bad"):
                frame = frame[:900, :1600]
            result.append(asdict(observer.observe(frame, captured_at=case.get("at"))))
    elif request["kind"] == "canyon":
        from diablo2.runs.summoner.canyon_finish import CanyonApproachObserver, red_portal, journal_landing
        from diablo2.runs.summoner.navigation_execution import load_arcane_step_planner
        from diablo2.runs.summoner.journal_vision import HorazonJournalObserver

        stairs = CanyonApproachObserver()
        ground, path = load_arcane_step_planner()
        journal = HorazonJournalObserver()
        for case in request["cases"]:
            frame = image(case["frame"])
            row = dict(stairs=stairs.observe(frame), portal=red_portal(frame))
            if case.get("landing"):
                player = ground.observe(frame, captured_at=1).point
                point = journal.observe(frame)
                view = path.observe(frame, sequence=1, generation=0, captured_at=1, viewport=(120, 180, 1720, 890), anchor=player)
                row.update(player=player, journal=point, landing=journal_landing(path, view, point, player), teleport=path.teleport)
            result.append(row)
    elif request["kind"] == "target":
        from diablo2.runs.summoner.scene_vision import ArcaneTargetObserver

        observer = ArcaneTargetObserver()
        for case in request["cases"]:
            frame = image(case["frame"])
            found = observer.observe(frame)
            result.append(None if found is None else asdict(found))
    elif request["kind"] == "hover":
        from diablo2.runs.summoner.scene_vision import ArcaneHoverObserver

        observer = ArcaneHoverObserver([(item["kind"], image(item["frame"])) for item in request["templates"]])
        for case in request["cases"]:
            result.append([asdict(item) for item in observer.observe(image(case["frame"]), tuple(case["aim"]))])
    elif request["kind"] == "automap":
        observer = AutomapObserver()
        for case in request["cases"]:
            visible = observer.visible(image(case["frame"]), case["area"])
            result.append(dict(visible=visible, evidence=observer.last_evidence, obscured=observer.last_obscured))
    elif request["kind"] == "label_mask":
        for case in request["cases"]:
            mask = label_occlusion_mask(image(case["frame"]), include_gold=case["include_gold"])
            result.append(dict(sha256=hashlib.sha256(mask.tobytes()).hexdigest(), nonzero=int(np.count_nonzero(mask))))
    elif request["kind"] == "visible_peak":
        for case in request["cases"]:
            score, location, fraction = visible_template_peak(
                image(case["gray"], 1), image(case["template"], 1), image(case["visible"], 1), minimum_visible=case["minimum_visible"]
            )
            result.append(dict(score=score, location=location, visible_fraction=fraction))
    elif request["kind"] == "hud":
        directory = ROOT / "assets/ui/survival"
        manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
        for case in request["cases"]:
            observer = SurvivalHudObserver(**case.get("options", {}))
            raw = case["frame"]
            if "reference" in raw:
                entry = manifest[raw["group"]][raw["reference"]]
                frame = np.zeros((1140, 1922, 3), np.uint8)
                left, top, right, bottom = entry["canvas_box"]
                frame[top:bottom, left:right] = cv.imread(str(directory / entry["path"]))
            else:
                frame = image(raw)
            for change in case.get("fill", []):
                left, top, right, bottom = change["box"]
                frame[top:bottom, left:right] = change["color"]
            for change in case.get("paste", []):
                left, top, right, bottom = change["box"]
                frame[top:bottom, left:right] = cv.imread(str(directory / change["path"]))
            reading = observer.observe(frame)
            result.append(dict(reading=asdict(reading), closed_slots=[asdict(s) for s in observer.closed_belt_slots(frame, reading)]))
    elif request["kind"] == "weapon":
        observer = WeaponSetObserver()
        for case in request["cases"]:
            frame = np.zeros((1140, 1922, 3), np.uint8)
            if case.get("set") is not None:
                template = observer.templates[case["set"]]
                start = 0
                for left, top, right, bottom in observer.manifest["regions"]:
                    frame[top:bottom, left:right] = template[:, start : start + right - left]
                    start += right - left
            for left, top, right, bottom in case.get("clear", []):
                frame[top:bottom, left:right] = 0
            result.append(observer.observe(frame))
    else:
        raise ValueError("unknown observer comparison")
    json.dump(result, sys.stdout, ensure_ascii=False, allow_nan=False)


if __name__ == "__main__":
    main()
