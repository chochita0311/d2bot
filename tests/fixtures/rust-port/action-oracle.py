"""Development-only action comparisons. No capture, focus or input is invoked."""
import json
import hashlib
import sys
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace
from queue import Empty
from unittest.mock import patch
from contextlib import contextmanager, ExitStack
import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.actions.gem_summing import GemSummingSession, MatchResult as GemMatch
from diablo2.actions.loot_pickup import LootPickupSession, LootCandidate
from diablo2.common.config import CaptureConfig
import diablo2.actions.gem_summing as gem_module
import diablo2.actions.loot_pickup as loot_module
import pydirectinput


def image(request):
    if "path" in request:
        result = cv.imread(str(ROOT / request["path"]), cv.IMREAD_COLOR)
        if result is None:
            raise ValueError("action fixture unreadable")
        return result
    return np.frombuffer(bytes.fromhex(request["bgr"]), np.uint8).reshape(request["height"], request["width"], 3)


@contextmanager
def fake_desktop(module):
    state = dict(trace=[], pointer=[150, 250], clock=0.0)

    def sleep(seconds):
        state["clock"] += seconds
        # Zero-interval scheduler yields carry no elapsed time in this trace.
        if seconds:
            state["trace"].append(["sleep", seconds])

    def move(x, y):
        state["pointer"][:] = [x, y]
        state["trace"].append(["move", x, y])

    def send(count, pointer, size):
        event = pointer.contents
        if event.type == 1:
            key = event.ii.ki
            state["trace"].append(["key", {0x1D: "ctrl", 0x2A: "shift", 1: "esc", 15: "tab"}[key.wScan], bool(key.dwFlags & 2)])
        else:
            state["trace"].append(["click", {6: "left", 24: "right"}[event.ii.mi.dwFlags]])
        return count

    fake_time = SimpleNamespace(
        time=lambda: state["clock"],
        monotonic=lambda: state["clock"],
        perf_counter=lambda: state["clock"],
        strftime=lambda fmt: fmt.replace("%Y%m%d-%H%M%S", "20000101-000000"),
        sleep=sleep,
    )
    fake_random = SimpleNamespace(randint=lambda low, high: 0 if low < 0 else low, uniform=lambda low, high: low)
    with ExitStack() as stack:
        for target, name, value in [
            (pydirectinput, "SendInput", send),
            (pydirectinput, "position", lambda: tuple(state["pointer"])),
            (pydirectinput, "failSafeCheck", lambda: None),
            (pydirectinput, "time", fake_time),
            (module, "time", fake_time),
            (module, "random", fake_random),
            (module, "move_desktop_pointer", move),
        ]:
            stack.enter_context(patch.object(target, name, value))
        yield state


def events(session):
    output = []
    while True:
        try:
            output.append(asdict(session.events.get_nowait()))
        except Empty:
            return output


