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


# Arcane Sanctuary 공용 템플릿 경로 모음.
# hover blocker, hub 정렬, 목표 지점 감지에 재사용한다.
ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/teleporter_when_hover.png")
ARCANE_SHRINE_HOVER_TEMPLATE_PATH = Path("assets/shrine/shrine_when_hover.png")
ARCANE_CHEST_HOVER_TEMPLATE_PATH = Path("assets/chests/act2/arcane_sanctuary/chest_when_hover.png")
ARCANE_SMALL_COFFIN_HOVER_TEMPLATE_PATH = Path("assets/chests/act2/arcane_sanctuary/small_coffin_when_hover.png")
ARCANE_TERMINAL_END_TEMPLATE_PATHS: tuple[tuple[str, Path], ...] = (
    ("chest", Path("assets/chests/act2/arcane_sanctuary/chest.png")),
    ("chest2", Path("assets/chests/act2/arcane_sanctuary/chest2.png")),
    ("chest3", Path("assets/chests/act2/arcane_sanctuary/chest3.png")),
    ("small_chest", Path("assets/chests/act2/arcane_sanctuary/small_chest.png")),
    ("small_coffin", Path("assets/chests/act2/arcane_sanctuary/small_coffin.png")),
)
ARCANE_STAR_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/star.png")
ARCANE_HUB_CENTER_TEMPLATE_PATH = Path("assets/waypoint/act2/hub_center.png")
ARCANE_GOAL_CENTER_TEMPLATE_PATH = Path("assets/waypoint/act2/goal_center.png")
ARCANE_GOAL_CENTER_COVERED_GOLD_TEMPLATE_PATH = Path("assets/waypoint/act2/goal_center_covered_gold.png")

# Arcane Sanctuary 진입 직후 공용 settle 값.
# 로딩 대기, ALT 라벨 표시, 버프 step 간격에 사용한다.
ARCANE_SANCTUARY_RATIO = (220 / 447, 498 / 597)
ARCANE_ENTRY_SETTLE = (2.4, 2.8)
ARCANE_LABELS_SETTLE = (0.28, 0.38)
ARCANE_BUFF_STEP_SETTLE = (0.35, 0.5)

# 고정 wing 탐색 순서.
# Summoner 탐색은 north -> east -> south -> west 순으로 시작한다.
ARCANE_WINGS: tuple[ArcaneWing, ...] = (
    ArcaneWing("north", "2 o'clock", 1),
    ArcaneWing("east", "4 o'clock", 2),
    ArcaneWing("south", "8 o'clock", 3),
    ArcaneWing("west", "10 o'clock", 4),
)

# 전역 탐색/이동 기준값.
# hub zero point, 경로 조작 ratio, floor 보정 후보군을 묶어 둔다.
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

# Arcane 공용 방향 후보 비율.
# probe가 기준 source of truth 역할을 하고, steering 후보는 여기에 맞춰 파생 계산한다.
# primary, sharp, soft는 같은 방향 안에서 서로 다른 조준 지점이다.
# east, south는 하단 UI 영역을 피하기 위해 별도 보정 비율을 사용한다.
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

# 방향별 open probe 반경 비율.
# north, west는 기존 기준을 유지하고,
# east, south는 하단 status UI 영향을 줄이기 위해 더 작은 반경을 쓴다.
ARCANE_NORTH_WEST_OPEN_CIRCLE_RADIUS_RATIO = 0.16
ARCANE_EAST_SOUTH_OPEN_CIRCLE_RADIUS_RATIO = 0.12

# 템플릿 매칭 임계값 모음.
# hover, star, terminal 같은 공용 감지에 사용한다.
ARCANE_TELEPORTER_HOVER_THRESHOLD = 0.82
ARCANE_SHRINE_HOVER_THRESHOLD = 0.82
ARCANE_CHEST_HOVER_THRESHOLD = 0.82
ARCANE_STAR_THRESHOLD = 0.82
ARCANE_GOAL_CENTER_THRESHOLD = 0.8
ARCANE_END_CHEST_THRESHOLD = 0.8
ARCANE_NORTH_WAY_THRESHOLD = 0.78

# 정체/분기 판단 기준.
# 화면 변화량과 floor score 차이를 기반으로 분기 선택과 정체 판단을 한다.
ARCANE_STAR_STAGNANT_LIMIT = 2
ARCANE_FIRST_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_FIRST_FORK_STAGNANT_LIMIT = 2
ARCANE_SECOND_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_SECOND_FORK_STAGNANT_LIMIT = 2
ARCANE_BRANCH_SPAN_TRIM_TICKS = 3

