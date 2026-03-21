from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import numpy as np

from diablo2.common.controller import pydirectinput


@dataclass(frozen=True)
class ArcaneWing:
    key: str
    clock_label: str
    search_order: int


@dataclass
class ArcaneBeliefState:
    phase: str
    checked_wings: list[str]
    current_wing: str | None
    remaining_wings: list[str]
    last_safe_anchor: str
    danger_level: str
    stuck_counter: int = 0
    summoner_detected: bool = False


ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/teleporter_when_hover.png")
ARCANE_STAR_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/star.png")
ARCANE_SUMMONER_NORTH_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/summoner_north.png")
ARCANE_WITHOUT_SUMMONER_NORTH_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/without_summoner_north.png")
ARCANE_NORTH_WAY_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/north_way.png")
ARCANE_HORAZON_JOURNAL_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/horazon_journal.png")
ARCANE_SUMMONER_LOCATION_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/summoner_location.png")
ARCANE_SUMMONER_LOCATION_BACKGROUND_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/summoner_location_background.png")
ARCANE_SANCTUARY_RATIO = (220 / 447, 498 / 597)
ARCANE_ENTRY_SETTLE = (2.4, 2.8)
ARCANE_LABELS_SETTLE = (0.28, 0.38)
ARCANE_BUFF_STEP_SETTLE = (0.35, 0.5)
ARCANE_WINGS: tuple[ArcaneWing, ...] = (
    ArcaneWing("north", "2 o'clock", 1),
    ArcaneWing("east", "4 o'clock", 2),
    ArcaneWing("south", "8 o'clock", 3),
    ArcaneWing("west", "10 o'clock", 4),
)
ARCANE_MONSTER_TEMPLATE_DIR = Path("assets/monster/act2/arcane_sanctuary")
ARCANE_MONSTER_THRESHOLD = 0.78
ARCANE_NORTH_TEST_TICK_SLEEP = (0.12, 0.18)
ARCANE_ZERO_POINT_CURSOR_RATIO = (0.50, 0.46)
ARCANE_NORTH_CURSOR_RATIO = (0.84, 0.24)
ARCANE_NEXT_PATH_CURSOR_RATIO = (0.12, 0.18)
ARCANE_RETURN_NORTH_CURSOR_RATIO = (0.88, 0.12)
ARCANE_FINAL_NORTH_BEND_CURSOR_RATIO = (0.90, 0.14)
ARCANE_FOUR_OCLOCK_CURSOR_RATIO = (0.92, 0.82)
ARCANE_MOVE_STEP_SETTLE = (0.001, 0.005)
ARCANE_FLOOR_SCORE_RADIUS = 18
ARCANE_FLOOR_CANDIDATE_OFFSETS = ((0.0, 0.0), (0.03, 0.0), (-0.03, 0.0), (0.0, 0.03), (0.0, -0.03), (0.05, -0.02), (-0.05, 0.02))
ARCANE_FOUR_OCLOCK_FLOOR_CANDIDATE_OFFSETS = ((0.0, 0.0), (0.02, 0.02), (0.04, 0.04), (0.06, 0.06), (-0.02, -0.02))
ARCANE_TELEPORTER_HOVER_THRESHOLD = 0.82
ARCANE_STAR_THRESHOLD = 0.82
ARCANE_NORTH_TERMINAL_THRESHOLD = 0.8
ARCANE_SUMMONER_CLUE_THRESHOLD = 0.78
ARCANE_NORTH_WAY_THRESHOLD = 0.78
ARCANE_STAR_STAGNANT_LIMIT = 2
ARCANE_FIRST_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_FIRST_FORK_STAGNANT_LIMIT = 2
ARCANE_SECOND_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_SECOND_FORK_STAGNANT_LIMIT = 2
ARCANE_BRANCH_SPAN_TRIM_TICKS = 3
ARCANE_TELEPORTER_NUDGE_OFFSETS = ((0.04, 0.0), (-0.04, 0.0), (0.0, 0.04), (0.0, -0.04))
ARCANE_PROGRESS_CHANGE_THRESHOLD = 8.0
ARCANE_PROGRESS_STAGNANT_LIMIT = 2
ARCANE_BRANCH_FLOOR_PREFERENCE_THRESHOLD = 90.0
ARCANE_BRANCH_FLOOR_SCORE_MARGIN = 8.0
ARCANE_FINAL_STAGE_TRAVEL_TICKS = 2
ARCANE_FINAL_STAGE_MIN_FOUR_OCLOCK_TICKS = 1
ARCANE_FINAL_QUARTER_FLOOR_THRESHOLD = 20.0


