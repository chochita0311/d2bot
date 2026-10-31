"""Development-only field decisions and fake-key trace; no OS input."""

import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.common.casting import CastingCatalog, CharacterCastingProfile
from diablo2.common.field_control import (
    AdaptiveTeleportCadence,
    DryFieldInput,
    FieldControlLoop,
    FieldLimits,
    FieldObservation,
    HeldFieldInput,
)
from diablo2.common.survival import CharacterSurvivalPolicy, CharacterSurvivalState, ObservationStamp, SafetyObservation


def stamp(raw):
    return ObservationStamp(**raw)


def belt(raw):
    return None if raw is None else {int(key): value for key, value in raw.items()}


def evaluate(request):
    state = CharacterSurvivalState("flash", "room", CharacterSurvivalPolicy.from_dict(request["policy"]))
    initial = ObservationStamp("flash", "room", 0, 0)
    state.observe_belt(initial, belt(request["initial_belt"]))
    state.confirm_buff(initial, "battle_orders", "field", True)
    backend = DryFieldInput()
    inputs = HeldFieldInput(backend, "a", "d")
    estimate = CastingCatalog.from_list(request["catalog"]).estimate(
        "resurrection", "sorceress", CharacterCastingProfile.from_dict(request["casting"])
    )
    loop = FieldControlLoop(
        state,
        inputs,
        FieldLimits(*request.get("limits", [1, 1000, 2, 1, 1])),
        AdaptiveTeleportCadence(estimate, request.get("limits", [1, 1000, 2, 1, 1])[3]),
    )
    results = []
    for operation in request["operations"]:
        mode = operation["op"]
        if mode == "tick":
            result = asdict(loop.tick(operation["now"]))
        elif mode == "pause":
            result = loop.pause(operation["paused"])
        elif mode == "stop":
            result = loop.stop()
        elif mode == "ack":
            result = loop.acknowledge_potion(stamp(operation["stamp"]), operation["column"])
        elif mode == "belt":
            result = loop.reconcile_belt(stamp(operation["stamp"]), belt(operation["contents"]))
        else:
            enemy = operation.get("enemy", "far")
            safety = SafetyObservation(
                stamp(operation["stamp"]),
                operation.get("life", 1),
                operation.get("mana", 1),
                operation.get("location", "field"),
                operation.get("monsters_clear", enemy == "clear"),
            )
            scene = FieldObservation(
                safety,
                operation.get("belt_closed", True),
                enemy,
                None if operation.get("aim") is None else tuple(operation["aim"]),
                operation.get("reachable", True),
                operation.get("arrived"),
                operation.get("loot_checked", False),
                None if operation.get("hunt") is None else tuple(operation["hunt"]),
            )
            result = asdict(loop.observe(scene, operation["now"], focused=operation.get("focused", True)))
        results.append(
            dict(
                value=result,
                phase=loop.phase,
                pending=loop.potion_pending is not None,
                counts=state.belt_counts,
                snapshot=inputs.snapshot(),
                events=list(backend.events),
            )
        )
    return results


if __name__ == "__main__":
    json.dump([evaluate(request) for request in json.load(sys.stdin)], sys.stdout, allow_nan=False)
