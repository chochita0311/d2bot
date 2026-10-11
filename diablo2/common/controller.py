from __future__ import annotations

import logging
import time
import ctypes

try:
    import keyboard
except ImportError:  # pragma: no cover
    keyboard = None

try:
    import pydirectinput
except ImportError:  # pragma: no cover
    pydirectinput = None


def _virtual_pointer_move(x, y):
    # moveTo는 주 모니터 크기로 정규화한다. 다른 모니터/음수 원점도
    # 마을 입력과 같은 가상 데스크톱 좌표로 전송해야 실제 조준점과 일치한다.
    user32 = ctypes.windll.user32
    left, top = user32.GetSystemMetrics(76), user32.GetSystemMetrics(77)
    width, height = user32.GetSystemMetrics(78), user32.GetSystemMetrics(79)
    if width <= 1 or height <= 1 or not left <= x < left + width or not top <= y < top + height:
        raise ValueError("field aim outside virtual desktop")
    dx, dy = round((x - left) * 65535 / (width - 1)), round((y - top) * 65535 / (height - 1))
    extra = ctypes.c_ulong(0)
    payload = pydirectinput.Input_I()
    payload.mi = pydirectinput.MouseInput(dx, dy, 0, 0x0001 | 0x8000 | 0x4000, 0, ctypes.pointer(extra))
    event = pydirectinput.Input(ctypes.c_ulong(0), payload)
    if pydirectinput.SendInput(1, ctypes.pointer(event), ctypes.sizeof(event)) != 1:
        raise RuntimeError("field pointer movement was not dispatched")
    return (left, top, width, height)


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
        self.pointer_evidence = None

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

    def pointer_matches(self, point):
        self._guard()
        if self.controller.dry_run:
            return True
        actual = tuple(pydirectinput.position())
        return max(abs(a - b) for a, b in zip(actual, point)) <= 3

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
                # 단일 눌림을50ms 유지한다. 전송 성공과 실제 시전/착지는 별도 확인한다.
                # 재시전은 하지 않으며 중단/포커스 변화 중에도finally에서 해제한다.
                time.sleep(0.05)
            finally:
                self.key_up(key)

    def click(self):
        self._guard()
        if self.controller.dry_run:
            self.controller.log.info("Dry-run field loot click")
            return
        left, top, right, bottom = self.bounds()
        x, y = pydirectinput.position()
        if not left <= x < right or not top <= y < bottom:
            raise ValueError("field click outside current game window")
        try:
            if pydirectinput.mouseDown(_pause=False) is not True:
                raise RuntimeError("field mouse down was not dispatched")
            time.sleep(0.05)
        finally:
            if pydirectinput.mouseUp(_pause=False) is not True:
                raise RuntimeError("field mouse release was not dispatched")

    def move(self, x, y):
        self._guard()
        left, top, right, bottom = self.bounds()
        if not left <= x < right or not top <= y < bottom:
            raise ValueError("field aim outside current game window")
        if self.controller.dry_run:
            self.controller.log.info("Dry-run field aim: %s %s", x, y)
        else:
            self.pointer_evidence = None
            desktop = _virtual_pointer_move(x, y)
            started = time.monotonic()
            # 전송 완료만으로 조준 성공을 만들지 않는다. 재전송 없이 짧게 확인한다.
            while True:
                self._guard()
                actual = tuple(pydirectinput.position())
                elapsed = time.monotonic() - started
                if max(abs(actual[0] - x), abs(actual[1] - y)) <= 3:
                    self.pointer_evidence = dict(requested=(x, y), observed=actual, desktop=desktop, settle_seconds=elapsed)
                    return
                if elapsed >= 0.08:
                    raise RuntimeError("field pointer mapping mismatch: requested {}, observed {}".format((x, y), actual))
                time.sleep(0.005)
