"""실제 Tk 메서드를 메모리 위젯에 적용해 표시 상태를 비교한다."""
import json
import sys
import unicodedata
from pathlib import Path
from queue import Queue
from types import SimpleNamespace
from unittest.mock import Mock

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path[:0] = [str(ROOT), str(ROOT / "tests")]
from diablo2.common.config import BotConfig, CaptureConfig, GuiConfig, _build_character_profile
from diablo2.runs.summoner.gui_session import SummonerEvent
from diablo2.ui.gui import D2BotControlPanel
from test_gui_summoner import Value


class Widget:
    def __init__(self, enabled=True):
        self.enabled = enabled
        self.values = []

    def state(self, values):
        self.enabled = values != ["disabled"]

    def configure(self, **values):
        if "values" in values:
            self.values = list(values["values"])
        if "state" in values:
            self.enabled = values["state"] != "disabled"


class Text:
    def __init__(self):
        self.text = ""

    def configure(self, **values):
        pass

    def insert(self, index, text):
        self.text = self.text + text if index == "end" else text + self.text

    def index(self, index):
        return str(self.text.count("\n") + 1) + ".0"

    def delete(self, start, end):
        self.text = "\n".join(self.text.split("\n")[int(end.split(".")[0]) - 1 :])

    def see(self, index):
        pass


def exercise(case):
    if case.get("digits"):
        return [[chr(code), D2BotControlPanel._validate_repeat_count(None, chr(code))] for code in range(0x110000) if chr(code).isnumeric()]
    raw = case.get("config", {})
    panel = D2BotControlPanel.__new__(D2BotControlPanel)
    panel.config = BotConfig(
        capture=CaptureConfig(**raw.get("capture", {})),
        gui=GuiConfig(**raw.get("gui", {})),
        characters={key: _build_character_profile(key, value) for key, value in raw.get("characters", {}).items()},
    )
    panel.root = Mock()
    panel._closing, panel._recording_cleanup = False, None
    panel._preserve_current_geometry = Mock()
    panel._visible_log_trim_notice = "Older GUI messages were trimmed from this live view."
    panel.log_text = Text()
    for name, value in (
        ("window_title", panel.config.capture.window_title or "Diablo"),
        ("backend", panel.config.capture.capture_backend),
        ("character", ""),
        ("repeat_count", ""),
        ("difficulty", "hell"),
        ("town_loop", False),
        ("status", "Idle"),
        ("output", "No recording yet"),
        ("window_list", []),
    ):
        setattr(panel, name + "_var", Value(value))
    for name in ("record", "lifecycle", "loot", "key", "gem"):
        setattr(panel, name + "_button", Widget())
    for name in ("stop", "stop_action"):
        setattr(panel, name + "_button", Widget(False))
    panel.character_box, panel.window_title_box, panel.backend_box = Widget(), Widget(), Widget()
    for name in ("recording", "lifecycle", "gem", "loot", "summoner"):
        setattr(panel, name + "_session", SimpleNamespace(is_running=False, events=Queue(), request_stop=Mock(), stop=Mock()))
    panel._refresh_character_options()
    rows = []
    for op in case.get("ops", []):
        result = {}
        for key, value in op.get("set", {}).items():
            getattr(panel, key + "_var").set(value)
        for name, running in op.get("activity", {}).items():
            if name == "recording_cleanup":
                panel._recording_cleanup = SimpleNamespace(is_alive=lambda: running) if running else None
            else:
                getattr(panel, name + "_session").is_running = running
        kind = op.get("op", "refresh")
        if kind == "windows":
            from unittest.mock import patch

            with patch("diablo2.ui.gui.list_windows", return_value=[SimpleNamespace(title=title) for title in op["titles"]]):
                panel._refresh_windows(op.get("select_current", False))
        elif kind == "repeat":
            result["valid"] = panel._validate_repeat_count(panel.repeat_count_var.get())
            try:
                result["repeat"] = panel._parse_repeat_count()
            except Exception as error:
                result["error"] = str(error)
        elif kind == "ensure":
            try:
                panel._ensure_action_available()
            except Exception as error:
                result["error"] = str(error)
        elif kind == "log":
            panel._append_log(op.get("level", "info"), op["message"])
        elif kind == "stop":
            panel.stop_current_action()
        elif kind == "close":
            from unittest.mock import patch

            with patch("diablo2.ui.gui.threading.Thread", return_value=SimpleNamespace(start=lambda: None, is_alive=lambda: True)):
                panel._on_close()
        elif kind == "record_started":
            panel.output_var.set(op["output"])
            if not panel._actions_running():
                panel.status_var.set("Recording")
            panel.record_button.state(["disabled"])
            panel.stop_button.state(["!disabled"])
        elif kind == "record_stopped":
            panel.status_var.set("Idle")
            panel.record_button.state(["!disabled"])
            panel.stop_button.state(["disabled"])
        elif kind == "poll":
            for name, events in op.get("events", {}).items():
                for event in events:
                    getattr(panel, name + "_session").events.put(SummonerEvent(**event))
            panel._poll_events()
        if kind == "refresh":
            panel._refresh_action_controls()
        cleanup = panel._recording_cleanup is not None and panel._recording_cleanup.is_alive()
        rows.append(
            dict(
                result,
                state=dict(
                    window_title=panel.window_title_var.get(),
                    character=panel.character_var.get(),
                    characters=panel.character_box.values,
                    selected=panel._character_display_to_key.get(panel.character_var.get().strip()),
                    status=panel.status_var.get(),
                    output=panel.output_var.get(),
                    closing=panel._closing,
                    can_close=panel._closing and not panel._actions_running() and not panel.recording_session.is_running and not cleanup,
                    controls=dict(
                        actions=panel.key_button.enabled,
                        stop_action=panel.stop_action_button.enabled,
                        selectors=panel.character_box.enabled,
                        record=panel.record_button.enabled,
                        stop_record=panel.stop_button.enabled,
                    ),
                    log=panel.log_text.text,
                ),
            )
        )
    return rows


if __name__ == "__main__":
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__, unicodedata.unidata_version) != (
        baseline["python"],
        baseline["numpy"],
        baseline["opencv"],
        "11.0.0",
    ):
        raise RuntimeError("panel baseline changed")
    print(json.dumps([exercise(case) for case in json.load(sys.stdin)], allow_nan=False))
