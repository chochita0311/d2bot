from diablo2.runs.base import RunRouteSegment

ROUTE_SEGMENT = RunRouteSegment(
    key="north_return",
    label="north way reset",
    direction="return",
    status="ready",
    notes=(
        "North-go tuning uses this route piece as the repeatable reset step after each attempt.",
        "The first implementation exits the room and returns to character select so the next north-go attempt can start cleanly.",
    ),
)


def run_arcane_north_reset(session, capture, run_number: int) -> None:
    reset_hook = getattr(session, "_run_north_reset", None)
    if reset_hook is None:
        raise RuntimeError("North-go reset is not available on this session.")
    reset_hook(capture, run_number)
