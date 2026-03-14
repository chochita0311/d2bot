from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class CaptureConfig:
    fps: int = 8
    monitor_index: int = 1
    region: dict[str, int] | None = None
    window_title: str | None = None
    window_title_mode: str = "contains"
    follow_window: bool = True
    capture_backend: str = "auto"
    preview_scale: float = 0.75


@dataclass
class RecordingConfig:
    enabled: bool = False
    output_path: str = "recordings/session.avi"
    codec: str = "XVID"


@dataclass
class HotkeyConfig:
    pause: str = "f8"
    stop: str = "f9"


@dataclass
class TemplateRule:
    name: str
    path: str
    threshold: float = 0.85
    action: str = "log"
    label: str | None = None
    context: str = "any"


@dataclass
class HuntingRules:
    objective: str = "watch"
    waypoint_act: int | None = None
    waypoint_name: str | None = None
    target_monsters: list[str] = field(default_factory=list)
    target_areas: list[str] = field(default_factory=list)
    route_notes: list[str] = field(default_factory=list)
    fight_style: str = "safe"
    search_timeout_seconds: int = 60
    disengage_on_uncertainty: bool = True


@dataclass
class LootRules:
    keep_labels: list[str] = field(default_factory=list)
    ignore_labels: list[str] = field(default_factory=list)
    potion_columns_reserved: int = 2
    free_inventory_slots_min: int = 6
    identify_before_drop: bool = False
    pickup_gold: bool = False


@dataclass
class FixedLootItem:
    label: str
    ground_template: str | None = None
    inventory_template: str | None = None
    threshold: float = 0.85


@dataclass
class SharedLootProfile:
    description: str = ""
    ignore_labels: list[str] = field(default_factory=list)
    fixed_items: list[FixedLootItem] = field(default_factory=list)


@dataclass
class LifeManagementRules:
    use_healing_potion_below: float = 0.65
    use_rejuvenation_below: float = 0.35
    emergency_retreat_below: float = 0.2
    use_mana_potion_below: float = 0.3
    town_portal_on_risk: bool = True
    stop_on_death_screen: bool = True
    belt_restock_healing_below: int = 4
    belt_restock_mana_below: int = 4


@dataclass
class FarmProfile:
    name: str = "run"
    goal: str = "watch"
    description: str = ""
    templates: list[TemplateRule] = field(default_factory=list)
    hunting: HuntingRules = field(default_factory=HuntingRules)
    loot: LootRules = field(default_factory=LootRules)
    life: LifeManagementRules = field(default_factory=LifeManagementRules)
    run_specific_rules: list[str] = field(default_factory=list)


@dataclass
class BotConfig:
    dry_run: bool = True
    overlay: bool = True
    log_level: str = "INFO"
    capture: CaptureConfig = field(default_factory=CaptureConfig)
    recording: RecordingConfig = field(default_factory=RecordingConfig)
    hotkeys: HotkeyConfig = field(default_factory=HotkeyConfig)
    shared_loot: SharedLootProfile = field(default_factory=SharedLootProfile)
    run_profiles: dict[str, FarmProfile] = field(default_factory=dict)
    farm: FarmProfile = field(default_factory=FarmProfile)


def _build_template(rule: dict[str, Any]) -> TemplateRule:
    return TemplateRule(
        name=rule["name"],
        path=rule["path"],
        threshold=rule.get("threshold", 0.85),
        action=rule.get("action", "log"),
        label=rule.get("label"),
        context=rule.get("context", "any"),
    )


def _build_hunting_rules(raw: dict[str, Any]) -> HuntingRules:
    return HuntingRules(
        objective=raw.get("objective", "watch"),
        waypoint_act=raw.get("waypoint_act"),
        waypoint_name=raw.get("waypoint_name"),
        target_monsters=list(raw.get("target_monsters", [])),
        target_areas=list(raw.get("target_areas", [])),
        route_notes=list(raw.get("route_notes", [])),
        fight_style=raw.get("fight_style", "safe"),
        search_timeout_seconds=raw.get("search_timeout_seconds", 60),
        disengage_on_uncertainty=raw.get("disengage_on_uncertainty", True),
    )


