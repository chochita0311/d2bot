import unittest
from unittest.mock import patch

from diablo2.common.config import BotConfig, FarmProfile
from diablo2.runs import SummonerRunOrchestrator, build_summoner_orchestrator
from diablo2.runs.summoner import SummonerGuiSession, resolve_summoner_run


class SummonerSessionCompatibilityTests(unittest.TestCase):
    def test_public_orchestrator_constructs_latest_adapter_without_loading_capture_assets(self):
        config = BotConfig()
        with patch("diablo2.common.capture.ScreenCapture", side_effect=AssertionError("no capture at construction")):
            session = build_summoner_orchestrator(config)
        self.assertIs(SummonerRunOrchestrator, SummonerGuiSession)
        self.assertIsInstance(session, SummonerGuiSession)
        self.assertFalse(session.is_running)

    def test_metadata_preserves_route_keys_and_points_to_latest_engine(self):
        config = BotConfig(run_profiles={"summoner": FarmProfile()})
        context = resolve_summoner_run(config)
        self.assertIs(context.profile, config.run_profiles["summoner"])
        self.assertEqual("runs/summoner/main_run.py", context.definition.metadata["organizer"])
        self.assertEqual(
            ["north_go", "north_return", "east_go", "east_return", "south_go", "south_return", "west_go", "west_return"],
            [segment.key for segment in context.definition.route_segments],
        )

    def test_missing_profile_is_reported_by_metadata_resolver(self):
        with self.assertRaisesRegex(RuntimeError, "requires the 'summoner' profile"):
            resolve_summoner_run(BotConfig())


if __name__ == "__main__":
    unittest.main()
