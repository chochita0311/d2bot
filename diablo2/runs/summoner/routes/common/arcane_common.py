from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import numpy as np

from diablo2.common.controller import pydirectinput
from diablo2.common.movement import (
    MOVEMENT_INTENT_REPOSITION,
    MovementExecutionState,
    apply_movement_intent,
    release_movement_intent,
)


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


# Arcane Sanctuary ?쒗뵆由?寃쎈줈 臾띠쓬
# hover blocker, hub ?뺣젹, Summoner ?꾩튂 ?먯젙???먯궛
# Arcane Sanctuary 怨듯넻 ?쒗뵆由?寃쎈줈 臾띠쓬
# hover blocker, hub ?뺣젹, Summoner ?꾩튂 ?먯젙 ?먯궛
ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/teleporter_when_hover.png")
ARCANE_CHEST_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/chest_when_hover.png")
ARCANE_SMALL_CHEST_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/small_chest_when_hover.png")
ARCANE_SMALL_LOCKED_CHEST_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/small_locked_chest_when_hover.png")
ARCANE_SMALL_COFFIN_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/small_coffin_when_hover.png")
ARCANE_STAR_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/star.png")
ARCANE_HUB_CENTER_TEMPLATE_PATH = Path("assets/waypoint/act2/hub_center.png")
ARCANE_GOAL_CENTER_TEMPLATE_PATH = Path("assets/waypoint/act2/goal_center.png")

# Arcane Sanctuary 吏꾩엯 ??湲곕낯 settle 媛?
# 濡쒕뵫 ?湲? ALT ?쇰꺼, 踰꾪봽 step 媛꾧꺽
# Arcane Sanctuary 吏꾩엯 吏곹썑 湲곕낯 settle 媛?# 濡쒕뵫 ?湲? ALT ?좉?, 踰꾪봽 step 媛꾧꺽
ARCANE_SANCTUARY_RATIO = (220 / 447, 498 / 597)
ARCANE_ENTRY_SETTLE = (2.4, 2.8)
ARCANE_LABELS_SETTLE = (0.28, 0.38)
ARCANE_BUFF_STEP_SETTLE = (0.35, 0.5)

# 怨좎젙 wing ?쒖꽌
# Summoner ?먯깋 ?쒖옉 湲곗?: north -> east -> south -> west
# 怨좎젙 wing ?쒖꽌
# Summoner ?먯깋 ?쒖옉 湲곗?: north -> east -> south -> west
ARCANE_WINGS: tuple[ArcaneWing, ...] = (
    ArcaneWing("north", "2 o'clock", 1),
    ArcaneWing("east", "4 o'clock", 2),
    ArcaneWing("south", "8 o'clock", 3),
    ArcaneWing("west", "10 o'clock", 4),
)

# 전역 탐색/이동 기준값
# hub zero point, 과거 단일 경로 조작 ratio, floor 보정 후보군
ARCANE_MONSTER_TEMPLATE_DIR = Path("assets/monster/act2/arcane_sanctuary")
ARCANE_MONSTER_THRESHOLD = 0.78
ARCANE_NORTH_TEST_TICK_SLEEP = (0.12, 0.18)
ARCANE_ZERO_POINT_CURSOR_RATIO = (0.50, 0.50)
ARCANE_HUB_CENTER_TEMPLATE_THRESHOLD = 0.66
ARCANE_NORTH_CURSOR_RATIO = (0.84, 0.24)
ARCANE_NEXT_PATH_CURSOR_RATIO = (0.12, 0.18)
ARCANE_RETURN_NORTH_CURSOR_RATIO = (0.88, 0.12)
ARCANE_FINAL_NORTH_BEND_CURSOR_RATIO = (0.90, 0.14)
ARCANE_FOUR_OCLOCK_CURSOR_RATIO = (0.92, 0.82)
ARCANE_MOVE_STEP_SETTLE = (0.001, 0.005)
ARCANE_FLOOR_SCORE_RADIUS = 18
ARCANE_FLOOR_CANDIDATE_OFFSETS = ((0.0, 0.0), (0.03, 0.0), (-0.03, 0.0), (0.0, 0.03), (0.0, -0.03), (0.05, -0.02), (-0.05, 0.02))
ARCANE_FOUR_OCLOCK_FLOOR_CANDIDATE_OFFSETS = ((0.0, 0.0), (0.02, 0.02), (0.04, 0.04), (0.06, 0.06), (-0.02, -0.02))

