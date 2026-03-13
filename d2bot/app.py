from __future__ import annotations

import argparse
import logging
from pathlib import Path

from d2bot.bot import DiabloBot
from d2bot.config import load_config


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Diablo 2 Windows capture bot scaffold")
    parser.add_argument(
        "--config",
        default="config.example.json",
        help="Path to bot config JSON file",
    )
    return parser


def configure_logging(level: str) -> None:
    logging.basicConfig(
        level=getattr(logging, level.upper(), logging.INFO),
        format="%(asctime)s | %(levelname)s | %(name)s | %(message)s",
    )


def main() -> int:
    args = build_parser().parse_args()
    config_path = Path(args.config)
    config = load_config(config_path)
    configure_logging(config.log_level)
    bot = DiabloBot(config)
    return bot.run()
