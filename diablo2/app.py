from __future__ import annotations

import argparse
import logging
import sys
from pathlib import Path

from diablo2.common.capture import list_windows
from diablo2.core.bot import DiabloBot
from diablo2.common.config import load_config
from diablo2.ui.gui import run_gui


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
            _safe_print(
                f"{window.title} | left={window.left} top={window.top} "
                f"width={window.width} height={window.height}"
            )
        return 0

    if not args.cli:
        return run_gui(args.config)

    config_path = Path(args.config)
    config = load_config(config_path)
    configure_logging(config.log_level)
    bot = DiabloBot(config)
    return bot.run()