# Arcane 怨듭슜 諛⑺뼢蹂?open probe 3??
# probe媛 ?⑥씪 source of truth
# steering candidate???꾨옒?먯꽌 probe濡쒕????뚯깮 怨꾩궛

# Arcane 怨듭슜 諛⑺뼢 3??援ъ“
# north, west??probe 3?먯쑝濡쒕????뚯깮 怨꾩궛
# primary = probe ??踰덉㎏ ??
# sharp = probe 泥?踰덉㎏? ??踰덉㎏??以묒젏
# soft = probe ??踰덉㎏? ??踰덉㎏??以묒젏
# east, south???섎떒 status UI ?먯쑀 ?곸뿭??怨좊젮??蹂꾨룄 蹂댁젙 ?꾩슂
# Arcane 怨듯넻 諛⑺뼢 3??援ъ“
# absolute ?붾㈃ ratio 湲곗?
# north, west???쒕줈 醫뚯슦 ?移?# east, south???쒕줈 醫뚯슦 ?移?# east, south???섎떒 status UI ?뚰뵾 湲곗? 諛섏쁺
ARCANE_NORTH_DIRECTION_POINTS = {
    "primary": (11.0 / 12.0, 1.0 / 6.0),
    "sharp": (7.0 / 8.0, 1.0 / 6.0),
    "soft": (11.0 / 12.0, 1.0 / 4.0),
}
ARCANE_WEST_DIRECTION_POINTS = {
    "primary": (1.0 / 12.0, 1.0 / 6.0),
    "sharp": (1.0 / 8.0, 1.0 / 6.0),
    "soft": (1.0 / 12.0, 1.0 / 4.0),
}
ARCANE_EAST_DIRECTION_POINTS = {
    "primary": (11.0 / 12.0, 11.0 / 12.0),
    "sharp": (5.0 / 6.0, 2.0 / 3.0),
    "soft": (2.0 / 3.0, 5.0 / 6.0),
}
ARCANE_SOUTH_DIRECTION_POINTS = {
    "primary": (1.0 / 12.0, 11.0 / 12.0),
    "sharp": (1.0 / 6.0, 2.0 / 3.0),
    "soft": (1.0 / 3.0, 5.0 / 6.0),
}

# 諛⑺뼢蹂?open probe ??諛섍꼍 鍮꾩쑉
# north, west??湲곗〈 湲곗? ?좎?
# east, south???섎떒 status UI ?쇱엯??以꾩씠湲??꾪븳 ?묒? 諛섍꼍
# 諛⑺뼢 open probe ??諛섍꼍 鍮꾩쑉
# north, west??湲곗〈 湲곗? ?좎?
# east, south???섎떒 status UI ?욎엫??以꾩씠湲??꾪븳 ?묒? 諛섍꼍
ARCANE_NORTH_WEST_OPEN_CIRCLE_RADIUS_RATIO = 0.16
ARCANE_EAST_SOUTH_OPEN_CIRCLE_RADIUS_RATIO = 0.12

# ?쒗뵆由?留ㅼ묶 ?꾧퀎媛?臾띠쓬
# hover, star, north terminal, Summoner clue ?먯젙 湲곗?
# ?쒗뵆由?留ㅼ묶 ?꾧퀎媛?臾띠쓬
# hover, star, north terminal, Summoner clue ?먯젙 湲곗?
ARCANE_TELEPORTER_HOVER_THRESHOLD = 0.82
ARCANE_CHEST_HOVER_THRESHOLD = 0.82
ARCANE_STAR_THRESHOLD = 0.82
ARCANE_GOAL_CENTER_THRESHOLD = 0.8
ARCANE_NORTH_WAY_THRESHOLD = 0.78

