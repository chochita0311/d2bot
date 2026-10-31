"""가짜 엔진과 고정 경로로 GUI 세션 및 공개 실행 설명을 비교한다."""
import json
import sys
import threading
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.common.config import BotConfig, _build_farm_profile
from diablo2.runs.summoner.gui_session import SummonerGuiSession
from diablo2.runs.summoner.summoner_run import resolve_summoner_run, summarize_summoner_payload


def paths(value):
    if isinstance(value, dict):
        return {key: item.replace("\\", "/") if key == "output" else paths(item) for key, item in value.items()}
    if isinstance(value, list):
        return [paths(item) for item in value]
    return value


def exercise(case):
    if case.get("metadata", False):
        config = BotConfig(
            run_profiles={} if case.get("missing") else {"summoner": _build_farm_profile("summoner", case.get("profile", {}))}
        )
        try:
            context = resolve_summoner_run(config)
            definition = asdict(context.definition)
            definition["metadata"].update(
                organizer="src/runs/summoner/main_run.rs",
                gui_adapter="src/runs/summoner/gui_session.rs",
                route_owner="src/runs/summoner/continuous_navigation.rs",
            )
            return dict(
                definition=definition,
                summary=summarize_summoner_payload(context),
                identity=context.profile is config.run_profiles["summoner"],
            )
        except Exception as error:
            return dict(error=str(error))
    session = SummonerGuiSession(SimpleNamespace(characters={"selected": object(), "flash": object()}))
    entered, release = threading.Event(), threading.Event()
    calls, during = [], {}

    def runner(config, **options):
        calls.append({key: str(value) if key == "output" else value for key, value in options.items() if key not in ("emit", "stop_event")})
        if case.get("blocked"):
            entered.set()
            if not release.wait(2):
                raise RuntimeError("fixture release timeout")
        if case.get("log"):
            options["emit"]("observed hub")
        if case.get("cancel"):
            options["stop_event"].set()
        if case.get("error"):
            raise RuntimeError("arrival missing")
        return case.get("code", 0)

    options = dict(
        repeat_count=case.get("count"), difficulty=case.get("difficulty", "hell"), character_key=case.get("character", "selected")
    )
    with patch("diablo2.runs.summoner.main_run.run", side_effect=runner), patch(
        "diablo2.runs.summoner.gui_session.datetime"
    ) as clock, patch("diablo2.runs.summoner.gui_session.uuid4", return_value=SimpleNamespace(hex="0123456789")):
        clock.now.return_value.strftime.return_value = "20000101-000000"
        try:
            output = session.start(**options)
            if case.get("blocked"):
                if not entered.wait(2):
                    raise RuntimeError("fixture entry timeout")
                session.close()
                during["running"] = session.is_running
                for name, operation in (
                    ("restart", lambda: session.start(**options)),
                    ("update", lambda: session.update_config(session.config)),
                ):
                    try:
                        operation()
                    except Exception as error:
                        during[name] = str(error)
                release.set()
            session._thread.join(2)
            if session.is_running:
                raise RuntimeError("fixture join timeout")
            result = dict(output=str(output), result=session.result, running=session.is_running, calls=calls, during=during)
        except Exception as error:
            release.set()
            result = dict(error=str(error), running=session.is_running)
    events = []
    while not session.events.empty():
        events.append(asdict(session.events.get_nowait()))
    result["events"] = events
    return paths(result)


if __name__ == "__main__":
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("summoner session baseline changed")
    print(json.dumps([exercise(case) for case in json.load(sys.stdin)], allow_nan=False))
