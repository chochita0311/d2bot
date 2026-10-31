"""메인 실행기의 실제 분기·부분 보고서를 의존 함수 경계에서 재생한다."""
import json
import sys
import tempfile
import threading
from contextlib import ExitStack
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch
import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.runs.summoner import main_run
from diablo2.common.config import load_config


def exercise(case):
    config = load_config(ROOT / "config")
    now, driver_index, nav_index = [10.0], [0], [0]
    stopped, focused_state = threading.Event(), [True]
    events, emits = [], []
    recovery_rows = [None]

    def recovery(*args, **kwargs):
        recovery_rows[0] = kwargs["records"]
        return Mock()

    capture = Mock(target=dict(left=0, top=0, width=1922, height=1140))
    capture.current_target.side_effect = lambda: capture.target
    capture.grab.return_value = SimpleNamespace(frame=SimpleNamespace(index=900))
    capture.close.side_effect = lambda: events.append(["close"])
    loot = SimpleNamespace(
        prepare=lambda **kwargs: dict(status="test_labels_off"),
        records=[],
        labels_state="off",
        last_hud_failure=None,
        last_landing_observation=None,
    )
    camera = Mock(calibrations=0, projections=0, center=(962, 578), authorized=True, reset_events=[])
    records = case.get("buffs", [{}])

    def execute():
        i = driver_index[0]
        driver_index[0] += 1
        events.append(["buff", i, now[0]])
        row = records[min(i, len(records) - 1)]
        now[0] += row.get("delay", 0.2)
        if row.get("error"):
            raise RuntimeError(row["error"])
        deadline = row.get("deadline", 100.0 + i * 20)
        return dict(
            observation=dict(source="runtime_screen", iteration=i),
            safety=dict(iteration=i),
            buff=dict(status="dispatched_unconfirmed"),
            deadlines=[deadline + 5],
            preparation_at=deadline,
        )

    driver = SimpleNamespace(execute=execute)

    def entry(session, difficulty, emit, **kwargs):
        events.append(["entry", difficulty, kwargs.get("resume_town", False)])
        now[0] += case.get("entry_delay", 0.05)
        if case.get("entry_error"):
            raise RuntimeError(case["entry_error"])

    def navigate(*args, **kwargs):
        i = nav_index[0]
        nav_index[0] += 1
        if i == 0 and recovery_rows[0] is not None:
            recovery_rows[0].extend(case.get("pending_recoveries", []))
        # 실제 호출 위치에 따라 함수명을 별도 wrapper에서 지정한다.
        is_continuous = kwargs.pop("_continuous")
        progress = kwargs["progress"]
        events.append(
            [
                "navigate",
                is_continuous,
                i,
                kwargs.get("wing", "north"),
                kwargs.get("return_from"),
                kwargs.get("continue_current_field", False),
                kwargs.get("transition_from"),
                kwargs.get("buff_expires_at"),
                json.loads(json.dumps(progress)),
            ]
        )
        rows = case.get("navigation", [dict(status="north_segment_observed"), dict(status="hub_arrived")])
        row = rows[min(i, len(rows) - 1)]
        now[0] += row.get("delay", 0.1)
        for name in row.get("evidence", []):
            kwargs["evidence"].append((name, SimpleNamespace(index=i)))
        progress.update(row.get("partial", {}))
        if "append_step" in row:
            progress["steps"].append(row["append_step"])
        if "append_breadcrumb" in row:
            progress["breadcrumbs"].append(row["append_breadcrumb"])
        if row.get("stop"):
            stopped.set()
        if row.get("unfocus"):
            focused_state[0] = False
        if row.get("error"):
            raise RuntimeError(row["error"])
        return dict(row.get("result", dict(status=row.get("status", "north_segment_observed"), **progress)))

    def continuous(*args, **kwargs):
        return navigate(*args, _continuous=True, **kwargs)

    def legacy(*args, **kwargs):
        return navigate(*args, _continuous=False, **kwargs)

    graph_count = [0]

    def graph():
        if graph_count[0]:
            events.append(["reset_graph"])
        graph_count[0] += 1
        return Mock()

    def finish(*args, **kwargs):
        destination = kwargs.get("destination", "arcane")
        events.append(["finish", destination])
        if case.get("finish_error"):
            raise RuntimeError(case["finish_error"])
        return dict(status="act1_then_character_select_observed")

    def encounter(*args, **kwargs):
        events.append(["encounter", kwargs["buff_expires_at"]])
        if case.get("encounter_error"):
            raise RuntimeError(case["encounter_error"])
        return case.get("encounter", dict(kill_confirmed=True, loot_complete=True))

    def canyon(*args, **kwargs):
        events.append(["canyon", kwargs["buff_expires_at"]])
        if case.get("canyon_error"):
            raise RuntimeError(case["canyon_error"])
        return dict(status="canyon_waypoint_observed")

    def pause(*args, **kwargs):
        events.append(["pause", kwargs["buff_expires_at"]])
        kwargs["report"]["paused_survival_guard"] = dict(status="test_monitor")
        if case.get("pause_renew"):
            kwargs["renew_buffs"]()
        if case.get("pause_stop"):
            stopped.set()
        if case.get("pause_error"):
            raise RuntimeError(case["pause_error"])

    def safe_exit(*args, **kwargs):
        events.append(["safe_exit"])
        kwargs["diagnostics"]["attempted"] = True
        if case.get("exit_error"):
            raise RuntimeError(case["exit_error"])

    options = dict(
        live=True, resume_arcane=False, casts=1, difficulty="hell", continuous=True, segments=2, finish_room=True, character_key="flash"
    )
    options.update(case.get("options", {}))
    if case.get("loot_evidence"):
        loot.last_hud_failure = (SimpleNamespace(index=1001), dict(reason="test_hud"))
        loot.last_landing_observation = (SimpleNamespace(index=1002), dict(reason="test_landing"))
    output_root = ROOT / "target/tmp"
    output_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="main-oracle-", dir=str(output_root)) as directory, ExitStack() as stack:
        output = Path(directory) / "room"

        def write(path, text, **kwargs):
            events.append(["report", json.loads(text)])
            return len(text)

        replacements = {
            "diablo2.runs.summoner.main_run.enter_arcane": entry,
            "diablo2.runs.summoner.main_run.load_arcane_step_planner": Mock(return_value=(Mock(), Mock())),
            "diablo2.runs.summoner.main_run.AutomaticBuffDriver": Mock(return_value=driver),
            "diablo2.runs.summoner.main_run.ensure_automap_on": Mock(return_value=dict(status="test_map_on")),
            "diablo2.runs.summoner.main_run.return_to_act1_and_exit": finish,
            "diablo2.runs.summoner.main_run.maintain_paused_field": pause,
            "diablo2.runs.summoner.main_run.exit_to_character": safe_exit,
            "diablo2.runs.summoner.main_run.run_north_segment": legacy,
            "diablo2.runs.summoner.main_run.RecordingInput": Mock(return_value=SimpleNamespace(events=[])),
            "diablo2.common.capture.ScreenCapture": Mock(return_value=capture),
            "diablo2.common.capture.resolve_window_from_config": Mock(return_value=SimpleNamespace(handle=7)),
            "diablo2.common.capture.USER32": SimpleNamespace(
                GetForegroundWindow=lambda: 7 if focused_state[0] else 8, IsIconic=lambda _: False
            ),
            "diablo2.common.controller.BotController": Mock(),
            "diablo2.common.controller.ControllerFieldInput": Mock(),
            "diablo2.common.survival_vision.SurvivalHudObserver": Mock(
                return_value=SimpleNamespace(
                    observe=lambda frame: SimpleNamespace(
                        life=SimpleNamespace(ratio=0.9), mana=SimpleNamespace(ratio=0.9), belt_visibility="closed", mercenary_status=None
                    )
                )
            ),
            "diablo2.common.buffing_vision.WeaponSetObserver": Mock(return_value=SimpleNamespace(observe=lambda frame: None)),
            "diablo2.common.automap_vision.AutomapObserver": Mock(return_value=SimpleNamespace(visible=lambda *args: True)),
            "diablo2.actions.town_loop_vision.TownLoopVision": Mock(
                return_value=SimpleNamespace(normalize=lambda frame: frame, find=lambda *args: None)
            ),
            "diablo2.actions.run_lifecycle.RunLifecycleSession": Mock(),
            "diablo2.common.conditional_loot.ConditionalLootController": Mock(return_value=loot),
            "diablo2.common.emergency_recovery.EmergencyResourceRecovery": recovery,
            "diablo2.runs.summoner.camera_ground.CalibratedCameraGroundObserver": Mock(return_value=camera),
            "diablo2.runs.summoner.continuous_navigation.run_continuous_route": continuous,
            "diablo2.runs.summoner.route_graph.ArcaneRouteGraph": graph,
            "diablo2.runs.summoner.terrain_patterns.ArcaneTerrainPatterns": Mock(),
            "diablo2.runs.summoner.terminal_vision.ArcaneTerminalObserver": Mock(),
            "diablo2.runs.summoner.objective_vision.ArcaneObjectiveObserver": Mock(),
            "diablo2.runs.summoner.encounter_execution.run_platform_encounter": encounter,
            "diablo2.runs.summoner.encounter_execution.SummonerAppearanceObserver": Mock(),
            "diablo2.runs.summoner.life_vision.SummonerLifeTracker": Mock(),
            "diablo2.runs.summoner.canyon_finish.enter_canyon": canyon,
            "keyboard.add_hotkey": Mock(side_effect=lambda key, callback: callback),
            "keyboard.remove_hotkey": Mock(),
        }
        for name, value in replacements.items():
            stack.enter_context(patch(name, value))
        for name in ("ArcaneHazardObserver", "ArcaneHoverObserver", "ArcaneTargetObserver", "ArcaneHubObserver"):
            stack.enter_context(
                patch("diablo2.runs.summoner.scene_vision." + name, Mock(return_value=SimpleNamespace(observe=lambda *args, **kwargs: ())))
            )
        camera.observe.return_value = SimpleNamespace(point=(962, 578), reason="test")
        stack.enter_context(patch.object(main_run.time, "monotonic", lambda: now[0]))
        stack.enter_context(patch.object(main_run.time, "time", lambda: 1000.0 + now[0]))
        stack.enter_context(patch.object(Path, "write_text", write))
        stack.enter_context(patch.object(Path, "replace", lambda *args: None))
        stack.enter_context(patch.object(cv, "imwrite", lambda path, frame: events.append(["image", Path(path).name, frame.index]) or True))
        result, error = None, None
        try:
            result = main_run.run_once(
                config, output=output, stop_event=stopped, emit=lambda text: emits.append(text.replace(str(output), "out")), **options
            )
        except (ValueError, RuntimeError) as caught:
            error = dict(kind=type(caught).__name__, message=str(caught))
    return dict(result=result, error=error, events=events, emits=emits, now=now[0])


