from __future__ import annotations

import argparse
import json
import math
import threading
import time
from pathlib import Path

from diablo2.common.config import CaptureConfig, load_config
from diablo2.common.controller import BotController, ControllerFieldInput
from diablo2.common.field_control import FieldDirective, HeldFieldInput
from diablo2.common.survival_vision import SurvivalHudObserver


def validate_permit(permit, character, policy, now, *, buff=False):
    required = ("field", "monsters_clear", "belt_closed")
    if not buff:
        required += ("buff_effect_verified", "equipment_restored")
    if permit.get("character_id") != character or any(permit.get(key) is not True for key in required):
        raise ValueError("explicit supervised field/buff/equipment/belt confirmations required")
    at = permit.get("observed_at")
    if type(at) not in (int, float) or not math.isfinite(at) or not 0 <= now - at <= 15:
        raise ValueError("supervised permit is stale")
    count = permit.get("potions_remaining")
    if type(count) is not int or not 1 <= count <= sum(c.capacity for c in policy.belt_columns if c.kind == policy.potion_kind):
        raise ValueError("confirmed nonempty purple belt required")
    if not buff:
        casts = permit.get("buff_casts", {})
        if any(
            type(casts.get(rule.name)) not in (int, float)
            or not math.isfinite(casts[rule.name])
            or not 0 <= now - casts[rule.name] < rule.duration_seconds - rule.renew_before_seconds
            for rule in policy.buffs
        ):
            raise ValueError("confirmed buff already due or missing cast time")


class RecordingInput:
    def __init__(self, backend, clock=time.monotonic):
        self.backend, self.clock, self.events = backend, clock, []

    def _send(self, action, *args):
        row = {"action": action, "args": args, "at": self.clock(), "wall_at": time.time(), "sent": False}
        self.events.append(row)
        getattr(self.backend, action)(*args)
        row["sent"] = True

    def key_down(self, key):
        self._send("key_down", key)

    def key_up(self, key):
        self._send("key_up", key)

    def press(self, key):
        self._send("press", key)

    def move(self, x, y):
        self._send("move", x, y)


def resource_guard(reading, policy):
    if reading.belt_visibility != "closed" or reading.life.ratio is None or reading.mana.ratio is None:
        return "observation_unverified"
    if reading.life.ratio < policy.life_potion_below or reading.mana.ratio < policy.mana_potion_below:
        return "resource_low"
    return None


def wait_readable_resources(capture, hud, focused, policy, stop, deadline, *, clock=time.monotonic):
    while clock() < deadline and not stop.is_set():
        if not focused():
            return "focus_lost"
        acquired = clock()
        reading = hud.observe(capture.grab().frame)
        if clock() - acquired >= 0.25:
            return "screen_stale"
        blocked = resource_guard(reading, policy)
        if blocked != "observation_unverified":
            return blocked
        # 잠깐 가려진 글자는 입력 없이 새 화면을 기다린다. 이전 최대값을 재사용하지 않는다.
        stop.wait(0.02)
    return "observation_unverified" if not stop.is_set() else "interrupted"


def hold_probe(capture, hud, inputs, focused, policy, action, seconds, aim, stop, *, buff_deadline, clock=time.monotonic):
    rows, errors, reason = [], [], []
    started = clock()
    deadline = min(started + seconds, buff_deadline)

    def finish(value):
        if not reason:
            reason.append(value)
        stop.set()

    def watchdog():
        try:
            while not stop.wait(0.005):
                now = clock()
                if not focused():
                    finish("focus_lost")
                elif now >= deadline:
                    finish("buff_due" if buff_deadline <= started + seconds else "duration_complete")
                else:
                    inputs.expire(now)
        except Exception as exc:
            errors.append(str(exc))
            finish("input_error")
        finally:
            try:
                inputs.close()
            except Exception as exc:
                errors.append(str(exc))

    thread = threading.Thread(target=watchdog, name="d2-probe-watchdog", daemon=True)
    thread.start()
    try:
        while not stop.is_set():
            acquired = clock()
            reading = hud.observe(capture.grab().frame)
            now = clock()
            rows.append(
                {
                    "elapsed": now - started,
                    "life": [reading.life.current, reading.life.maximum],
                    "mana": [reading.mana.current, reading.mana.maximum],
                    "belt": reading.belt_visibility,
                }
            )
            blocked = resource_guard(reading, policy)
            if blocked:
                finish(blocked)
            elif not focused():
                finish("focus_lost")
            elif now - acquired >= 0.25:
                finish("screen_stale")
            elif now < deadline and not stop.is_set():
                inputs.apply(FieldDirective(action, "supervised_probe", aim=aim, deadline=min(deadline, acquired + 0.25)), now)
            stop.wait(0.02)
    except Exception as exc:
        errors.append(str(exc))
        finish("input_error")
    finally:
        finish("interrupted")
        try:
            inputs.close()
        except Exception as exc:
            errors.append(str(exc))
        thread.join(1)
        if thread.is_alive():
            errors.append("watchdog did not finish")
    return {"status": reason[0], "held_after": sorted(inputs.held), "errors": errors, "observations": rows}


