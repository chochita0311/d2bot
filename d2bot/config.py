from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass(slots=True)
class CaptureConfig:
    fps: int = 8
    monitor_index: int = 1
    region: dict[str, int] | None = None
    preview_scale: float = 0.75


@dataclass(slots=True)
class RecordingConfig:
    enabled: bool = False
    output_path: str = "recordings/session.avi"
    codec: str = "XVID"


@dataclass(slots=True)
class HotkeyConfig:
    pause: str = "f8"
    stop: str = "f9"


@dataclass(slots=True)
class TemplateRule:
    name: str
    path: str
    threshold: float = 0.85
    action: str = "log"


@dataclass(slots=True)
class FarmProfile:
    name: str = "countess"
    goal: str = "watch"
    loot_whitelist: list[str] = field(
        default_factory=lambda: ["jah rune", "ber rune", "key of terror"]
    )
    templates: list[TemplateRule] = field(default_factory=list)


@dataclass(slots=True)
class BotConfig:
    dry_run: bool = True
    overlay: bool = True
    log_level: str = "INFO"
    capture: CaptureConfig = field(default_factory=CaptureConfig)
    recording: RecordingConfig = field(default_factory=RecordingConfig)
    hotkeys: HotkeyConfig = field(default_factory=HotkeyConfig)
    farm: FarmProfile = field(default_factory=FarmProfile)


def _build_template(rule: dict[str, Any]) -> TemplateRule:
    return TemplateRule(
        name=rule["name"],
        path=rule["path"],
        threshold=rule.get("threshold", 0.85),
        action=rule.get("action", "log"),
    )


def load_config(path: str | Path) -> BotConfig:
    raw = json.loads(Path(path).read_text(encoding="utf-8"))

    capture = CaptureConfig(**raw.get("capture", {}))
    recording = RecordingConfig(**raw.get("recording", {}))
    hotkeys = HotkeyConfig(**raw.get("hotkeys", {}))

    farm_raw = raw.get("farm", {})
    farm = FarmProfile(
        name=farm_raw.get("name", "countess"),
        goal=farm_raw.get("goal", "watch"),
        loot_whitelist=farm_raw.get(
            "loot_whitelist", ["jah rune", "ber rune", "key of terror"]
        ),
        templates=[_build_template(item) for item in farm_raw.get("templates", [])],
    )

    return BotConfig(
        dry_run=raw.get("dry_run", True),
        overlay=raw.get("overlay", True),
        log_level=raw.get("log_level", "INFO"),
        capture=capture,
        recording=recording,
        hotkeys=hotkeys,
        farm=farm,
    )