def cycles_case(case):
    events, emits, stopped, index = [], [], threading.Event(), [0]
    if case.get("stopped"):
        stopped.set()
    options = dict(live=True, continuous=True, wing="north", automatic=True)
    options.update(case.get("options", {}))
    output_root = ROOT / "target/tmp"
    output_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="main-oracle-", dir=str(output_root)) as directory, ExitStack() as stack:
        output = Path(directory) / "cycles"

        def room(config, **current):
            i = index[0]
            index[0] += 1
            fields = ("resume_arcane", "resume_town", "resume_labels_off", "return_from", "continue_navigation", "finish_room")
            events.append(
                ["room", current["output"].name, {key: current.get(key, False if key.startswith("resume_") else None) for key in fields}]
            )
            row = case.get("rooms", [{}])[min(i, len(case.get("rooms", [{}])) - 1)]
            if row.get("stop"):
                stopped.set()
            if row.get("error"):
                raise RuntimeError(row["error"])
            return row.get("code", 0)

        def read(path, **kwargs):
            events.append(["read", path.parent.name + "/" + path.name])
            row = case.get("rooms", [{}])[min(index[0] - 1, len(case.get("rooms", [{}])) - 1)]
            if row.get("read_error"):
                raise RuntimeError(row["read_error"])
            return json.dumps(row.get("report", dict(status="north_room_closed")))

        stack.enter_context(patch.object(main_run, "run_once", room))
        stack.enter_context(patch.object(Path, "read_text", read))
        stack.enter_context(
            patch.object(Path, "write_text", lambda path, text, **kwargs: events.append(["report", json.loads(text)]) or len(text))
        )
        result, error = None, None
        try:
            result = main_run.run(None, north_cycles=case.get("cycles", 2), output=output, stop_event=stopped, emit=emits.append, **options)
        except (ValueError, RuntimeError, KeyError) as caught:
            error = dict(kind=type(caught).__name__, message=str(caught))
    return dict(result=result, error=error, events=events, emits=emits)


