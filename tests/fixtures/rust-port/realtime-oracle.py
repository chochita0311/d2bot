"""Development-only generation/admission oracle using metadata and fake time."""

import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from diablo2.common.realtime import DecisionSnapshot, RealtimeRuntimeState, RuntimeFrame, VisionSnapshot


def outcome(call):
    try:
        return {"value": call()}
    except Exception as error:
        return {"error_type": type(error).__name__, "error_message": str(error)}


def summary(snapshot):
    frame = snapshot.latest_frame
    latest = (
        None
        if frame is None
        else dict(
            sequence_id=frame.sequence_id,
            captured_at=frame.captured_at,
            target=frame.target,
            generation=frame.generation,
            wall_captured_at=frame.wall_captured_at,
        )
    )
    return dict(
        sampled_at=snapshot.sampled_at,
        latest=latest,
        recent=[item.sequence_id for item in snapshot.recent_frames],
        fast=None if snapshot.fast_vision is None else asdict(snapshot.fast_vision),
        slow=None if snapshot.slow_vision is None else asdict(snapshot.slow_vision),
        decision=None if snapshot.latest_decision is None else asdict(snapshot.latest_decision),
        generation=snapshot.generation,
    )


def evaluate(request):
    clock = [0]
    state = RealtimeRuntimeState(clock=lambda: clock[0])
    saved = []
    results = []
    for operation in request:
        clock[0] = operation["now"]

        def run():
            mode = operation["op"]
            if mode == "frame":
                return state.publish_frame(
                    RuntimeFrame(
                        operation["sequence"],
                        None,
                        operation["captured_at"],
                        operation.get("target", dict(left=0, top=0, width=1, height=1)),
                        operation.get("generation", state.generation),
                        operation.get("wall"),
                    )
                )
            if mode == "vision":
                channel = operation.get("channel", "fast")
                return state.publish_vision(
                    channel,
                    VisionSnapshot(
                        operation.get("snapshot_channel", channel),
                        operation.get("payload"),
                        operation["sequence"],
                        operation["captured_at"],
                        operation["produced_at"],
                        operation.get("generation", state.generation),
                    ),
                )
            if mode == "decision":
                return state.publish_decision(
                    DecisionSnapshot(
                        operation.get("payload"),
                        operation["produced_at"],
                        operation.get("sequence"),
                        operation.get("captured_at"),
                        operation.get("generation", state.generation),
                    )
                )
            if mode == "invalidate":
                return state.invalidate()
            if mode == "save":
                saved.append(state.snapshot())
                return None
            if mode == "current":
                return state.is_current_generation(saved[operation["index"]])
            return None

        results.append(dict(outcome(run), snapshot=outcome(lambda: summary(state.snapshot()))))
    return results


if __name__ == "__main__":
    json.dump([evaluate(request) for request in json.load(sys.stdin)], sys.stdout, allow_nan=False)