# hover blocker 회피 오프셋.
# upper-right를 우선으로 시도하고, 기존보다 조금 더 크게 벗어나도록 둔다.
ARCANE_HOVER_NUDGE_OFFSETS = (
    (0.18, -0.14),
    (0.20, 0.0),
    (0.0, -0.20),
    (0.18, 0.14),
    (-0.20, 0.0),
    (0.0, 0.20),
    (-0.18, -0.14),
    (-0.18, 0.14),
)

# 일반부/분기부 진행 판단 기준.
# dead-end 진입과 final bend 진행 여부를 결정할 때 사용한다.
ARCANE_PROGRESS_CHANGE_THRESHOLD = 8.0
ARCANE_PROGRESS_STAGNANT_LIMIT = 2
ARCANE_BRANCH_FLOOR_PREFERENCE_THRESHOLD = 90.0
ARCANE_BRANCH_FLOOR_SCORE_MARGIN = 8.0
ARCANE_FINAL_STAGE_TRAVEL_TICKS = 2
ARCANE_FINAL_STAGE_MIN_FOUR_OCLOCK_TICKS = 1
ARCANE_FINAL_QUARTER_FLOOR_THRESHOLD = 20.0


# Arcane 공용 템플릿을 session 필드로 preload 한다.
# hover blocker, hub, end/goal 감지에서 재사용한다.
def load_arcane_assets(session) -> None:
    session._arcane_teleporter_hover_template = session._load_image(ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH)
    session._arcane_shrine_hover_template = session._load_image(ARCANE_SHRINE_HOVER_TEMPLATE_PATH)
    session._arcane_chest_hover_template = session._load_image(ARCANE_CHEST_HOVER_TEMPLATE_PATH)
    session._arcane_small_coffin_hover_template = session._load_image(ARCANE_SMALL_COFFIN_HOVER_TEMPLATE_PATH)
    session._arcane_terminal_end_templates = tuple(
        (template_name, session._load_image(template_path)) for template_name, template_path in ARCANE_TERMINAL_END_TEMPLATE_PATHS
    )
    session._arcane_star_template = session._load_image(ARCANE_STAR_TEMPLATE_PATH)
    session._arcane_hub_center_template = session._load_image(ARCANE_HUB_CENTER_TEMPLATE_PATH)
    session._arcane_goal_center_template = session._load_image(ARCANE_GOAL_CENTER_TEMPLATE_PATH)
    session._arcane_goal_center_covered_gold_template = session._load_image(ARCANE_GOAL_CENTER_COVERED_GOLD_TEMPLATE_PATH)
    session._arcane_monster_templates = None


# Arcane end 목표 지점을 감지한다.
# goal_center 계열과 NEWS 공통 chest/coffin end 특징을 같은 end 판정으로 함께 본다.
# chest/coffin 계열은 5개 후보 중 3개 이상이 동시에 잡힐 때만 end로 인정한다.
def detect_arcane_end(session, frame: np.ndarray) -> str | None:
    if session._locate_template(frame, session._arcane_goal_center_template, ARCANE_GOAL_CENTER_THRESHOLD) is not None:
        return "goal_center"
    if session._locate_template(frame, session._arcane_goal_center_covered_gold_template, ARCANE_GOAL_CENTER_THRESHOLD) is not None:
        return "goal_center_covered_gold"

    matched_template_names: list[str] = []
    for template_name, template in getattr(session, "_arcane_terminal_end_templates", ()):
        if session._locate_template(frame, template, ARCANE_END_CHEST_THRESHOLD) is not None:
            matched_template_names.append(template_name)
    if len(matched_template_names) >= 3:
        return f"common_end:{','.join(matched_template_names)}"

    return None