def load_arcane_assets(session) -> None:
    session._arcane_teleporter_hover_template = session._load_image(ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH)
    session._arcane_star_template = session._load_image(ARCANE_STAR_TEMPLATE_PATH)
    session._arcane_summoner_north_template = session._load_optional_image(ARCANE_SUMMONER_NORTH_TEMPLATE_PATH, "Arcane north Summoner")
    session._arcane_without_summoner_north_template = session._load_optional_image(
        ARCANE_WITHOUT_SUMMONER_NORTH_TEMPLATE_PATH, "Arcane north without-Summoner"
    )
    session._arcane_north_way_template = session._load_image(ARCANE_NORTH_WAY_TEMPLATE_PATH)
    session._arcane_horazon_journal_template = session._load_image(ARCANE_HORAZON_JOURNAL_TEMPLATE_PATH)
    session._arcane_summoner_location_template = session._load_image(ARCANE_SUMMONER_LOCATION_TEMPLATE_PATH)
    session._arcane_summoner_location_background_template = session._load_image(ARCANE_SUMMONER_LOCATION_BACKGROUND_TEMPLATE_PATH)
    session._arcane_monster_templates = None


def settle_arcane_entry(session, capture) -> None:
    session.events.put(session.event_class("info", "Summoner: waiting for Arcane Sanctuary to finish loading."))
    session._sleep_range(*ARCANE_ENTRY_SETTLE)
    session.events.put(session.event_class("info", "Summoner: pressing ALT once to enable item labels in Arcane Sanctuary."))
    session._press_key("alt")
    session._sleep_range(*ARCANE_LABELS_SETTLE)


def run_arcane_pre_run_buffs(session) -> None:
    actions = resolve_arcane_character_actions(session)
    if actions is None:
        session.events.put(session.event_class("info", "Summoner: no character action profile is configured; skipping Arcane entry buffs."))
        return
    if not actions.pre_run_buff_order:
        session.events.put(
            session.event_class("info", "Summoner: character has no pre-run buff order configured; skipping Arcane entry buffs.")
        )
        return

    session.events.put(
        session.event_class("info", f"Summoner: replaying {len(actions.pre_run_buff_order)} configured Arcane entry buff action(s).")
    )
    for index, token in enumerate(actions.pre_run_buff_order, start=1):
        session.events.put(session.event_class("info", f"Summoner: Arcane buff step {index}/{len(actions.pre_run_buff_order)} -> {token}."))
        execute_configured_action(session, token)
        sleep_after_buff_action(session, actions, token)


def prepare_arcane_entry(session, capture) -> None:
    settle_arcane_entry(session, capture)
    run_arcane_pre_run_buffs(session)


def sleep_after_buff_action(session, actions, token: str) -> None:
    normalized = token.strip().lower()
    pause_seconds = actions.buff_action_pause_seconds.get(normalized)
    if pause_seconds is None:
        session._sleep_range(*ARCANE_BUFF_STEP_SETTLE)
        return
    session._sleep_range(pause_seconds, pause_seconds)