def main():
    request = json.load(sys.stdin)
    kind = request["kind"]
    if kind == "town_evidence":
        import diablo2.actions.run_lifecycle as module
        import diablo2.actions.town_loop_vision as town_module

        session = module.RunLifecycleSession(CaptureConfig())
        session._repeat_count, session._town_loop = 1, True
        capture = SimpleNamespace(target=dict(left=100, top=200, width=1922, height=1140))
        directory = Path(request["directory"])
        window = SimpleNamespace(handle=7, left=100, top=200, width=1922, height=1140)
        stopped = [False]
        frame = np.zeros((1140, 1922, 3), np.uint8)
        with fake_desktop(module) as state:

            def grab():
                state["trace"].append(["capture"])
                if request["scenario"] == "capture_failure":
                    raise RuntimeError("capture failed")
                return SimpleNamespace(frame=frame)

            capture.grab = grab
            capture.close = lambda: state["trace"].append(["close"])
            session._focus_game_window = lambda _: None
            session._bind_hotkey = lambda: state["trace"].append(["bind"])
            session._unbind_hotkey = lambda: state["trace"].append(["unbind"])

            def stop():
                stopped[0] = True

            def wait(seconds):
                module.time.sleep(seconds)
                session.request_stop()

            session._stop_event = SimpleNamespace(is_set=lambda: stopped[0], set=stop, wait=wait)
            with patch.object(module, "ScreenCapture", lambda _: capture), patch.object(
                module, "resolve_window_from_config", lambda _: window
            ), patch.object(
                module, "ctypes", SimpleNamespace(windll=SimpleNamespace(user32=SimpleNamespace(GetForegroundWindow=lambda: 7)))
            ), patch.object(
                module, "Path", lambda _: directory.parent
            ), patch.object(
                town_module, "time", module.time
            ):
                session._run()
        artifacts = {}
        for path in sorted(directory.iterdir()):
            if path.suffix == ".json":
                artifacts[path.name] = json.loads(path.read_text(encoding="utf-8"))
            elif path.suffix == ".jsonl":
                artifacts[path.name] = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
            elif path.suffix == ".png":
                artifacts[path.name] = hashlib.sha256(cv.imread(str(path)).tobytes()).hexdigest()
        result = dict(trace=state["trace"], events=events(session), elapsed=state["clock"], artifacts=artifacts)
    elif kind == "lifecycle_flow":
        import diablo2.actions.run_lifecycle as module

        session = module.RunLifecycleSession(CaptureConfig())
        session._repeat_count = 1
        pixels = lambda start: (np.arange(27, dtype=np.uint16) * 37 + start).astype(np.uint8).reshape(3, 3, 3)
        button, loading, exit_image = (pixels(n) for n in (11, 73, 151))
        session._difficulty_button_templates["hell"] = button
        session._loading_template, session._exit_template = loading, exit_image

        def frame(template=None):
            result = np.zeros((1140, 1922, 3), np.uint8)
            if template is not None:
                result[40:43, 30:33] = template
            return result

        frames = iter([frame(), frame(), frame(button), frame(loading), frame(), frame(exit_image), frame()])
        capture = SimpleNamespace(target=dict(left=100, top=200, width=1922, height=1140))
        with fake_desktop(module) as state:
            calls = [0]

            def grab():
                calls[0] += 1
                state["trace"].append(["capture"])
                return SimpleNamespace(frame=next(frames))

            capture.grab = grab
            capture.close = lambda: state["trace"].append(["close"])
            session._focus_game_window = lambda _: None
            session._bind_hotkey = lambda: state["trace"].append(["bind"])
            session._unbind_hotkey = lambda: state["trace"].append(["unbind"])
            session._stop_event = SimpleNamespace(is_set=lambda: False, wait=lambda seconds: module.time.sleep(seconds))
            vision = SimpleNamespace(
                normalize=lambda frame: frame,
                find=lambda frame, label: dict(x=100, y=100) if label == ("difficulty_header" if calls[0] == 2 else "play") else None,
            )
            with patch.object(module, "ScreenCapture", lambda _: capture), patch(
                "diablo2.actions.town_loop_vision.TownLoopVision", lambda: vision
            ):
                session._run()
        result = dict(trace=state["trace"], events=events(session), elapsed=state["clock"])
    elif kind == "town_camera":
        from diablo2.actions.town_loop_vision import TownLoopVision

        vision = TownLoopVision()
        result = []
        for case in request["cases"]:
            frame = image(case["frame"])
            stable = vision.camera_stable(frame, case["now"])
            result.append(dict(stable=stable, reading=vision.camera_reading))
    elif kind == "town_vision":
        from diablo2.actions.town_loop_vision import TownLoopVision

        vision = TownLoopVision()
        result = []
        for case in request["cases"]:
            frame = image(case["frame"]).copy()
            if "resize" in case:
                frame = cv.resize(frame, tuple(case["resize"]), interpolation=cv.INTER_AREA)
            for x0, y0, x1, y1 in case.get("clear", []):
                frame[y0:y1, x0:x1] = 0
            gray = vision.normalize(frame)
            result.append({name: vision.find(gray, name) for name in case["finds"]})
    elif kind == "town_blue_pair":
        from diablo2.actions.town_loop_vision import TownLoopVision

        vision = TownLoopVision.__new__(TownLoopVision)
        vision.color = image(request["frame"])
        result = vision.find_blue_pair(request["definition"])
    elif kind == "town_pixels":
        frame = image(request["frame"])
        gray = cv.cvtColor(frame, cv.COLOR_BGR2GRAY)
        result = {"gaussian": []}
        for size, sigma in [(3, 0.8), (31, 5)]:
            result["gaussian"].append(hashlib.sha256(cv.GaussianBlur(gray, (size, size), sigma).tobytes()).hexdigest())
        result["width_scales"] = [
            hashlib.sha256(cv.resize(gray, None, fx=scale, fy=1, interpolation=cv.INTER_LINEAR).tobytes()).hexdigest()
            for scale in request["scales"]
        ]
    elif kind == "town_plan":
        from diablo2.actions.town_loop_vision import TownLoopPlanner

        class Sight:
            templates = {name: None for name in request.get("calibrated", [])}
            camera_reading = None

            def normalize(self, frame):
                return frame

            def find(self, frame, name):
                return self.current.get("finds", {}).get(name)

            def camera_stable(self, frame, now):
                return self.current.get("stable", True)

            def reset_camera(self):
                pass

        sight = Sight()
        planner = TownLoopPlanner(sight, **request.get("options", {}))
        planner.restore(request.get("restore", {}))
        result = []
        for observation in request["observations"]:
            sight.current = observation
            planner.restore(observation.get("restore", {}))
            try:
                action = planner.step(None, observation["now"])
                error = None
            except Exception as exc:
                action, error = None, str(exc)
            result.append(dict(action=action, error=error, state=json.loads(json.dumps(planner.data))))
    elif kind == "loot_specs":
        raw = request["config"]
        config = SimpleNamespace(
            shared_loot=SimpleNamespace(fixed_items=[SimpleNamespace(**item) for item in raw["shared_loot"]["fixed_items"]]),
            farm=SimpleNamespace(templates=[SimpleNamespace(**rule) for rule in raw["farm"]["templates"]]),
        )
        session = LootPickupSession(config)
        loaded = []

        def load(path):
            loaded.append(path.as_posix())
            return np.zeros((2, 2, 3), np.uint8)

        session._load_image = load
        candidates = session._build_candidates()
        result = [dict(label=c.label, path=path, threshold=c.threshold) for c, path in zip(candidates, loaded)]
    elif kind == "loot_scan":
        session = LootPickupSession(SimpleNamespace())
        candidates = [LootCandidate(c["label"], image(c), c["threshold"]) for c in request["candidates"]]
        hit = session.scan_frame(image(request["frame"]), candidates)
        result = None if hit is None else asdict(hit)
    elif kind == "gem_pointer":
        session = GemSummingSession.__new__(GemSummingSession)
        session._last_pointer = None
        session._stop_event = SimpleNamespace(is_set=lambda: False)
        capture = SimpleNamespace(target=dict(left=100, top=200, width=640, height=480))
        with fake_desktop(gem_module) as state:
            session._ctrl_shift_click_absolute(capture, 300, 400, button="right")
        result = dict(trace=state["trace"], last_pointer=session._last_pointer, elapsed=state["clock"])
    elif kind == "loot_session":
        template = image(dict(path=request["template_path"]))
        first = np.zeros((480, 640, 3), np.uint8)
        first[30 : 30 + template.shape[0], 20 : 20 + template.shape[1]] = template
        blank = np.zeros_like(first)
        session = LootPickupSession(SimpleNamespace(capture=CaptureConfig()))
        session._build_candidates = lambda: [LootCandidate("fixture", template, 0.85)]
        capture = SimpleNamespace(target=dict(left=100, top=200, width=640, height=480))
        calls = [0]
        with fake_desktop(loot_module) as state:

            def grab():
                state["trace"].append(["capture"])
                calls[0] += 1
                return SimpleNamespace(frame=first if calls[0] == 1 else blank)

            capture.grab = grab
            session._focus_game_window = lambda _: None
            session._bind_hotkey = lambda: state["trace"].append(["bind"])
            session._unbind_hotkey = lambda: state["trace"].append(["unbind"])
            with patch.object(loot_module, "ScreenCapture", lambda _: capture):
                session._run()
        result = dict(trace=state["trace"], events=events(session), elapsed=state["clock"])
    elif kind == "gem_session":
        session = GemSummingSession(CaptureConfig())
        counts = [[0] * 7 for _ in range(5)]
        counts[0][0] = 10
        occupied_calls = [0]
        capture = SimpleNamespace(target=dict(left=100, top=200, width=700, height=900))
        frame = np.zeros((900, 700, 3), np.uint8)
        session._locate_stash_panel = lambda _: GemMatch((20, 30), 576, 757, 1.0)
        session._locate_best_template = lambda *args: GemMatch((500, 600), 40, 20, 1.0)
        session._read_frame_counts = lambda *args: (
            [row[:] for row in counts],
            [[1.0] * 7 for _ in range(5)],
            [[1.0] * 7 for _ in range(5)],
        )
        with fake_desktop(gem_module) as state:

            def grab():
                state["trace"].append(["capture"])
                return SimpleNamespace(frame=frame)

            capture.grab = grab

            def occupied(capture, match):
                capture.grab()
                occupied_calls[0] += 1
                if occupied_calls[0] != 2:
                    return []
                return [(359, 648), (408, 648)] if request["scenario"] == "bad_cells" else [(359, 648)]

            session._detect_occupied_cube_cells = occupied
            session._cube_result_icon_crop = lambda capture, *args: (capture.grab(), np.zeros((1, 1, 3), np.uint8))[1]
            session._is_expected_result = lambda *args: request["scenario"] != "bad_icon"
            session._focus_game_window = lambda _: None
            session._bind_hotkey = lambda: state["trace"].append(["bind"])
            session._unbind_hotkey = lambda: state["trace"].append(["unbind"])
            with patch.object(gem_module, "ScreenCapture", lambda _: capture):
                session._run()
        result = dict(trace=state["trace"], events=events(session), elapsed=state["clock"])
    elif kind.startswith("gem_"):
        session = GemSummingSession(CaptureConfig())
        if kind == "gem_atlas":
            result = []
            for variant, _ in session.PREPROCESS_VARIANTS:

                def bank(source):
                    return [
                        dict(digit=digit, hashes=[hashlib.sha256(glyph.tobytes()).hexdigest() for glyph in glyphs])
                        for digit, glyphs in source[variant].items()
                    ]

                result.append(
                    dict(
                        variant=variant,
                        ones=bank(session._ones_templates),
                        tens=bank(session._tens_templates),
                        blank=[hashlib.sha256(glyph.tobytes()).hexdigest() for glyph in session._blank_left_templates[variant]],
                    )
                )
        elif kind == "gem_counts":
            result = [dict(zip(("value", "confidence"), session._read_count_value(image(item)))) for item in request["crops"]]
        elif kind == "gem_policy":
            session._blocked_slots = {tuple(slot) for slot in request["blocked"]}
            result = [asdict(step) for step in session._build_plan(request["counts"])]
        elif kind == "gem_consensus":
            samples = request["samples"]
            cursor = iter(range(3))
            capture = SimpleNamespace(grab=lambda: SimpleNamespace(frame=next(cursor)))
            session._read_frame_counts = lambda frame, match: (
                samples[frame]["counts"],
                samples[frame]["confidences"],
                samples[frame]["icon_scores"],
            )
            session._sleep_range = lambda *args: None
            counts = session._read_consensus_counts(capture, None, request.get("fallback"))
            result = dict(counts=counts, events=events(session))
        elif kind == "gem_panel":
            frame = image(request["frame"])
            match = session._locate_stash_panel(frame)
            result = None if match is None else asdict(match)
        else:
            raise ValueError("unsupported gem oracle kind")
    else:
        raise ValueError("unsupported action oracle kind")
    json.dump(result, sys.stdout, ensure_ascii=True, allow_nan=False)


if __name__ == "__main__":
    main()
