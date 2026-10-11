from __future__ import annotations

import argparse
import json
import threading
import time
from dataclasses import asdict
from pathlib import Path

from diablo2.common.config import CaptureConfig, load_config
from diablo2.common.controller import BotController, ControllerFieldInput
from diablo2.common.field_control import FieldDirective, HeldFieldInput
from diablo2.common.survival_vision import SurvivalHudObserver


from diablo2.common.supervised_field import (
    RecordingInput,
    confirm_initial_probe,
    dispatch_buffs,
    read_probe_signals,
    resource_guard,
    validate_permit,
    wait_probe_signal,
    wait_readable_resources,
)
from diablo2.runs.summoner.navigation_execution import (
    dispatch_planned_step,
    load_arcane_step_planner,
    observe_step_landing,
    plan_arcane_step,
)


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


def wait_renewal_deadline(due, *, capture, hud, focused, policy, stop, clock=time.monotonic, progress=lambda remaining: None):
    last_progress = None
    while not stop.is_set() and clock() < due:
        blocked = wait_readable_resources(capture, hud, focused, policy, stop, min(due, clock() + 0.75), clock=clock)
        if blocked:
            # 시한 직전에 관찰을 마쳤다면 다음 단계가 새 화면을 다시 확인한다.
            if blocked == "observation_unverified" and clock() >= due:
                break
            raise RuntimeError("renewal monitor: " + blocked)
        remaining = max(0, due - clock())
        bucket = int(remaining // 10)
        if bucket != last_progress:
            progress(remaining)
            last_progress = bucket
        stop.wait(min(0.02, remaining))
    if stop.is_set():
        raise RuntimeError("renewal monitor interrupted")


def main(argv=None):
    parser = argparse.ArgumentParser(description="Explicitly enabled bounded supervised field input probe")
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--config", type=Path, default=Path("config"))
    parser.add_argument("--character", required=True)
    permits = parser.add_mutually_exclusive_group(required=True)
    permits.add_argument("--permit", type=Path)
    permits.add_argument("--permit-stdin", action="store_true", help="Prepare capture/backend before waiting for one fresh JSON line")
    parser.add_argument("--action", choices=("travel", "combat", "buff"), required=True)
    parser.add_argument("--buff-trace", action="store_true", help="Capture bounded per-key buff frames in memory; save after input cleanup")
    parser.add_argument(
        "--renew-once", action="store_true", help="Prepare one configured timer renewal; requires fresh supervised proof and safety"
    )
    parser.add_argument("--seconds", type=float, default=0.5)
    parser.add_argument(
        "--step-after-buff",
        choices=("north", "east", "south", "west"),
        help="One experimental current-frame step after fresh supervised effect/safety proof",
    )
    parser.add_argument("--aim", type=int, nargs=2, help="Reviewed native window point")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.buff_trace and args.action != "buff":
        parser.error("--buff-trace requires --action buff")
    if args.renew_once and (args.action != "buff" or not args.permit_stdin):
        parser.error("--renew-once requires --action buff and --permit-stdin")
    if args.step_after_buff and (
        args.action != "buff" or not args.permit_stdin or args.character != "flash" or args.renew_once or args.seconds > 0.1
    ):
        parser.error("--step-after-buff requires Flash buff/stdin mode, no renewal, and at most 0.1 seconds")
    if not 0 < args.seconds <= 1.0 or (args.action != "buff" and args.aim is None):
        parser.error("hold requires aim and duration in (0,1] seconds")
    config = load_config(args.config)
    profile = config.characters[args.character]
    if profile.survival is None:
        parser.error("character survival policy required")
    step_planner = load_arcane_step_planner() if args.step_after_buff else None
    step_reference = None
    if args.step_after_buff:
        estimate = config.casting_rules.estimate(profile.ruleset_family, profile.character_class, profile.casting)
        if estimate.action != "estimate" or estimate.frames is None:
            parser.error("step requires configured class/FCR reference")
        step_reference = estimate.frames / 25.0
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
    trace_frames = [] if args.buff_trace else None
    renewal_trace = [] if args.buff_trace and args.renew_once else None
    output_created = False
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
        output_created = True
        for key in (config.hotkeys.stop, config.hotkeys.pause):
            hotkeys.append(keyboard.add_hotkey(key, lambda: stop.set()))
        first = capture.grab().frame
        cv.imwrite(str(args.output / "before.png"), first)
        # 준비 시간은 유지 시간에 넣지 않고, 준비 후 승인/효과의 시한을 다시 확인한다.
        validate_permit(permit, args.character, profile.survival, time.time(), buff=args.action == "buff")
        if args.action == "buff":
            report = dispatch_buffs(capture, hud, backend, focused, profile, stop, trace_frames=trace_frames)
            if args.step_after_buff and report["status"] == "dispatched_unconfirmed":
                signals = read_probe_signals()
                print("AWAIT_STEP_EFFECT: fresh supervised_visual effect/equipment and field safety required", flush=True)
                proof = wait_probe_signal(
                    signals,
                    capture=capture,
                    hud=hud,
                    focused=focused,
                    policy=profile.survival,
                    stop=stop,
                    deadline=time.monotonic() + 60,
                    phase="initial_effect",
                    accept=lambda signal: confirm_initial_probe(
                        signal, args.character, report["buff_dispatches"][-1]["cast_request_at"], time.time()
                    ),
                )
                validate_permit(proof, args.character, profile.survival, time.time(), buff=True)
                report["initial_supervised_confirmation"] = proof
                acquired = time.monotonic()
                frame = capture.grab().frame
                choice, ground_reading, view = plan_arcane_step(frame, args.step_after_buff, *step_planner, acquired)
                blocked = resource_guard(hud.observe(frame), profile.survival)
                if blocked:
                    raise RuntimeError(blocked)
                validate_permit(proof, args.character, profile.survival, time.time(), buff=True)
                aim = (capture.target["left"] + choice.aim[0], capture.target["top"] + choice.aim[1])
                report["path_choice"] = asdict(choice)
                report["player_ground"] = asdict(ground_reading)
                report["planned_frame_at"] = acquired
                if dispatch_planned_step(backend, focused, stop, aim, profile.actions.movement_skill_key, acquired):
                    report["navigation"] = observe_step_landing(
                        capture, hud, focused, profile.survival, stop, frame, view, choice, *step_planner, reference_seconds=step_reference
                    )
                else:
                    report["navigation"] = {"status": "dispatch_blocked"}
                report["status"] = "step_arrival_observed" if report["navigation"]["status"] == "arrival_observed" else "step_failed"
            if args.renew_once and report["status"] == "dispatched_unconfirmed":
                if len(profile.survival.buffs) != 1:
                    raise ValueError("timer probe requires one explicit timed buff rule")
                rule = profile.survival.buffs[0]
                cast = next(row for row in reversed(report["buff_dispatches"]) if row["key"] == rule.key)
                due = cast["cast_request_monotonic"] + rule.duration_seconds - rule.renew_before_seconds
                signals = read_probe_signals()
                print("AWAIT_INITIAL_EFFECT: fresh supervised_visual confirmation required; no renewal armed", flush=True)
                proof = wait_probe_signal(
                    signals,
                    capture=capture,
                    hud=hud,
                    focused=focused,
                    policy=profile.survival,
                    stop=stop,
                    deadline=min(due, time.monotonic() + 60),
                    phase="initial_effect",
                    accept=lambda signal: confirm_initial_probe(
                        signal, args.character, report["buff_dispatches"][-1]["cast_request_at"], time.time()
                    ),
                )
                report["initial_supervised_confirmation"] = proof
                report["renewal_due_monotonic"] = due
                wait_renewal_deadline(
                    due,
                    capture=capture,
                    hud=hud,
                    focused=focused,
                    policy=profile.survival,
                    stop=stop,
                    progress=lambda remaining: print("RENEWAL_TIMER: {:.3f} seconds remaining".format(remaining), flush=True),
                )
                print("AWAIT_RENEWAL_SAFETY: prepared executor requires one fresh permit within 10 seconds", flush=True)
                renewal_permit = wait_probe_signal(
                    signals,
                    capture=capture,
                    hud=hud,
                    focused=focused,
                    policy=profile.survival,
                    stop=stop,
                    deadline=due + 10,
                    phase="renewal_safety",
                    accept=lambda signal: validate_permit(signal, args.character, profile.survival, time.time(), buff=True),
                )
                validate_permit(renewal_permit, args.character, profile.survival, time.time(), buff=True)
                report["renewal"] = dispatch_buffs(capture, hud, backend, focused, profile, stop, trace_frames=renewal_trace)
                report["status"] = "renewed_unconfirmed" if report["renewal"]["status"] == "dispatched_unconfirmed" else "renewal_failed"
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
    except Exception as exc:
        if not output_created:
            raise
        report["status"] = "probe_error"
        report["errors"].append(str(exc))
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
    if trace_frames is not None:
        # 제어가 끝나고 눌린 입력/캡처를 정리한 뒤에만 이미지 파일을 저장한다.
        report["buff_trace"] = []
        for frame, metadata in trace_frames:
            filename = "buff-step-{:02d}.png".format(metadata["index"])
            if not cv.imwrite(str(args.output / filename), frame):
                raise RuntimeError("Could not save buff trace frame")
            report["buff_trace"].append(dict(metadata, frame=filename))
    if renewal_trace is not None:
        report["renewal_trace"] = []
        for frame, metadata in renewal_trace:
            filename = "renewal-step-{:02d}.png".format(metadata["index"])
            if not cv.imwrite(str(args.output / filename), frame):
                raise RuntimeError("Could not save renewal trace frame")
            report["renewal_trace"].append(dict(metadata, frame=filename))
    if args.action != "buff" and not any(e["action"] == "key_down" and e["sent"] for e in backend.events):
        report["status"] = "no_input_" + report["status"]
    (args.output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    return (
        0
        if report["status"] in ("duration_complete", "dispatched_unconfirmed", "renewed_unconfirmed", "step_arrival_observed")
        and not report["errors"]
        else 2
    )


if __name__ == "__main__":
    raise SystemExit(main())
