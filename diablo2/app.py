from __future__ import annotations

import argparse
import logging
import sys
from queue import Empty
from pathlib import Path

from diablo2.common.capture import list_windows
from diablo2.core.bot import DiabloBot
from diablo2.common.config import load_config
from diablo2.ui.gui import run_gui
from diablo2.actions.run_lifecycle import RunLifecycleSession


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Diablo 2 Windows capture bot scaffold")
    parser.add_argument(
        "--list-windows",
        action="store_true",
        help="List visible window titles and exit",
    )
    parser.add_argument(
        "--config",
        default="config",
        help="Path to a bot config JSON file or config directory",
    )
    parser.add_argument(
        "--cli",
        action="store_true",
        help="Run the existing OpenCV preview loop instead of the desktop GUI",
    )
    supervised = parser.add_mutually_exclusive_group()
    supervised.add_argument("--town-loop", type=int, choices=(1, 2, 3), help="Live bounded town round trip, 1-3 rooms; F10 stops")
    supervised.add_argument("--arcane-entry", action="store_true", help="Live room creation and Arcane hub entry only; F10 stops")
    supervised.add_argument(
        "--summoner-run", action="store_true", help="Integrated room entry, automatic buffs, and current-screen north run"
    )
    parser.add_argument("--live", action="store_true", help="Enable game input for --summoner-run (default dry)")
    resume = parser.add_mutually_exclusive_group()
    resume.add_argument(
        "--resume-arcane", action="store_true", help="Development Arcane resume flag; automatic runs reject unknown ALT state"
    )
    resume.add_argument("--resume-town", action="store_true", help="Resume --summoner-run from a positively observed Act 1/2 town")
    parser.add_argument("--north-casts", type=int, choices=range(1, 13), default=3, help="Discrete north integration bound (1-12)")
    parser.add_argument(
        "--north-segments", type=int, choices=range(1, 5), default=1, help="Retain route history across 1-4 development segments"
    )
    parser.add_argument("--continuous-north", action="store_true", help="Wide held-F2 navigation and nearby F4 Nova development run")
    parser.add_argument(
        "--all-wings", action="store_true", help="Continue north/east/south/west only after each observed ending and central return"
    )
    parser.add_argument(
        "--north-cycles", type=int, choices=range(1, 6), help="Test 1-5 new-room north runs through central waypoint, Act 1, and save/exit"
    )
    parser.add_argument("--no-combat", action="store_true", help="Keep held-F2 navigation active without the nearby-monster Nova module")
    parser.add_argument(
        "--automap",
        choices=("on", "preserve"),
        default="on",
        help="Summoner map ON by default; preserve only for explicit comparison tests",
    )
    parser.add_argument(
        "--continue-navigation", type=Path, help="Restore local bend from a development report only after current scene correspondence"
    )
    parser.add_argument("--field-seconds", type=int, choices=range(1, 61), default=20, help="Continuous segment duration, 1-60 seconds")
    parser.add_argument(
        "--wing", choices=("north", "east", "south", "west"), default="north", help="Outbound wing from verified central waypoint"
    )
    parser.add_argument(
        "--return-to-hub-from",
        choices=("north", "east", "south", "west"),
        help="Development return from a wing to the current-room hub; unknown ALT state is refused",
    )
    parser.add_argument("--output", type=Path, help="New local evidence directory for --summoner-run")
    parser.add_argument(
        "--difficulty", choices=("normal", "nightmare", "hell"), default="hell", help="Room difficulty for town and Summoner runs"
    )
    parser.add_argument(
        "--town-destination",
        choices=("town", "arcane"),
        default="town",
        help="Arcane uses the reviewed shared arrival calibration",
    )
    return parser


def configure_logging(level: str) -> None:
    logging.basicConfig(
        level=getattr(logging, level.upper(), logging.INFO),
        format="%(asctime)s | %(levelname)s | %(name)s | %(message)s",
    )


def _safe_print(text: str) -> None:
    encoding = sys.stdout.encoding or "utf-8"
    sys.stdout.buffer.write(text.encode(encoding, errors="backslashreplace") + b"\n")


def main() -> int:
    args = build_parser().parse_args()
    if args.list_windows:
        for window in list_windows():
            _safe_print(f"{window.title} | left={window.left} top={window.top} " f"width={window.width} height={window.height}")
        return 0

    if args.town_loop is not None or args.arcane_entry:
        config = load_config(Path(args.config))
        config.capture.window_title = "Diablo II: Resurrected"
        config.capture.window_title_mode = "exact"
        config.capture.capture_backend = "window"
        session = RunLifecycleSession(config.capture)
        session.start(
            1 if args.arcane_entry else args.town_loop,
            args.difficulty,
            town_loop=True,
            destination="arcane" if args.arcane_entry else args.town_destination,
            arrival_only=args.arcane_entry,
        )
        try:
            while session.is_running or not session.events.empty():
                try:
                    event = session.events.get(timeout=0.2)
                except Empty:
                    continue
                _safe_print(f"{event.level}: {event.message}")
        except KeyboardInterrupt:
            session.stop()
        return 0 if session.last_result == "passed" else 2

    if args.summoner_run:
        from datetime import datetime
        from diablo2.runs.summoner.main_run import run

        return run(
            load_config(Path(args.config)),
            live=args.live,
            resume_arcane=args.resume_arcane or args.return_to_hub_from is not None,
            resume_town=args.resume_town,
            casts=args.north_casts,
            segments=args.north_segments,
            continuous=args.continuous_north
            or args.all_wings
            or args.north_cycles is not None
            or args.return_to_hub_from is not None
            or args.no_combat
            or args.wing != "north",
            all_wings=args.all_wings,
            north_cycles=args.north_cycles,
            wing=args.wing,
            return_from=args.return_to_hub_from,
            engage_monsters=not args.no_combat,
            automap=args.automap,
            continue_navigation=args.continue_navigation,
            field_seconds=args.field_seconds,
            difficulty=args.difficulty,
            output=args.output or Path("recordings/summoner/evidence") / (datetime.now().strftime("%Y%m%d-%H%M%S") + "-main"),
            emit=lambda message: print(message, flush=True),
        )

    if not args.cli:
        return run_gui(args.config)

    config_path = Path(args.config)
    config = load_config(config_path)
    configure_logging(config.log_level)
    bot = DiabloBot(config)
    return bot.run()
