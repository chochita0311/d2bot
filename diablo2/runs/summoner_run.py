from __future__ import annotations

from dataclasses import dataclass

from diablo2.common.config import BotConfig, FarmProfile

SUMMONER_PROFILE_ID = "summoner"


@dataclass
class SummonerRunContext:
    profile_id: str
    profile: FarmProfile


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
