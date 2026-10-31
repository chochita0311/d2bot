"""개발용 장면·거리·생사 계약 비교. 화면이나 입력 장치에 접근하지 않는다."""
import json
import sys
from dataclasses import asdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.common.survival import ObservationStamp
from diablo2.common.encounters import SceneContext, VisualCandidate, CombatRadius, Proximity, LifeWitness, proximity, life_state


def context(raw):
    stamp = ObservationStamp(**raw["stamp"])
    return SceneContext(stamp, raw["region_id"], raw["movement_generation"], tuple(raw["frame_size"]), tuple(raw["viewport"]))


def witness(raw):
    if raw is None:
        return None
    return LifeWitness(
        context(raw["context"]), raw["target_id"], raw["kind"], raw["state"], raw["provenance"], raw.get("linked_alive_sequence")
    )


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if sys.version.split()[0] != baseline["python"]:
        raise RuntimeError("encounter oracle Python baseline changed")
    result = []
    for case in json.load(sys.stdin):
        current = context(case["current"])
        if case["kind"] == "proximity":
            raw = case["candidate"]
            candidate = VisualCandidate(
                context(raw["context"]),
                raw["asset_id"],
                raw["kind"],
                raw["category"],
                raw["appearance"],
                raw["score"],
                tuple(raw["box"]),
                None if raw["ground"] is None else tuple(raw["ground"]),
            )
            previous = case.get("previous")
            previous_context = case.get("previous_context")
            value = proximity(
                candidate,
                current,
                None if case["player"] is None else tuple(case["player"]),
                CombatRadius(*case["radius"]),
                now=case["now"],
                max_age=case["max_age"],
                target_id=case["target_id"],
                previous=None if previous is None else Proximity(**previous),
                previous_context=None if previous_context is None else context(previous_context),
            )
            result.append(asdict(value))
        elif case["kind"] == "life":
            result.append(
                life_state(
                    current,
                    witness(case["witness"]),
                    now=case["now"],
                    max_age=case["max_age"],
                    previous_alive=witness(case.get("previous_alive")),
                    first_dead=witness(case.get("first_dead")),
                )
            )
        else:
            raise ValueError("unknown encounter comparison")
    json.dump(result, sys.stdout, allow_nan=False)


if __name__ == "__main__":
    main()
