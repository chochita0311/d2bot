"""Development-only comparison of pydirectinput payloads; SendInput is replaced."""

import json
import sys
from unittest.mock import patch

import pydirectinput


def main():
    request = json.load(sys.stdin)
    events = []

    def send(count, pointer, size):
        event = pointer.contents
        if event.type != 1:
            raise RuntimeError("Oracle accepts fake keyboard events only")
        key = event.ii.ki
        events.append({"scan": key.wScan, "up": bool(key.dwFlags & 2), "extended": bool(key.dwFlags & 1)})
        return count

    # Match a deterministic fake OS map; no live keyboard or pointer is accessed.
    for name, code in (("left", 0x25), ("up", 0x26), ("right", 0x27), ("down", 0x28)):
        pydirectinput.KEYBOARD_MAPPING[name] = code + 0x100
    result = {}
    with patch.object(pydirectinput, "SendInput", send), patch.object(pydirectinput, "failSafeCheck", lambda: None), patch(
        "ctypes.windll.user32.GetKeyState", return_value=int(request["numlock"])
    ):
        for key in sorted(pydirectinput.KEYBOARD_MAPPING):
            events.clear()
            if not pydirectinput.keyDown(key, _pause=False) or not pydirectinput.keyUp(key, _pause=False):
                raise RuntimeError("Fake dispatch failed")
            result[key] = list(events)
    json.dump(result, sys.stdout, ensure_ascii=True)


if __name__ == "__main__":
    main()