# ?뺤껜/遺꾧린 ?먯젙 湲곗?
# ?붾㈃ 蹂?붾웾怨?floor score 李⑥씠 湲곕컲 遺꾧린 ?좏깮 湲곗?
# ?뺤껜/遺꾧린 ?먯젙 湲곗?
# ?붾㈃ 蹂?붾웾怨?floor score 李⑥씠 湲곕컲 遺꾧린 ?좏깮 湲곗?
ARCANE_STAR_STAGNANT_LIMIT = 2
ARCANE_FIRST_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_FIRST_FORK_STAGNANT_LIMIT = 2
ARCANE_SECOND_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_SECOND_FORK_STAGNANT_LIMIT = 2
ARCANE_BRANCH_SPAN_TRIM_TICKS = 3

# hover blocker ?뚰뵾 ?ㅽ봽??
# chest 怨꾩뿴 湲곗??????뚰뵾 ???섎굹濡??듭씪
# hover blocker ?뚰뵾 ?ㅽ봽??# chest 怨꾩뿴 湲곗? ?볦? ?뚰뵾???섎굹濡??듭씪
ARCANE_HOVER_NUDGE_OFFSETS = (
    (0.16, 0.0),
    (-0.16, 0.0),
    (0.0, 0.16),
    (0.0, -0.16),
    (0.14, 0.10),
    (-0.14, 0.10),
    (0.14, -0.10),
    (-0.14, -0.10),
)

# ?꾨컲遺/遺꾧린 諛붾떏 ?됯? 湲곗?
# dead-end 諛⑹?? final bend 吏꾪뻾 ?먮떒??湲곗?
# ?쇰컲遺/遺꾧린 諛붾떏 ?됯? 湲곗?
# dead-end 諛⑹?? final bend 吏꾪뻾 ?먮떒 湲곗?
ARCANE_PROGRESS_CHANGE_THRESHOLD = 8.0
ARCANE_PROGRESS_STAGNANT_LIMIT = 2
ARCANE_BRANCH_FLOOR_PREFERENCE_THRESHOLD = 90.0
ARCANE_BRANCH_FLOOR_SCORE_MARGIN = 8.0
ARCANE_FINAL_STAGE_TRAVEL_TICKS = 2
ARCANE_FINAL_STAGE_MIN_FOUR_OCLOCK_TICKS = 1
ARCANE_FINAL_QUARTER_FLOOR_THRESHOLD = 20.0


# Arcane 공용 템플릿을 session 필드로 preload
# hover blocker, hub, terminal, Summoner 탐색에서 공용으로 재사용한다.
def load_arcane_assets(session) -> None:
    # Arcane 공용 템플릿 preload
    # Arcane 怨듭슜 ?쒗뵆由?preload
    session._arcane_teleporter_hover_template = session._load_image(ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH)
    session._arcane_chest_hover_template = session._load_image(ARCANE_CHEST_HOVER_TEMPLATE_PATH)
    session._arcane_small_chest_hover_template = session._load_image(ARCANE_SMALL_CHEST_HOVER_TEMPLATE_PATH)
    session._arcane_small_locked_chest_hover_template = session._load_image(ARCANE_SMALL_LOCKED_CHEST_HOVER_TEMPLATE_PATH)
    session._arcane_small_coffin_hover_template = session._load_image(ARCANE_SMALL_COFFIN_HOVER_TEMPLATE_PATH)
    session._arcane_star_template = session._load_image(ARCANE_STAR_TEMPLATE_PATH)
    session._arcane_hub_center_template = session._load_image(ARCANE_HUB_CENTER_TEMPLATE_PATH)
    session._arcane_goal_center_template = session._load_image(ARCANE_GOAL_CENTER_TEMPLATE_PATH)
    session._arcane_monster_templates = None


# Arcane Sanctuary 吏꾩엯 吏곹썑 濡쒕뵫 settle
# Arcane Sanctuary 吏꾩엯 吏곹썑 濡쒕뵫 settle
def detect_arcane_terminal(session, frame: np.ndarray) -> str | None:
    if session._locate_template(frame, session._arcane_goal_center_template, ARCANE_GOAL_CENTER_THRESHOLD) is not None:
        return "goal_center"
    return None


