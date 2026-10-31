"""네이티브 효과 없이 기존 방 순환 계획기의 전체 상태를 비교한다."""
import json
import sys
from copy import deepcopy
from dataclasses import asdict
from pathlib import Path

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.common.survival import ObservationStamp
from diablo2.runs.summoner.cycle import COMMANDS, CycleLimits, CycleObservation, SummonerCycle


def state(cycle):
    return deepcopy(
        dict(
            room_id=cycle.room_id,
            room_index=cycle.room_index,
            room_started_at=cycle.room_started_at,
            last_now=cycle._last_now,
            pending=None if cycle.pending is None else asdict(cycle.pending),
            results=[asdict(result) for result in cycle.results],
            checked_wings=cycle.checked_wings,
            boss_wing=cycle.boss_wing,
            keys_dropped=cycle.keys_dropped,
            keys_acquired=cycle.keys_acquired,
            done=cycle.done,
            command_id=cycle._command_id,
            last_sequence=cycle._last_sequence,
            last_observed_at=cycle._last_observed_at,
            generation=cycle._generation,
            wing_index=cycle._wing_index,
            loot_complete=cycle._loot_complete,
            room_entered=cycle._room_entered,
            terminal_reason=cycle._terminal_reason,
        )
    )


def exercise(case):
    try:
        limits = dict(screen_age=1, room_seconds=100, total_seconds=500, stage_seconds={kind: 5 for kind in COMMANDS})
        limits.update(case.get("limits", {}))
        rooms = case.get("room_ids", ["room-" + str(index) for index in range(case.get("rooms", 1))])
        cycle = SummonerCycle(case.get("character", "flash"), tuple(rooms), CycleLimits(**limits), started_at=10)
    except Exception as error:
        return dict(error=dict(kind=type(error).__name__, message=str(error)))
    now, sequence, rows = 10.0, -1, []
    for op in case["ops"]:
        now = op.get("now", now + op.get("dt", 0.1))
        try:
            observation = None
            if not op.get("none", False):
                sequence = op.get("sequence", sequence + 1)
                observation = CycleObservation(
                    ObservationStamp(op.get("character", "flash"), op.get("room", cycle.room_id), sequence, op.get("captured", now)),
                    op.get("generation", 0),
                    op.get("event", "unknown"),
                    op.get("command_id", None if cycle.pending is None else cycle.pending.command_id),
                    op.get("keys_dropped", 0),
                    op.get("keys_acquired", 0),
                    op.get("reason", ""),
                )
            result = dict(directive=asdict(cycle.step(observation, now, focused=op.get("focused", True), stopped=op.get("stopped", False))))
        except Exception as error:
            result = dict(error=dict(kind=type(error).__name__, message=str(error)))
        rows.append(dict(result, state=state(cycle)))
    return rows


if __name__ == "__main__":
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("cycle baseline changed")
    print(json.dumps([exercise(case) for case in json.load(sys.stdin)], allow_nan=False))
