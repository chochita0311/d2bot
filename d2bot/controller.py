from __future__ import annotations

import logging
import time

try:
    import keyboard
except ImportError:  # pragma: no cover
    keyboard = None

try:
    import pydirectinput
except ImportError:  # pragma: no cover
    pydirectinput = None


class BotController:
    def __init__(self, dry_run: bool):
        self.dry_run = dry_run
        self.log = logging.getLogger("d2bot.controller")
        self.paused = False
        self.stop_requested = False

    def bind_hotkeys(self, pause_key: str, stop_key: str) -> None:
        if keyboard is None:
            self.log.warning("keyboard module not installed; hotkeys disabled.")
            return
        keyboard.add_hotkey(pause_key, self.toggle_pause)
        keyboard.add_hotkey(stop_key, self.request_stop)
        self.log.info("Hotkeys active: pause=%s stop=%s", pause_key, stop_key)

    def toggle_pause(self) -> None:
        self.paused = not self.paused
        state = "paused" if self.paused else "running"
        self.log.info("Bot is now %s.", state)

    def request_stop(self) -> None:
        self.stop_requested = True
        self.log.info("Stop requested.")

    def click(self, x: int, y: int) -> None:
        if self.dry_run or pydirectinput is None:
            self.log.info("Dry-run click at (%s, %s)", x, y)
            return
        pydirectinput.moveTo(x, y)
        time.sleep(0.05)
        pydirectinput.click()
