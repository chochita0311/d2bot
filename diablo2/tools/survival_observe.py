from __future__ import annotations

import argparse
import json
import math
import time
from dataclasses import asdict
from pathlib import Path

import cv2 as cv

from diablo2.common.config import CaptureConfig, load_config
from diablo2.common.survival import CharacterSurvivalState, ObservationStamp
from diablo2.common.survival_vision import DEFAULT_ASSETS, SurvivalHudObserver


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Input-free survival HUD/belt observation; no game inputs")
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--live", action="store_true", help="Capture the exact Diablo II: Resurrected window")
    source.add_argument("--image", type=Path, help="Read a full native window PNG")
    parser.add_argument("--frames", type=int, choices=range(1, 6), default=1)
    parser.add_argument("--assets", type=Path, default=DEFAULT_ASSETS)
    parser.add_argument("--config", type=Path, default=Path("config"))
    parser.add_argument("--character", help="Explicit policy profile; this does not verify the in-game identity")
    parser.add_argument("--room", help="Explicit observation context for policy requests")
    parser.add_argument("--age-limit", type=float, help="Explicit diagnostic age limit for policy requests, in seconds")
    parser.add_argument("--output", type=Path, help="Optional JSON report path; no raw frames are written")
    return parser


def main(argv=None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if bool(args.character) != bool(args.room):
        parser.error("--character and --room must be provided together")
    if args.character and (args.age_limit is None or not math.isfinite(args.age_limit) or args.age_limit <= 0):
        parser.error("Policy requests require an explicit positive finite --age-limit")
    observer = SurvivalHudObserver(args.assets)
    state = None
    if args.character:
        config = load_config(args.config)
        if args.character not in config.characters or config.characters[args.character].survival is None:
            parser.error("Explicit character with a configured survival policy required")
        state = CharacterSurvivalState(args.character, args.room, config.characters[args.character].survival)
    capture = None
    if args.live:
        from diablo2.common.capture import USER32, ScreenCapture, resolve_window_from_config

        capture = ScreenCapture(CaptureConfig(window_title="Diablo II: Resurrected", window_title_mode="exact", capture_backend="window"))
    reports = []
    try:
        for sequence in range(args.frames):
            if capture:
                window = resolve_window_from_config(capture.config)
                if window is None or USER32.IsIconic(window.handle) or not USER32.IsWindowVisible(window.handle):
                    raise RuntimeError("Visible, non-minimized game window required for observation")
            started = time.perf_counter()
            frame = capture.grab().frame if capture else cv.imread(str(args.image), cv.IMREAD_COLOR)
            acquired = time.perf_counter()
            if frame is None:
                raise ValueError("Could not read observation image")
            reading = observer.observe(frame)
            finished = time.perf_counter()
            report = asdict(reading)
            report.update(
                source="live_window" if capture else "offline_image",
                frame_size=[frame.shape[1], frame.shape[0]],
                sequence=sequence,
                capture_started_at=started,
                acquired_at=acquired,
                processed_at=finished,
                capture_seconds=acquired - started,
                processing_seconds=finished - acquired,
                life_ratio=reading.life.ratio,
                mana_ratio=reading.mana.ratio,
            )
            if state:
                # 미확인 벨트는 이전 캡처의 잔량으로 대신하지 않는다.
                current_state = CharacterSurvivalState(args.character, args.room, state.policy)
                stamp = ObservationStamp(args.character, args.room, sequence, started)
                contents = reading.belt_contents(state.policy)
                if contents is not None:
                    current_state.observe_belt(stamp, contents)
                decision = current_state.decide(
                    reading.safety_observation(stamp),
                    finished,
                    max_observation_age_seconds=args.age_limit,
                    max_belt_age_seconds=args.age_limit,
                )
                report.update(policy_character=args.character, room=args.room, belt_contents=contents, decision=asdict(decision))
            reports.append(report)
    finally:
        if capture:
            capture.close()
    result = {"mode": "observation_only", "inputs_sent": 0, "reports": reports}
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