def dispatch_buffs(capture, hud, backend, focused, profile, stop):
    # 짧은 감독 준비용이다. 전송 기록은 효과나 안전의 자동 확인이 아니다.
    events, swapped, status = [], False, "dispatched_unconfirmed"
    began = time.monotonic()
    try:
        for token in profile.actions.pre_run_buff_order:
            if stop.is_set() or not focused() or time.monotonic() - began >= 8:
                status = "interrupted"
                break
            blocked = wait_readable_resources(capture, hud, focused, profile.survival, stop, min(began + 8, time.monotonic() + 0.75))
            if blocked:
                status = blocked
                break
            if stop.is_set() or not focused() or time.monotonic() - began >= 8:
                status = "interrupted_or_stale"
                break
            if token in ("right-click", "left-click"):
                raise ValueError("this probe supports configured keyboard buff sequences only")
            at = time.time()
            backend.press(token)
            events.append({"key": token, "cast_request_at": at})
            if token == "w":
                swapped = not swapped
            stop.wait(profile.actions.buff_action_pause_seconds.get(token, 0.4))
    finally:
        # 성공한 장비 전환 한 번 뒤 중단됐다면 포커스가 유지될 때만 복원 전송한다.
        if swapped and focused():
            backend.press("w")
            events.append({"key": "w", "purpose": "restore_after_interrupt", "cast_request_at": time.time()})
            swapped = False
    return {
        "status": status,
        "buff_dispatches": events,
        "equipment_dispatch_balanced": not swapped,
        "effect_verified": False,
        "held_after": [],
        "errors": [],
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description="Explicitly enabled bounded supervised field input probe")
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--config", type=Path, default=Path("config"))
    parser.add_argument("--character", required=True)
    permits = parser.add_mutually_exclusive_group(required=True)
    permits.add_argument("--permit", type=Path)
    permits.add_argument("--permit-stdin", action="store_true", help="Prepare capture/backend before waiting for one fresh JSON line")
    parser.add_argument("--action", choices=("travel", "combat", "buff"), required=True)
    parser.add_argument("--seconds", type=float, default=0.5)
    parser.add_argument("--aim", type=int, nargs=2, help="Reviewed native window point")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if not 0 < args.seconds <= 1.0 or (args.action != "buff" and args.aim is None):
        parser.error("hold requires aim and duration in (0,1] seconds")
    config = load_config(args.config)
    profile = config.characters[args.character]
    if profile.survival is None:
        parser.error("character survival policy required")
    from diablo2.common.capture import USER32, ScreenCapture, resolve_window_from_config
    import cv2 as cv
    import keyboard

    hud = SurvivalHudObserver()
    capture = ScreenCapture(CaptureConfig(window_title="Diablo II: Resurrected", window_title_mode="exact", capture_backend="window"))
    window = resolve_window_from_config(capture.config)
    focused = lambda: window is not None and USER32.GetForegroundWindow() == window.handle and not USER32.IsIconic(window.handle)
    controller = BotController(dry_run=not args.live)
    backend = RecordingInput(
        ControllerFieldInput(
            controller,
            focused=focused,
            bounds=lambda: (
                capture.target["left"],
                capture.target["top"],
                capture.target["left"] + capture.target["width"],
                capture.target["top"] + capture.target["height"],
            ),
        )
    )
    inputs = HeldFieldInput(backend, profile.actions.movement_skill_key, profile.actions.primary_attack_skill_key)
    stop, hotkeys = threading.Event(), []
    report = {"status": "preparation_error", "errors": []}
    try:
        if args.permit_stdin:
            print("READY: awaiting one fresh supervised permit JSON line; no input sent", flush=True)
            raw = input()
            if len(raw) > 4096:
                parser.error("permit exceeds 4096 characters")
        else:
            raw = args.permit.read_text(encoding="utf-8-sig")
        permit = json.loads(raw)
        try:
            validate_permit(permit, args.character, profile.survival, time.time(), buff=args.action == "buff")
        except ValueError as exc:
            parser.error(str(exc))
        args.output.mkdir(parents=True, exist_ok=False)
        for key in (config.hotkeys.stop, config.hotkeys.pause):
            hotkeys.append(keyboard.add_hotkey(key, lambda: stop.set()))
        first = capture.grab().frame
        cv.imwrite(str(args.output / "before.png"), first)
        # 준비 시간은 유지 시간에 넣지 않고, 준비 후 승인/효과의 시한을 다시 확인한다.
        validate_permit(permit, args.character, profile.survival, time.time(), buff=args.action == "buff")
        if args.action == "buff":
            report = dispatch_buffs(capture, hud, backend, focused, profile, stop)
        else:
            remaining = min(
                permit["buff_casts"][b.name] + b.duration_seconds - b.renew_before_seconds - time.time() for b in profile.survival.buffs
            )
            aim = (capture.target["left"] + args.aim[0], capture.target["top"] + args.aim[1])
            report = hold_probe(
                capture,
                hud,
                inputs,
                focused,
                profile.survival,
                args.action,
                args.seconds,
                aim,
                stop,
                buff_deadline=time.monotonic() + remaining,
            )
        cv.imwrite(str(args.output / "after.png"), capture.grab().frame)
    finally:
        stop.set()
        try:
            inputs.close()
        except Exception as exc:
            report["errors"].append(str(exc))
        try:
            for handle in hotkeys:
                try:
                    keyboard.remove_hotkey(handle)
                except Exception as exc:
                    report["errors"].append(str(exc))
        finally:
            capture.close()
    report.update(
        mode="live_supervised_probe" if args.live else "dry_probe", action=args.action, seconds=args.seconds, input_events=backend.events
    )
    if args.action != "buff" and not any(e["action"] == "key_down" and e["sent"] for e in backend.events):
        report["status"] = "no_input_" + report["status"]
    (args.output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    return 0 if report["status"] in ("duration_complete", "dispatched_unconfirmed") and not report["errors"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