def _build_loot_rules(raw: dict[str, Any]) -> LootRules:
    return LootRules(
        keep_labels=list(raw.get("keep_labels", [])),
        ignore_labels=list(raw.get("ignore_labels", [])),
        potion_columns_reserved=raw.get("potion_columns_reserved", 2),
        free_inventory_slots_min=raw.get("free_inventory_slots_min", 6),
        identify_before_drop=raw.get("identify_before_drop", False),
        pickup_gold=raw.get("pickup_gold", False),
    )


def _build_fixed_loot_item(raw: dict[str, Any]) -> FixedLootItem:
    return FixedLootItem(
        label=raw["label"],
        ground_template=raw.get("ground_template"),
        inventory_template=raw.get("inventory_template"),
        threshold=raw.get("threshold", 0.85),
    )


def _build_shared_loot_profile(raw: dict[str, Any]) -> SharedLootProfile:
    return SharedLootProfile(
        description=raw.get("description", ""),
        ignore_labels=list(raw.get("ignore_labels", [])),
        fixed_items=[_build_fixed_loot_item(item) for item in raw.get("fixed_items", [])],
    )


def _build_life_rules(raw: dict[str, Any]) -> LifeManagementRules:
    return LifeManagementRules(
        use_healing_potion_below=raw.get("use_healing_potion_below", 0.65),
        use_rejuvenation_below=raw.get("use_rejuvenation_below", 0.35),
        emergency_retreat_below=raw.get("emergency_retreat_below", 0.2),
        use_mana_potion_below=raw.get("use_mana_potion_below", 0.3),
        town_portal_on_risk=raw.get("town_portal_on_risk", True),
        stop_on_death_screen=raw.get("stop_on_death_screen", True),
        belt_restock_healing_below=raw.get("belt_restock_healing_below", 4),
        belt_restock_mana_below=raw.get("belt_restock_mana_below", 4),
    )


def _build_farm_profile(name: str, raw: dict[str, Any]) -> FarmProfile:
    return FarmProfile(
        name=raw.get("name", name),
        goal=raw.get("goal", "watch"),
        description=raw.get("description", ""),
        templates=[_build_template(item) for item in raw.get("templates", [])],
        hunting=_build_hunting_rules(raw.get("hunting", {})),
        loot=_build_loot_rules(raw.get("loot", {})),
        life=_build_life_rules(raw.get("life", {})),
        run_specific_rules=list(raw.get("run_specific_rules", [])),
    )


def _deep_merge(base: dict[str, Any], incoming: dict[str, Any]) -> dict[str, Any]:
    merged = dict(base)
    for key, value in incoming.items():
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = _deep_merge(merged[key], value)
        else:
            merged[key] = value
    return merged


def _load_raw_config(path: Path) -> dict[str, Any]:
    if path.is_dir():
        raw: dict[str, Any] = {}
        for child in sorted(path.rglob("*.json")):
            child_raw = json.loads(child.read_text(encoding="utf-8-sig"))
            raw = _deep_merge(raw, child_raw)
        return raw
    return json.loads(path.read_text(encoding="utf-8-sig"))


def load_config(path: str | Path) -> BotConfig:
    raw = _load_raw_config(Path(path))

    capture = CaptureConfig(**raw.get("capture", {}))
    recording = RecordingConfig(**raw.get("recording", {}))
    hotkeys = HotkeyConfig(**raw.get("hotkeys", {}))
    shared_loot = _build_shared_loot_profile(raw.get("shared_loot", {}))

    run_profiles_raw = raw.get("run_profiles", {})
    run_profiles = {
        profile_name: _build_farm_profile(profile_name, profile_raw)
        for profile_name, profile_raw in run_profiles_raw.items()
    }

    farm_raw = raw.get("farm", {})
    legacy_farm = _build_farm_profile("farm", farm_raw) if farm_raw else FarmProfile()

    selected_farm = legacy_farm
    if run_profiles and not farm_raw:
        first_profile_name = next(iter(run_profiles))
        selected_farm = run_profiles[first_profile_name]

    return BotConfig(
        dry_run=raw.get("dry_run", True),
        overlay=raw.get("overlay", True),
        log_level=raw.get("log_level", "INFO"),
        capture=capture,
        recording=recording,
        hotkeys=hotkeys,
        shared_loot=shared_loot,
        run_profiles=run_profiles,
        farm=selected_farm,
    )