# 현재 프레임에서 커서 hover 때문에 이동을 방해하는 오브젝트를 찾는다.
# chest 계열, shrine, teleporter를 구분해서 Arcane 공용 회피 동작에 쓴다.
def detect_arcane_hover_blocker(session, frame: np.ndarray) -> str | None:
    chest_hover_templates = (
        session._arcane_chest_hover_template,
        session._arcane_small_coffin_hover_template,
    )
    for chest_template in chest_hover_templates:
        if session._locate_template(frame, chest_template, ARCANE_CHEST_HOVER_THRESHOLD) is not None:
            return "chest"
    if session._locate_template(frame, session._arcane_shrine_hover_template, ARCANE_SHRINE_HOVER_THRESHOLD) is not None:
        return "shrine"
    if session._locate_template(frame, session._arcane_teleporter_hover_template, ARCANE_TELEPORTER_HOVER_THRESHOLD) is not None:
        return "teleporter"
    return None


def settle_arcane_entry(session, capture) -> None:
    session.events.put(session.event_class("info", "Summoner: waiting for Arcane Sanctuary to finish loading."))
    session._sleep_range(*ARCANE_ENTRY_SETTLE)


# 아이템 라벨 표시를 위해 ALT를 한 번 누른다.
# Arcane 진입 직후 loot label 인식 안정화 단계다.
def ensure_arcane_item_labels(session) -> None:
    session.events.put(session.event_class("info", "Summoner: pressing ALT once to enable item labels in Arcane Sanctuary."))
    session._press_key("alt")
    session._sleep_range(*ARCANE_LABELS_SETTLE)


# wing 진입 전에 hub 중심으로 시점을 맞춘다.
# hub template를 우선 사용하고, 실패하면 zero point 비율로 fallback 한다.
def prepare_arcane_hub_start(session, capture, wing_key: str) -> None:
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


# 캐릭터 설정에 들어 있는 pre-run buff 순서를 재생한다.
# 현재 선택된 캐릭터 액션 설정을 기준으로 동작한다.
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


# 현재 common 단계에서는 Arcane entry settle만 수행한다.
def prepare_arcane_entry(session, capture) -> None:
    settle_arcane_entry(session, capture)


# 버프 액션 뒤 대기 시간을 적용한다.
# token별 개별 pause가 있으면 우선하고, 없으면 공용 settle 값을 쓴다.
def sleep_after_buff_action(session, actions, token: str) -> None:
    normalized = token.strip().lower()
    pause_seconds = actions.buff_action_pause_seconds.get(normalized)
    if pause_seconds is None:
        session._sleep_range(*ARCANE_BUFF_STEP_SETTLE)
        return
    session._sleep_range(pause_seconds, pause_seconds)


# Arcane controller belief state를 초기화하고 첫 wing을 commit 한다.
# 이 함수는 공용 belief state를 만들고 첫 탐색 branch를 확정한다.
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


# hub 기준 초기 belief state를 만든다.
def build_initial_arcane_belief_state() -> ArcaneBeliefState:
    return ArcaneBeliefState(
        phase="hub",
        checked_wings=[],
        current_wing=None,
        remaining_wings=[wing.key for wing in ARCANE_WINGS],
        last_safe_anchor="hub_waypoint",
        danger_level="stable",
    )


# 아직 방문하지 않은 다음 wing을 반환한다.
# ARCANE_WINGS의 고정 순서를 기준으로 peek 한다.
def peek_next_arcane_wing(state: ArcaneBeliefState) -> ArcaneWing | None:
    for wing in ARCANE_WINGS:
        if wing.key in state.remaining_wings:
            return wing
    return None


# 현재 wing 진입을 commit 하고 remaining 목록을 갱신한다.
# anchor는 해당 wing 출구 기준 이름으로 바꾼다.
def commit_arcane_wing(state: ArcaneBeliefState, wing: ArcaneWing) -> None:
    state.phase = "wing_search"
    state.current_wing = wing.key
    state.last_safe_anchor = f"hub_exit_{wing.key}"
    state.remaining_wings = [candidate for candidate in state.remaining_wings if candidate != wing.key]


# belief state를 읽기 쉬운 로그 문자열로 출력한다.
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


# Arcane monster template 폴더의 png 파일을 모두 로드한다.
# 파일 stem을 monster 이름 key로 사용한다.
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


# 현재 프레임에서 threshold를 넘는 최고 score monster 이름을 반환한다.
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


# Arcane run에서 사용할 캐릭터 액션 프로필을 고른다.
# 현재 선택된 캐릭터를 우선하고, 없으면 summoner 선호 캐릭터, 그래도 없으면 첫 캐릭터로 fallback 한다.
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


# 설정 문자열 token을 실제 입력 동작으로 바꾼다.
# click token은 마우스 입력, 나머지는 key 입력으로 처리한다.
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

