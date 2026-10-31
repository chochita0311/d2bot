"""실제 preview 루프를 고정 프레임/가짜 효과로 실행한다. 게임에는 접근하지 않는다."""
import json
import logging
import sys
from pathlib import Path
from unittest.mock import patch

import cv2 as cv
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from diablo2.common.config import load_config
from diablo2.core import bot as module


def inspect(request):
    if request["op"] == "scale":
        frame = np.asarray(request["pixels"], dtype=np.uint8).reshape(request["size"][1], request["size"][0], 3)
        output = cv.resize(frame, None, fx=request["scale"], fy=request["scale"])
        return {"size": [output.shape[1], output.shape[0]], "pixels": output.reshape(-1).tolist()}
    config = load_config(Path(request["config"]))
    frames = [cv.imread(path) for path in request["frames"]]
    records, previews, clicks, logs, sleeps = [], [], [], [], []
    state = {"index": 0, "time": 0.0, "closed": False}

    class Capture:
        def __init__(self, settings):
            self.target = {"left": 100, "top": -200, "width": frames[0].shape[1], "height": frames[0].shape[0]}
            self.settings = settings

        def grab(self):
            from types import SimpleNamespace

            index = state["index"]
            state["index"] += 1
            controller.paused = request["paused"][min(index, len(frames) - 1)]
            return SimpleNamespace(frame=frames[index].copy())

    class Controller:
        def __init__(self, dry_run):
            self.paused, self.stop_requested = False, False

        def bind_hotkeys(self, pause, stop):
            pass

        def click(self, x, y):
            clicks.append([x, y])

        def request_stop(self):
            self.stop_requested = True

    class Recorder:
        def __init__(self, settings, size):
            state["record_size"] = list(size)

        def start(self):
            pass

        def write(self, frame):
            records.append(frame.reshape(-1).tolist())
            if len(records) == len(frames) - 1:
                controller.stop_requested = True

        def close(self):
            state["closed"] = True

    class Handler(logging.Handler):
        def emit(self, record):
            logs.append(record.getMessage())

    def sleep(seconds):
        sleeps.append(seconds)
        state["time"] += seconds

    logger = logging.getLogger("diablo2.bot")
    logger.setLevel(logging.INFO)
    handler = Handler()
    logger.addHandler(handler)
    try:
        with patch.object(module, "ScreenCapture", Capture), patch.object(module, "BotController", Controller), patch.object(
            module, "SessionRecorder", Recorder
        ), patch.object(module, "frame_to_desktop", lambda capture, point: (point[0] + 100, point[1] - 200)), patch.object(
            module.cv, "imshow", lambda title, frame: previews.append(frame.reshape(-1).tolist())
        ), patch.object(
            module.cv, "waitKey", return_value=-1
        ), patch.object(
            module.cv, "destroyAllWindows"
        ), patch.object(
            module.time, "time", lambda: state["time"]
        ), patch.object(
            module.time, "sleep", sleep
        ):
            bot = module.DiabloBot(config)
            controller = bot.controller
            code = bot.run()
    finally:
        logger.removeHandler(handler)
    return {
        "code": code,
        "records": records,
        "previews": previews,
        "clicks": clicks,
        "logs": logs,
        "sleeps": sleeps,
        "closed": state["closed"],
        "record_size": state["record_size"],
    }


if __name__ == "__main__":
    assert cv.__version__ == "4.13.0"
    print(json.dumps(inspect(json.load(sys.stdin)), ensure_ascii=False))
