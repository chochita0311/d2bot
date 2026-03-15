from __future__ import annotations

from dataclasses import dataclass

from diablo2.common.config import BotConfig, FarmProfile

SUMMONER_PROFILE_ID = "summoner"


@dataclass
class SummonerRunContext:
    profile_id: str
    profile: FarmProfile


@dataclass(frozen=True)
class SummonerPayloadState:
    key: str
    description: str
    success_signal: str
    next_action: str


SUMMONER_PAYLOAD_STATES: tuple[SummonerPayloadState, ...] = (
    SummonerPayloadState(
        key="act1_town_loaded",
        description="The room is already created and the character can act in town.",
        success_signal="Rogue Encampment is loaded and input is available.",
        next_action="Enable labels if needed and move toward the waypoint.",
    ),
    SummonerPayloadState(
        key="labels_enabled",
        description="Item labels should remain visible without holding ALT.",
        success_signal="Ground labels stay visible after the ALT tap.",
        next_action="Open the waypoint and route to Act 2 Arcane Sanctuary.",
    ),
    SummonerPayloadState(
        key="waypoint_open",
        description="The waypoint menu is open and ready for travel.",
        success_signal="Act tabs and destination list are available.",
        next_action="Switch to Act 2 and click Arcane Sanctuary.",
    ),
    SummonerPayloadState(
        key="arcane_loaded",
        description="The character has arrived in Arcane Sanctuary.",
        success_signal="Arcane geometry or area label is visible.",
        next_action="Commit to a wing and start the live coordinator loop.",
    ),
    SummonerPayloadState(
        key="wing_search_started",
        description="Search one wing while only killing blockers or survival threats.",
        success_signal="Forward route progress continues toward the Summoner platform.",
        next_action="Keep pathing, combat, and loot watching active until the boss area is found.",
    ),
    SummonerPayloadState(
        key="summoner_detected",
        description="The Summoner platform or boss identity is confirmed.",
        success_signal="Boss name or platform geometry is confidently detected.",
        next_action="Switch combat priority to killing The Summoner safely.",
    ),
    SummonerPayloadState(
        key="summoner_killed",
        description="The target is dead and the run can resolve loot.",
        success_signal="The Summoner is no longer active and post-kill drops are visible.",
        next_action="Run the fixed-item and run-specific loot decision pass.",
    ),
    SummonerPayloadState(
        key="loot_scan_complete",
        description="Approved drops have been picked up or explicitly ignored.",
        success_signal="A keep-or-ignore decision has been made for visible loot.",
        next_action="If the journal is available, click it to unlock the red portal.",
    ),
    SummonerPayloadState(
        key="journal_clicked",
        description="Horazon's Journal has been used and the portal should be available.",
        success_signal="The red portal is visible and reachable.",
        next_action="Enter the portal to reach Canyon of the Magi.",
    ),
    SummonerPayloadState(
        key="canyon_loaded",
        description="The run has left Arcane Sanctuary successfully.",
        success_signal="Canyon of the Magi is loaded after taking the red portal.",
        next_action="Return to town and settle into a clean handoff state for room exit logic.",
    ),
    SummonerPayloadState(
        key="post_run_complete",
        description="The payload is finished and ready for the room-back-half wrapper.",
        success_signal="The character is back in a safe town-ready state with no pending target actions.",
        next_action="Hand off to reusable exit-room logic when the wrapper is added.",
    ),
)


def resolve_summoner_run(config: BotConfig) -> SummonerRunContext:
    profile = config.run_profiles.get(SUMMONER_PROFILE_ID)
    if profile is None:
        raise RuntimeError(
            f"Summoner Run requires the '{SUMMONER_PROFILE_ID}' profile in run_profiles."
        )

    return SummonerRunContext(
        profile_id=SUMMONER_PROFILE_ID,
        profile=profile,
    )


def iter_summoner_payload_states() -> tuple[SummonerPayloadState, ...]:
    return SUMMONER_PAYLOAD_STATES


def summarize_summoner_payload(context: SummonerRunContext) -> list[str]:
    hunting = context.profile.hunting
    target_monsters = ", ".join(hunting.target_monsters) or "(not configured)"
    target_areas = ", ".join(hunting.target_areas) or "(not configured)"
    waypoint = f"Act {hunting.waypoint_act} / {hunting.waypoint_name}" if hunting.waypoint_act and hunting.waypoint_name else "(not configured)"
    return [
        f"goal: {context.profile.goal}",
        f"waypoint: {waypoint}",
        f"targets: {target_monsters}",
        f"areas: {target_areas}",
        f"fight style: {hunting.fight_style}",
        f"search timeout: {hunting.search_timeout_seconds}s",
        "wrapper: room create/exit stays outside this payload",
    ]