def entry_case(case):
    from queue import Empty

    events, emits, stopped = [], [], threading.Event()
    if case.get("stopped"):
        stopped.set()

    class Queue:
        def __init__(self):
            self.items = list(case.get("events", [dict(level="info", message="arrived")]))

        def empty(self):
            return not self.items

        def get(self, timeout):
            if case.get("event_error"):
                raise RuntimeError(case["event_error"])
            if not self.items:
                raise Empty()
            return SimpleNamespace(**self.items.pop(0))

    class Session:
        def __init__(self):
            self.events, self.index = Queue(), 0
            self.last_result = case.get("last_result", "passed")

        @property
        def is_running(self):
            index = self.index
            self.index += 1
            if case.get("stop_iteration") == index:
                stopped.set()
            return index < case.get("running", 1)

        def start(self, count, difficulty, **options):
            events.append(["start", count, difficulty, options])
            if case.get("start_error"):
                raise RuntimeError(case["start_error"])

        def request_stop(self):
            events.append(["stop"])

    error = None
    try:
        main_run.enter_arcane(
            Session(), case.get("difficulty", "hell"), emits.append, resume_town=case.get("resume_town", False), stop=stopped
        )
    except RuntimeError as caught:
        error = dict(kind=type(caught).__name__, message=str(caught))
    return dict(error=error, events=events, emits=emits)


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("main execution baseline changed")
    request = json.load(sys.stdin)
    methods = dict(cycles=cycles_case, entry=entry_case)
    print(
        json.dumps(
            [methods.get(c.get("operation"), exercise)(c) for c in request["cases"]],
            ensure_ascii=False,
            allow_nan=False,
        )
    )


if __name__ == "__main__":
    main()