def run_arcane_controller(session, capture) -> None:
    state = build_initial_arcane_belief_state()
    session.events.put(
        session.event_class(
            "info", "Summoner: Arcane controller initialized at the hub; fixed wing order is north -> east -> south -> west."
        )
    )
    session.events.put(
        session.event_class(
            "info", "Summoner: Arcane wing angle mapping is north=2 o'clock, east=4 o'clock, south=8 o'clock, west=10 o'clock."
        )
    )
    log_arcane_state(session, state)
    next_wing = peek_next_arcane_wing(state)
    if next_wing is None:
        session.events.put(session.event_class("info", "Summoner: no Arcane wings remain to search."))
        return
    commit_arcane_wing(state, next_wing)
    session.events.put(
        session.event_class(
            "info", f"Summoner: committed to Arcane wing '{next_wing.key}' ({next_wing.clock_label}) as the first search branch."
        )
    )
    log_arcane_state(session, state)


def build_initial_arcane_belief_state() -> ArcaneBeliefState:
    return ArcaneBeliefState(
        phase="hub",
        checked_wings=[],
        current_wing=None,
        remaining_wings=[wing.key for wing in ARCANE_WINGS],
        last_safe_anchor="hub_waypoint",
        danger_level="stable",
    )


def peek_next_arcane_wing(state: ArcaneBeliefState) -> ArcaneWing | None:
    for wing in ARCANE_WINGS:
        if wing.key in state.remaining_wings:
            return wing
    return None


def commit_arcane_wing(state: ArcaneBeliefState, wing: ArcaneWing) -> None:
    state.phase = "wing_search"
    state.current_wing = wing.key
    state.last_safe_anchor = f"hub_exit_{wing.key}"
    state.remaining_wings = [candidate for candidate in state.remaining_wings if candidate != wing.key]


def log_arcane_state(session, state: ArcaneBeliefState) -> None:
    current = state.current_wing or "none"
    checked = ", ".join(state.checked_wings) if state.checked_wings else "none"
    remaining = ", ".join(state.remaining_wings) if state.remaining_wings else "none"
    session.events.put(
        session.event_class(
            "info",
            f"Summoner: Arcane state phase={state.phase}, current_wing={current}, checked={checked}, remaining={remaining}, anchor={state.last_safe_anchor}, danger={state.danger_level}.",
        )
    )


def load_arcane_monster_templates(session) -> dict[str, np.ndarray]:
    templates: dict[str, np.ndarray] = {}
    if not ARCANE_MONSTER_TEMPLATE_DIR.exists():
        return templates
    for path in sorted(ARCANE_MONSTER_TEMPLATE_DIR.glob("*.png")):
        try:
            templates[path.stem] = session._load_image(path)
        except RuntimeError as exc:
            session.events.put(session.event_class("warning", f"Arcane monster template skipped: {path.name} ({exc})"))
    return templates


def scan_arcane_monsters(session, frame: np.ndarray) -> str | None:
    if not session._arcane_monster_templates:
        return None
    best_name: str | None = None
    best_score = ARCANE_MONSTER_THRESHOLD
    for name, template in session._arcane_monster_templates.items():
        match = session._locate_template(frame, template, ARCANE_MONSTER_THRESHOLD)
        if match is not None and match.score > best_score:
            best_score = match.score
            best_name = name
    return best_name


def resolve_arcane_character_actions(session):
    characters = getattr(session.config, "characters", {})
    if not characters:
        return None
    if "abyss_knight" in characters:
        return characters["abyss_knight"].actions
    for profile in characters.values():
        if profile.preferred_run_profile == "summoner":
            return profile.actions
    return next(iter(characters.values())).actions


def execute_configured_action(session, token: str) -> None:
    normalized = token.strip().lower()
    if not normalized:
        return
    if session._check_for_user_interrupt():
        raise RuntimeError("stopped by user interference")
    if normalized == "right-click":
        pydirectinput.click(button="right")
        session._sleep_range(*session.ACTION_SLEEP)
        return
    if normalized == "left-click":
        pydirectinput.click(button="left")
        session._sleep_range(*session.ACTION_SLEEP)
        return
    session._press_key(normalized)
