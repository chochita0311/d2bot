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
        self.log = logging.getLogger("diablo2.controller")
        self.paused = False
        self.stop_requested = False
        self.field_control = None

    def attach_field_control(self, loop) -> None:
        if self.field_control is not None:
            self.field_control.stop()
        self.field_control = loop
        if self.stop_requested:
            loop.stop()
        elif self.paused:
            loop.pause(True)

    def bind_hotkeys(self, pause_key: str, stop_key: str) -> None:
        if keyboard is None:
            self.log.warning("keyboard module not installed; hotkeys disabled.")
            return
        keyboard.add_hotkey(pause_key, self.toggle_pause)
        keyboard.add_hotkey(stop_key, self.request_stop)
        self.log.info("Hotkeys active: pause=%s stop=%s", pause_key, stop_key)

    def toggle_pause(self) -> None:
        self.paused = not self.paused
        if self.field_control is not None:
            self.field_control.pause(self.paused)
        state = "paused" if self.paused else "running"
        self.log.info("Bot is now %s.", state)

    def request_stop(self) -> None:
        self.stop_requested = True
        if self.field_control is not None:
            self.field_control.stop()
        self.log.info("Stop requested.")

    def click(self, x: int, y: int) -> None:
        if self.dry_run or pydirectinput is None:
            self.log.info("Dry-run click at (%s, %s)", x, y)
            return
        pydirectinput.moveTo(x, y)
        time.sleep(0.05)
        pydirectinput.click()


class ControllerFieldInput:
    """현재 포커스/창 경계를 확인하며 정리 입력은 중단 후에도 허용한다."""

    def __init__(self, controller: BotController, *, focused, bounds):
        self.controller, self.focused, self.bounds = controller, focused, bounds

    def _guard(self):
        if self.controller.stop_requested or self.controller.paused or not self.focused():
            raise RuntimeError("field input interrupted or game unfocused")
        if not self.controller.dry_run and pydirectinput is None:
            raise RuntimeError("field input backend unavailable")

    def key_down(self, key):
        self._guard()
        if self.controller.dry_run:
            self.controller.log.info("Dry-run key down: %s", key)
        else:
            if pydirectinput.keyDown(key, _pause=False) is not True:
                raise RuntimeError("field key down was not dispatched")

    def key_up(self, key):
        if self.controller.dry_run:
            self.controller.log.info("Dry-run key up: %s", key)
        elif pydirectinput is None:
            raise RuntimeError("field release backend unavailable")
        else:
            if pydirectinput.keyUp(key, _pause=False) is not True:
                raise RuntimeError("field key release was not dispatched")

    def press(self, key):
        self._guard()
        if self.controller.dry_run:
            self.controller.log.info("Dry-run press: %s", key)
        else:
            try:
                if pydirectinput.keyDown(key, _pause=False) is not True:
                    raise RuntimeError("field press was not dispatched")
                time.sleep(0.02)
            finally:
                self.key_up(key)

    def move(self, x, y):
        self._guard()
        left, top, right, bottom = self.bounds()
        if not left <= x < right or not top <= y < bottom:
            raise ValueError("field aim outside current game window")
        if self.controller.dry_run:
            self.controller.log.info("Dry-run field aim: %s %s", x, y)
        else:
            pydirectinput.moveTo(x, y, _pause=False)
