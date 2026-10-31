"""Pure reading contracts only; no observer or image processing is run."""

import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.common.survival import CharacterSurvivalPolicy, ObservationStamp
from diablo2.common.survival_vision import HudReading, ResourceReading, SlotReading


def evaluate(request):
    if request["op"] == "text":
        reading = ResourceReading.from_text(request["text"], 1)
        return dict(asdict(reading), ratio=reading.ratio)
    if request["op"] == "bound":
        reading = ResourceReading(lower_bound=request["bound"], reason="orb_bound")
        return dict(asdict(reading), ratio=reading.ratio)
    reading = HudReading(
        "layout",
        "revision",
        ResourceReading.from_text("1580/1600", 1),
        ResourceReading(),
        request["status"],
        tuple(SlotReading(**slot) for slot in request["slots"]),
        "expanded",
    )
    return dict(
        contents=reading.belt_contents(CharacterSurvivalPolicy.from_dict(request["policy"])),
        safety=asdict(reading.safety_observation(ObservationStamp("another", "room", 1, 10))),
    )


if __name__ == "__main__":
    json.dump([evaluate(request) for request in json.load(sys.stdin)], sys.stdout, allow_nan=False)
