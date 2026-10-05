from __future__ import annotations

import argparse
import json
from dataclasses import asdict
from pathlib import Path

from diablo2.common.config import load_config
from diablo2.common.field_control import FieldLimits, FieldObservation
from diablo2.common.field_runtime import create_field_control
from diablo2.common.survival import ObservationStamp, SafetyObservation, _number


def replay(config, character_id, room_id, scenario):
    if scenario.get("evidence") != "synthetic":
        raise ValueError("this replay consumes synthetic semantic events, not live visual evidence")
    limits = FieldLimits(**scenario["limits"])
    events = scenario["events"]
    if not isinstance(events, list) or not 1 <= len(events) <= 1000:
        raise ValueError("bounded event list required")
    loop = create_field_control(config, character_id, room_id, limits)
    results, previous_at = [], -1.0
    try:
        for event in events:
            now = _number(event["at"], "event time")
            if now < previous_at:
                raise ValueError("replay time cannot move backwards")
            previous_at = now
            kind = event["type"]
            stamp = ObservationStamp(character_id, room_id, event["sequence"], now) if kind != "tick" else None
            if kind == "belt":
                contents = {int(column): tuple(value) for column, value in event["contents"].items()}
                if not loop.state.observe_belt(stamp, contents):
                    raise ValueError("belt event not accepted")
                result = {"action": "confirmed_belt"}
            elif kind == "buff":
                if not loop.state.confirm_buff(stamp, event["name"], event["location"], event["monsters_clear"]):
                    raise ValueError("buff event not accepted")
                result = {"action": "synthetic_confirmed_buff"}
            elif kind == "potion_consumed":
                if not loop.acknowledge_potion(stamp, event["column"]):
                    raise ValueError("consumption event not accepted")
                result = {"action": "confirmed_consumption"}
            elif kind == "observation":
                raw = dict(event["field"])
                safety = SafetyObservation(stamp, **raw.pop("safety"))
                for key in ("travel_aim", "hunt_aim"):
                    if raw.get(key) is not None:
                        raw[key] = tuple(raw[key])
                result = asdict(loop.observe(FieldObservation(safety, **raw), now))
            elif kind == "tick":
                result = asdict(loop.tick(now))
            else:
                raise ValueError("unsupported replay event")
            results.append(dict(result, sequence=event.get("sequence"), at=now))
    finally:
        loop.stop()
    return {"mode": "synthetic_replay", "inputs_sent": 0, "results": results, "dry_input_events": list(loop.inputs.backend.events)}


def main(argv=None):
    parser = argparse.ArgumentParser(description="Bounded input-free replay of admitted field-control semantics")
    parser.add_argument("--config", type=Path, default=Path("config"))
    parser.add_argument("--character", required=True)
    parser.add_argument("--room", required=True)
    parser.add_argument("--scenario", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.scenario.stat().st_size > 1024 * 1024:
        parser.error("scenario must be at most 1 MiB")
    scenario = json.loads(args.scenario.read_text(encoding="utf-8"))
    print(json.dumps(replay(load_config(args.config), args.character, args.room, scenario), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
