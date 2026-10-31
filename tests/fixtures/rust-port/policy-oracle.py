"""Development-only policy/state oracle; never sends input or captures a screen."""

import json
import sys
from dataclasses import asdict
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.common.casting import CastingCatalog, CharacterCastingProfile
from diablo2.common.buffing import AutomaticBuffRenewal, BuffConfirmation, BuffRenewalLimits, BuffSequence
from diablo2.common.survival import CharacterSurvivalPolicy, CharacterSurvivalState, ObservationStamp, SafetyObservation


def outcome(call):
    try:
        return {"value": call()}
    except Exception as error:
        return {"error_type": type(error).__name__, "error_message": str(error)}


def stamp(raw):
    return ObservationStamp(**raw)


def survival(request):
    policy = request.get("policy")
    state = CharacterSurvivalState(
        request.get("character_id", "flash"),
        request.get("room_id", "room"),
        None if policy is None else CharacterSurvivalPolicy.from_dict(policy),
    )
    results = []
    for operation in request["operations"]:
        mode = operation["op"]

        def run():
            if mode == "belt":
                contents = operation["contents"]
                if isinstance(contents, dict):
                    contents = {int(key): value for key, value in contents.items()}
                return state.observe_belt(stamp(operation["stamp"]), contents)
            if mode == "delta":
                return state.confirm_resource_change(stamp(operation["stamp"]), operation["column"], operation["kind"], operation["delta"])
            if mode == "invalidate":
                return state.invalidate_belt(stamp(operation["stamp"]))
            if mode == "buff":
                return state.confirm_buff(
                    stamp(operation["stamp"]), operation["name"], operation.get("location", "field"), operation.get("monsters_clear", True)
                )
            if mode == "pickup":
                return state.potion_pickup_kinds(operation["now"], max_belt_age_seconds=operation["belt_age"])
            if mode == "decide":
                observation = SafetyObservation(
                    stamp(operation["stamp"]),
                    operation.get("life", 1),
                    operation.get("mana", 1),
                    operation.get("location", "field"),
                    operation.get("monsters_clear", True),
                )
                return asdict(
                    state.decide(
                        observation,
                        operation["now"],
                        max_observation_age_seconds=operation.get("screen_age", 1),
                        max_belt_age_seconds=operation.get("belt_age", 1000),
                    )
                )
            raise ValueError("unknown oracle operation")

        results.append(dict(outcome(run), counts=state.belt_counts, potions=state.potions_remaining, renewal=state.next_buff_renewal_at))
    return results


def evaluate(request):
    if request.get("mode") == "casting":
        catalog = CastingCatalog.from_list(request["catalog"])
        return [
            outcome(
                lambda query=query: asdict(
                    catalog.estimate(
                        query["family"],
                        query.get("class"),
                        None if query.get("profile") is None else CharacterCastingProfile.from_dict(query["profile"]),
                    )
                )
            )
            for query in request["queries"]
        ]
    if request.get("mode") == "buff_sequence":
        return buff_sequence(request)
    if request.get("mode") == "buff_runtime":
        return buff_runtime(request)
    return survival(request)


def buff_sequence(request):
    state = CharacterSurvivalState("flash", "room", CharacterSurvivalPolicy.from_dict(request["policy"]))
    state.observe_belt(stamp(request["initial_stamp"]), {int(key): value for key, value in request["initial_belt"].items()})
    sequence = BuffSequence(state, request["order"])
    results = []
    for operation in request["operations"]:

        def run():
            mode = operation["op"]
            if mode == "cancel":
                return sequence.cancel()
            if mode == "ack":
                return sequence.acknowledge_step(operation["index"], stamp(operation["stamp"]), operation.get("succeeded", True))
            observation = SafetyObservation(
                stamp(operation["stamp"]),
                operation.get("life", 1),
                operation.get("mana", 1),
                operation.get("location", "field"),
                operation.get("monsters_clear", True),
            )
            ages = dict(screen_age=operation.get("screen_age", 1), belt_age=operation.get("belt_age", 1000))
            if mode == "next":
                return asdict(sequence.next_step(observation, operation["now"], **ages))
            evidence = BuffConfirmation(
                stamp(operation.get("evidence_stamp", operation["stamp"])),
                operation.get("source", "validated_vision"),
                operation.get("effect_verified", True),
                operation.get("equipment_restored", True),
                operation.get("sequence_effects_verified", True),
            )
            return sequence.confirm_effect(operation.get("name", "battle_orders"), evidence, observation, operation["now"], **ages)

        results.append(dict(outcome(run), phase=sequence.phase, index=sequence.index, renewal=state.next_buff_renewal_at))
    return results


def buff_runtime(request):
    state = CharacterSurvivalState("flash", "room", CharacterSurvivalPolicy.from_dict(request["policy"]))
    state.observe_belt(stamp(request["initial_stamp"]), {int(key): value for key, value in request["initial_belt"].items()})
    if request.get("confirmed_at") is not None:
        state.confirm_buff(ObservationStamp("flash", "room", 0, request["confirmed_at"]), "battle_orders", "field", True)
    actions = SimpleNamespace(pre_run_buff_order=request["order"], buff_action_pause_seconds=request.get("pauses", {}))
    current = dict(now=0, sequence=0, interrupted=False, offset=0)
    sent = []

    def send(token):
        sent.append([token, current["now"]])
        current["now"] += request.get("dispatch_delay", 0)
        return request.get("dispatch_succeeds", True)

    def next_stamp():
        return ObservationStamp("flash", "room", current["sequence"], current["now"] + current["offset"])

    renewal = AutomaticBuffRenewal(
        state,
        actions,
        BuffRenewalLimits(*request.get("limits", [10, 2, 3])),
        screen_age=request.get("screen_age", 1),
        belt_age=1000,
        next_stamp=next_stamp,
        clock=lambda: current["now"],
    )
    results = []
    for operation in request["operations"]:
        current.update(
            now=operation["now"],
            sequence=operation.get("ack_sequence", operation.get("stamp", {}).get("sequence", 0) + 1),
            interrupted=operation.get("interrupted", False),
            offset=operation.get("ack_offset", 0),
        )

        def run():
            if operation["op"] == "tick":
                result = renewal.tick(operation["now"])
            elif operation["op"] == "interrupt":
                result = renewal.interrupt(operation["reason"])
            else:
                safety = SafetyObservation(
                    stamp(operation["stamp"]),
                    operation.get("life", 1),
                    operation.get("mana", 1),
                    operation.get("location", "field"),
                    operation.get("monsters_clear", True),
                )
                evidence = None
                if operation.get("confirm"):
                    evidence = BuffConfirmation(safety.stamp, "validated_vision", True, True, True)
                result = renewal.observe(
                    SimpleNamespace(safety=safety, belt_closed=operation.get("belt_closed", True)),
                    operation["now"],
                    send_token=send,
                    interrupted=lambda: current["interrupted"],
                    confirmation=evidence,
                )
            return None if result is None else asdict(result)

        results.append(
            dict(
                outcome(run),
                pending=renewal.pending,
                review=renewal.review_reason,
                phase=None if renewal.sequence is None else renewal.sequence.phase,
                next_step=renewal.next_step_at,
                renewal=state.next_buff_renewal_at,
                sent=list(sent),
            )
        )
    return results


if __name__ == "__main__":
    requests = json.load(sys.stdin)
    json.dump([outcome(lambda request=request: evaluate(request)) for request in requests], sys.stdout, ensure_ascii=False, allow_nan=False)