def settle_arcane_entry(session, capture) -> None:
    session.events.put(session.event_class("info", "Summoner: waiting for Arcane Sanctuary to finish loading."))
    session._sleep_range(*ARCANE_ENTRY_SETTLE)


# ?꾩씠???쇰꺼 ?쒖떆瑜??꾪븳 ALT 1???낅젰
# Arcane 吏꾩엯 吏곹썑 loot label ?몄떇 ?덉젙???④퀎
# ?꾩씠???쇰꺼 ?쒖떆瑜??꾪븳 ALT 1???낅젰
# Arcane 吏꾩엯 吏곹썑 loot label ?몄떇 ?덉젙???④퀎
def ensure_arcane_item_labels(session) -> None:
    session.events.put(session.event_class("info", "Summoner: pressing ALT once to enable item labels in Arcane Sanctuary."))
    session._press_key("alt")
    session._sleep_range(*ARCANE_LABELS_SETTLE)


# wing 吏꾩엯 ??hub 以묒떖 ?ъ젙??
# hub template 以묒떖 ?곗꽑, ?ㅽ뙣 ??zero point fallback
# wing 吏꾩엯 ??hub 以묒떖??蹂댁젙
# hub template ?곗꽑, ?ㅽ뙣 ??zero point fallback
def prepare_arcane_hub_start(session, capture, wing_key: str) -> None:
    # ?쒖옉 ??hub 以묒떖 ?ъ젙??
    # ?쒗뵆由?留ㅼ묶 ?ㅽ뙣 ??zero point fallback ?ъ슜
    session.events.put(session.event_class("info", f"Summoner: centering at the Arcane hub before starting {wing_key}_go."))
    focus_ratio = ARCANE_ZERO_POINT_CURSOR_RATIO
    frame = capture.grab().frame
    hub_match = session._locate_template(frame, session._arcane_hub_center_template, ARCANE_HUB_CENTER_TEMPLATE_THRESHOLD)
    if hub_match is not None:
        center_x = hub_match.top_left[0] + (hub_match.width // 2)
        center_y = hub_match.top_left[1] + (hub_match.height // 2)
        frame_height, frame_width = frame.shape[:2]
        focus_ratio = (center_x / frame_width, center_y / frame_height)
        session.events.put(
            session.event_class(
                "info",
                f"Summoner: hub focus located at ratio=({focus_ratio[0]:.3f}, {focus_ratio[1]:.3f}) before {wing_key}_go.",
            )
        )
    session._arcane_hub_focus_ratio = focus_ratio
    session._aim_relative_ratio(capture, *focus_ratio, apply_jitter=False)
    session._sleep_range(*ARCANE_LABELS_SETTLE)
    actions = resolve_arcane_character_actions(session)
    if actions is None or not actions.movement_skill_key:
        raise RuntimeError(f"Arcane {wing_key}_go requires a character with movement_skill_key configured.")
    movement_state = MovementExecutionState()
    hub_action_phrase = apply_movement_intent(session, actions, movement_state, MOVEMENT_INTENT_REPOSITION)
    session.events.put(
        session.event_class(
            "info",
            f"Summoner: repositioning onto the Arcane hub focus at ratio=({focus_ratio[0]:.3f}, {focus_ratio[1]:.3f}) {hub_action_phrase}",
        )
    )
    session._sleep_range(*session.CLICK_SETTLE)
    release_movement_intent(session, actions, movement_state)


# 罹먮┃???ㅼ젙??pre-run buff ?쒖꽌 ?ъ깮
def run_arcane_pre_run_buffs(session) -> None:
    # 罹먮┃???ㅼ젙???ㅼ뼱 ?덈뒗 pre-run buff replay
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


# ?꾩옱 common ?④퀎?먯꽌??Arcane entry settle留??섑뻾
def prepare_arcane_entry(session, capture) -> None:
    settle_arcane_entry(session, capture)


# 踰꾪봽 ?≪뀡 ???湲??쒓컙 ?곸슜
# token蹂?媛쒕퀎 pause ?곗꽑, ?놁쑝硫?怨듭슜 settle ?ъ슜
def sleep_after_buff_action(session, actions, token: str) -> None:
    normalized = token.strip().lower()
    pause_seconds = actions.buff_action_pause_seconds.get(normalized)
    if pause_seconds is None:
        session._sleep_range(*ARCANE_BUFF_STEP_SETTLE)
        return
    session._sleep_range(pause_seconds, pause_seconds)


# Arcane controller belief state 珥덇린?붿? 泥?wing commit
def run_arcane_controller(session, capture) -> None:
    # 怨듭슜 belief state 珥덇린?붿? 泥?wing commit
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


# hub 湲곗? 珥덇린 belief state ?앹꽦
def build_initial_arcane_belief_state() -> ArcaneBeliefState:
    return ArcaneBeliefState(
        phase="hub",
        checked_wings=[],
        current_wing=None,
        remaining_wings=[wing.key for wing in ARCANE_WINGS],
        last_safe_anchor="hub_waypoint",
        danger_level="stable",
    )


# ?꾩쭅 諛⑸Ц?섏? ?딆? ?ㅼ쓬 wing 諛섑솚
# ARCANE_WINGS 怨좎젙 ?쒖꽌 湲곗? peek ?④퀎
def peek_next_arcane_wing(state: ArcaneBeliefState) -> ArcaneWing | None:
    for wing in ARCANE_WINGS:
        if wing.key in state.remaining_wings:
            return wing
    return None


# ?꾩옱 wing 吏꾩엯 commit怨?remaining 紐⑸줉 媛깆떊
# anchor瑜??대떦 wing 異쒓뎄 湲곗??쇰줈 ?꾪솚
def commit_arcane_wing(state: ArcaneBeliefState, wing: ArcaneWing) -> None:
    # ?꾩옱 wing 吏꾩엯 commit怨?remaining 紐⑸줉 媛깆떊
    state.phase = "wing_search"
    state.current_wing = wing.key
    state.last_safe_anchor = f"hub_exit_{wing.key}"
    state.remaining_wings = [candidate for candidate in state.remaining_wings if candidate != wing.key]


# belief state瑜??щ엺???쎄린 ?ъ슫 濡쒓렇 臾몄옄?대줈 異쒕젰
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


# Arcane monster template ?대뜑 ??png ?꾨? 濡쒕뱶
# ?뚯씪 stem??monster ?대쫫 key濡??ъ슜
def load_arcane_monster_templates(session) -> dict[str, np.ndarray]:
    # ?대뜑 ??png瑜?stem ?대쫫?쇰줈 濡쒕뱶
    templates: dict[str, np.ndarray] = {}
    if not ARCANE_MONSTER_TEMPLATE_DIR.exists():
        return templates
    for path in sorted(ARCANE_MONSTER_TEMPLATE_DIR.glob("*.png")):
        try:
            templates[path.stem] = session._load_image(path)
        except RuntimeError as exc:
            session.events.put(session.event_class("warning", f"Arcane monster template skipped: {path.name} ({exc})"))
    return templates


# ?꾩옱 ?꾨젅?꾩뿉??threshold瑜??섎뒗 理쒓퀬 score monster ?대쫫 諛섑솚
def scan_arcane_monsters(session, frame: np.ndarray) -> str | None:
    # threshold瑜??섎뒗 理쒓퀬 score monster ?대쫫 諛섑솚
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


# Arcane run????action profile ?좏깮
# summoner profile ?곗꽑, ?놁쑝硫?泥?罹먮┃??fallback
def resolve_arcane_character_actions(session):
    characters = getattr(session.config, "characters", {})
    if not characters:
        return None
    active_character = getattr(session.config, "active_character", None)
    if active_character in characters:
        return characters[active_character].actions
    for profile in characters.values():
        if profile.preferred_run_profile == "summoner":
            return profile.actions
    return next(iter(characters.values())).actions


# ?ㅼ젙 臾몄옄??token???ㅼ젣 ?낅젰 ?숈옉?쇰줈 蹂??
# click token? 留덉슦???낅젰, ?섎㉧吏??key ?낅젰
def execute_configured_action(session, token: str) -> None:
    # ?ㅼ젙 臾몄옄?댁쓣 ?ㅼ젣 ?낅젰?쇰줈 蹂??
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

