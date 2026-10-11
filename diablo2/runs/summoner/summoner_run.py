from __future__ import annotations

from dataclasses import dataclass

from diablo2.common.config import BotConfig
from diablo2.runs.base import RunDefinition, RunPayloadState, RunPort, RunRouteSegment
from diablo2.runs.summoner.gui_session import SummonerEvent, SummonerGuiSession
from diablo2.runs.summoner.routes.east_go import ROUTE_SEGMENT as EAST_GO
from diablo2.runs.summoner.routes.east_return import ROUTE_SEGMENT as EAST_RETURN
from diablo2.runs.summoner.routes.south_go import ROUTE_SEGMENT as SOUTH_GO
from diablo2.runs.summoner.routes.south_return import ROUTE_SEGMENT as SOUTH_RETURN
from diablo2.runs.summoner.routes.west_go import ROUTE_SEGMENT as WEST_GO
from diablo2.runs.summoner.routes.west_return import ROUTE_SEGMENT as WEST_RETURN

SUMMONER_PROFILE_ID = "summoner"

# 기존 공개 import는 최신 GUI 세션으로 연결한다.
SummonerRunOrchestrator = SummonerGuiSession


@dataclass
class SummonerRunContext:
    profile_id: str
    definition: RunDefinition

    @property
    def profile(self):
        return self.definition.profile


SUMMONER_PAYLOAD_STATES: tuple[RunPayloadState, ...] = (
    RunPayloadState(
        key="room_created",
        description="The room is created with the character already selected by the user.",
        success_signal="Town is observed and input is available.",
        next_action="Enter the Arcane Sanctuary through the town waypoint.",
    ),
    RunPayloadState(
        key="payload_started",
        description="The current main_run engine observes the hub and prepares the configured character.",
        success_signal="Hub and automap are observed; the user supplies the main-weapon precondition and the balanced buff sequence is dispatched. "
        "Buff effects remain not_independently_verified.",
        next_action="Activate the north route.",
    ),
    RunPayloadState(
        key="route_piece_active",
        description="Continuous north movement yields to observed combat, hazards, and buff renewal.",
        success_signal="Observed route progress remains valid.",
        next_action="Continue until a terminal, encounter, interruption, or navigation limit.",
    ),
    RunPayloadState(
        key="summoner_encountered",
        description="Observed Summoner-platform evidence enters the encounter controller.",
        success_signal="The encounter result is recorded separately from route progress.",
        next_action="Complete the encounter and loot observation or report an incomplete attempt.",
    ),
    RunPayloadState(
        key="payload_complete",
        description="The north attempt is ready to return through the central waypoint.",
        success_signal="The room completion observations are satisfied.",
        next_action="Return to Act 1 and save/exit.",
    ),
    RunPayloadState(
        key="room_exit_complete",
        description="The character selection screen is observed after exiting through Act 1.",
        success_signal="The latest runtime reports north_room_closed.",
        next_action="Start the next requested fresh north attempt, up to five in total.",
    ),
)

SUMMONER_ROUTE_SEGMENTS = (
    RunRouteSegment(
        key="north_go",
        label="north way go",
        direction="go",
        status="ready",
        notes=(
            "Implemented by main_run and continuous_navigation; fresh north attempts are bounded from one to five.",
            "Ready describes the executable route; live acceptance remains separate from code availability.",
        ),
    ),
    RunRouteSegment(
        key="north_return",
        label="north way return",
        direction="return",
        status="ready",
        notes=("The latest main_run returns through the central waypoint to Act 1 and verifies character select.",),
    ),
    EAST_GO,
    EAST_RETURN,
    SOUTH_GO,
    SOUTH_RETURN,
    WEST_GO,
    WEST_RETURN,
)

SUMMONER_SHARED_PORTS: tuple[RunPort, ...] = (
    RunPort("make_room", "wrapper", "Create a fresh room for the selected character and difficulty.", status="ready"),
    RunPort("arcane_entry", "payload", "Observe town travel and Arcane Sanctuary waypoint arrival.", status="ready"),
    RunPort(
        "buff_before_run",
        "payload",
        "Dispatch the balanced initial buff sequence and automatically renew it through the current runtime.",
        status="ready",
        notes=("The main-weapon state is a user precondition; buff effects remain not_independently_verified.",),
    ),
    RunPort(
        "hunting",
        "payload",
        "Handle observed nearby monsters and the Summoner encounter.",
        status="partial",
        notes=("An encounter controller exists; completed encounter and boss-death acceptance remain unverified.",),
    ),
    RunPort(
        "looting",
        "payload",
        "Record encounter loot observations and outcomes.",
        status="partial",
        notes=("Loot observation and conditional pickup are implemented; completed pickup acceptance remains unverified.",),
    ),
    RunPort("escape_plan", "payload", "Yield or leave on observed danger and interruption.", status="ready"),
    RunPort("exit_room", "wrapper", "Return through Act 1 and observe character select.", status="ready"),
    RunPort("continuous_runtime", "runtime", "Run current main_run navigation in a cancellable GUI worker.", status="ready"),
)


def build_summoner_definition(config: BotConfig) -> RunDefinition:
    profile = config.run_profiles.get(SUMMONER_PROFILE_ID)
    if profile is None:
        raise RuntimeError(f"Summoner Run requires the '{SUMMONER_PROFILE_ID}' profile in run_profiles.")
    return RunDefinition(
        profile_id=SUMMONER_PROFILE_ID,
        profile=profile,
        payload_states=SUMMONER_PAYLOAD_STATES,
        route_segments=SUMMONER_ROUTE_SEGMENTS,
        shared_ports=SUMMONER_SHARED_PORTS,
        metadata={
            "organizer": "runs/summoner/main_run.py",
            "gui_adapter": "runs/summoner/gui_session.py",
            "route_owner": "runs/summoner/continuous_navigation.py",
            "current_stage": "fresh room -> Arcane hub -> automatic north attempt -> Act 1 -> character select",
        },
    )


def resolve_summoner_run(config: BotConfig) -> SummonerRunContext:
    return SummonerRunContext(profile_id=SUMMONER_PROFILE_ID, definition=build_summoner_definition(config))


def build_summoner_orchestrator(config: BotConfig) -> SummonerRunOrchestrator:
    return SummonerRunOrchestrator(config)


def iter_summoner_payload_states() -> tuple[RunPayloadState, ...]:
    return SUMMONER_PAYLOAD_STATES


def summarize_summoner_payload(context: SummonerRunContext) -> list[str]:
    return context.definition.summarize()
