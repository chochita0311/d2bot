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


# Arcane Sanctuary 템플릿 경로 묶음
# hover blocker, hub 정렬, Summoner 위치 판정용 자산
# Arcane Sanctuary 공통 템플릿 경로 묶음
# hover blocker, hub 정렬, Summoner 위치 판정 자산
ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/teleporter_when_hover.png")
ARCANE_CHEST_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/chest_when_hover.png")
ARCANE_SMALL_CHEST_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/small_chest_when_hover.png")
ARCANE_SMALL_LOCKED_CHEST_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/small_locked_chest_when_hover.png")
ARCANE_SMALL_COFFIN_HOVER_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/small_coffin_when_hover.png")
ARCANE_STAR_TEMPLATE_PATH = Path("assets/map/act2/arcane_sanctuary/star.png")
ARCANE_HUB_CENTER_TEMPLATE_PATH = Path("assets/waypoint/act2/hub_center.png")
ARCANE_GOAL_CENTER_TEMPLATE_PATH = Path("assets/waypoint/act2/goal_center.png")

# Arcane Sanctuary 진입 후 기본 settle 값
# 로딩 대기, ALT 라벨, 버프 step 간격
# Arcane Sanctuary 진입 직후 기본 settle 값
# 로딩 대기, ALT 토글, 버프 step 간격
ARCANE_SANCTUARY_RATIO = (220 / 447, 498 / 597)
ARCANE_ENTRY_SETTLE = (2.4, 2.8)
ARCANE_LABELS_SETTLE = (0.28, 0.38)
ARCANE_BUFF_STEP_SETTLE = (0.35, 0.5)

# 고정 wing 순서
# Summoner 탐색 시작 기준: north -> east -> south -> west
# 고정 wing 순서
# Summoner 탐색 시작 기준: north -> east -> south -> west
ARCANE_WINGS: tuple[ArcaneWing, ...] = (
    ArcaneWing("north", "2 o'clock", 1),
    ArcaneWing("east", "4 o'clock", 2),
    ArcaneWing("south", "8 o'clock", 3),
    ArcaneWing("west", "10 o'clock", 4),
)

# 전역 탐색/이동 기준점
# hub zero point, 과거 단일 경로 조준 ratio, floor 보정 후보군
# 전역 탐색/이동 기준값
# hub zero point, 과거 단일 경로 조준 ratio, floor 보정 후보군
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

# Arcane 공용 방향별 open probe 3점
# probe가 단일 source of truth
# steering candidate는 아래에서 probe로부터 파생 계산

# Arcane 공용 방향 3점 구조
# north, west는 probe 3점으로부터 파생 계산
# primary = probe 두 번째 점
# sharp = probe 첫 번째와 두 번째의 중점
# soft = probe 두 번째와 세 번째의 중점
# east, south는 하단 status UI 점유 영역을 고려한 별도 보정 필요
# Arcane 공통 방향 3점 구조
# absolute 화면 ratio 기준
# north, west는 서로 좌우 대칭
# east, south도 서로 좌우 대칭
# east, south는 하단 status UI 회피 기준 반영
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

# 방향별 open probe 원 반경 비율
# north, west는 기존 기준 유지
# east, south는 하단 status UI 혼입을 줄이기 위한 작은 반경
# 방향 open probe 원 반경 비율
# north, west는 기존 기준 유지
# east, south는 하단 status UI 섞임을 줄이기 위한 작은 반경
ARCANE_NORTH_WEST_OPEN_CIRCLE_RADIUS_RATIO = 0.16
ARCANE_EAST_SOUTH_OPEN_CIRCLE_RADIUS_RATIO = 0.12

# 템플릿 매칭 임계값 묶음
# hover, star, north terminal, Summoner clue 판정 기준
# 템플릿 매칭 임계값 묶음
# hover, star, north terminal, Summoner clue 판정 기준
ARCANE_TELEPORTER_HOVER_THRESHOLD = 0.82
ARCANE_CHEST_HOVER_THRESHOLD = 0.82
ARCANE_STAR_THRESHOLD = 0.82
ARCANE_GOAL_CENTER_THRESHOLD = 0.8
ARCANE_NORTH_WAY_THRESHOLD = 0.78

# 정체/분기 판정 기준
# 화면 변화량과 floor score 차이 기반 분기 선택 기준
# 정체/분기 판정 기준
# 화면 변화량과 floor score 차이 기반 분기 선택 기준
ARCANE_STAR_STAGNANT_LIMIT = 2
ARCANE_FIRST_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_FIRST_FORK_STAGNANT_LIMIT = 2
ARCANE_SECOND_FORK_PROGRESS_THRESHOLD = 5.0
ARCANE_SECOND_FORK_STAGNANT_LIMIT = 2
ARCANE_BRANCH_SPAN_TRIM_TICKS = 3

# hover blocker 회피 오프셋
# chest 계열 기준의 큰 회피 폭 하나로 통일
# hover blocker 회피 오프셋
# chest 계열 기준 넓은 회피폭 하나로 통일
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

# 후반부/분기 바닥 평가 기준
# dead-end 방지와 final bend 진행 판단용 기준
# 일반부/분기 바닥 평가 기준
# dead-end 방지와 final bend 진행 판단 기준
ARCANE_PROGRESS_CHANGE_THRESHOLD = 8.0
ARCANE_PROGRESS_STAGNANT_LIMIT = 2
ARCANE_BRANCH_FLOOR_PREFERENCE_THRESHOLD = 90.0
ARCANE_BRANCH_FLOOR_SCORE_MARGIN = 8.0
ARCANE_FINAL_STAGE_TRAVEL_TICKS = 2
ARCANE_FINAL_STAGE_MIN_FOUR_OCLOCK_TICKS = 1
ARCANE_FINAL_QUARTER_FLOOR_THRESHOLD = 20.0


# Arcane 공용 템플릿을 session 필드로 preload
# hover blocker, hub, terminal, Summoner 탐색에서 재사용
# Arcane 공통 템플릿 preload
# hover blocker, hub, terminal, Summoner 탐색에서 재사용
def load_arcane_assets(session) -> None:
    # Arcane 공용 템플릿 preload
    session._arcane_teleporter_hover_template = session._load_image(ARCANE_TELEPORTER_HOVER_TEMPLATE_PATH)
    session._arcane_chest_hover_template = session._load_image(ARCANE_CHEST_HOVER_TEMPLATE_PATH)
    session._arcane_small_chest_hover_template = session._load_image(ARCANE_SMALL_CHEST_HOVER_TEMPLATE_PATH)
    session._arcane_small_locked_chest_hover_template = session._load_image(ARCANE_SMALL_LOCKED_CHEST_HOVER_TEMPLATE_PATH)
    session._arcane_small_coffin_hover_template = session._load_image(ARCANE_SMALL_COFFIN_HOVER_TEMPLATE_PATH)
    session._arcane_star_template = session._load_image(ARCANE_STAR_TEMPLATE_PATH)
    session._arcane_hub_center_template = session._load_image(ARCANE_HUB_CENTER_TEMPLATE_PATH)
    session._arcane_goal_center_template = session._load_image(ARCANE_GOAL_CENTER_TEMPLATE_PATH)
    session._arcane_monster_templates = None


# Arcane Sanctuary 진입 직후 로딩 settle
# Arcane Sanctuary 진입 직후 로딩 settle
def detect_arcane_terminal(session, frame: np.ndarray) -> str | None:
    if session._locate_template(frame, session._arcane_goal_center_template, ARCANE_GOAL_CENTER_THRESHOLD) is not None:
        return "goal_center"
    return None


def settle_arcane_entry(session, capture) -> None:
    session.events.put(session.event_class("info", "Summoner: waiting for Arcane Sanctuary to finish loading."))
    session._sleep_range(*ARCANE_ENTRY_SETTLE)


# 아이템 라벨 표시를 위한 ALT 1회 입력
# Arcane 진입 직후 loot label 인식 안정화 단계
# 아이템 라벨 표시를 위한 ALT 1회 입력
# Arcane 진입 직후 loot label 인식 안정화 단계
def ensure_arcane_item_labels(session) -> None:
    session.events.put(session.event_class("info", "Summoner: pressing ALT once to enable item labels in Arcane Sanctuary."))
    session._press_key("alt")
    session._sleep_range(*ARCANE_LABELS_SETTLE)


# wing 진입 전 hub 중심 재정렬
# hub template 중심 우선, 실패 시 zero point fallback
# wing 진입 전 hub 중심점 보정
# hub template 우선, 실패 시 zero point fallback
def prepare_arcane_hub_start(session, capture, wing_key: str) -> None:
    # 시작 전 hub 중심 재정렬
    # 템플릿 매칭 실패 시 zero point fallback 사용
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


# 캐릭터 설정의 pre-run buff 순서 재생
def run_arcane_pre_run_buffs(session) -> None:
    # 캐릭터 설정에 들어 있는 pre-run buff replay
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


# 현재 common 단계에서는 Arcane entry settle만 수행
def prepare_arcane_entry(session, capture) -> None:
    settle_arcane_entry(session, capture)


# 버프 액션 뒤 대기 시간 적용
# token별 개별 pause 우선, 없으면 공용 settle 사용
def sleep_after_buff_action(session, actions, token: str) -> None:
    normalized = token.strip().lower()
    pause_seconds = actions.buff_action_pause_seconds.get(normalized)
    if pause_seconds is None:
        session._sleep_range(*ARCANE_BUFF_STEP_SETTLE)
        return
    session._sleep_range(pause_seconds, pause_seconds)


# Arcane controller belief state 초기화와 첫 wing commit
def run_arcane_controller(session, capture) -> None:
    # 공용 belief state 초기화와 첫 wing commit
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


# hub 기준 초기 belief state 생성
def build_initial_arcane_belief_state() -> ArcaneBeliefState:
    return ArcaneBeliefState(
        phase="hub",
        checked_wings=[],
        current_wing=None,
        remaining_wings=[wing.key for wing in ARCANE_WINGS],
        last_safe_anchor="hub_waypoint",
        danger_level="stable",
    )


# 아직 방문하지 않은 다음 wing 반환
# ARCANE_WINGS 고정 순서 기준 peek 단계
def peek_next_arcane_wing(state: ArcaneBeliefState) -> ArcaneWing | None:
    for wing in ARCANE_WINGS:
        if wing.key in state.remaining_wings:
            return wing
    return None


# 현재 wing 진입 commit과 remaining 목록 갱신
# anchor를 해당 wing 출구 기준으로 전환
def commit_arcane_wing(state: ArcaneBeliefState, wing: ArcaneWing) -> None:
    # 현재 wing 진입 commit과 remaining 목록 갱신
    state.phase = "wing_search"
    state.current_wing = wing.key
    state.last_safe_anchor = f"hub_exit_{wing.key}"
    state.remaining_wings = [candidate for candidate in state.remaining_wings if candidate != wing.key]


# belief state를 사람이 읽기 쉬운 로그 문자열로 출력
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


# Arcane monster template 폴더 내 png 전부 로드
# 파일 stem을 monster 이름 key로 사용
def load_arcane_monster_templates(session) -> dict[str, np.ndarray]:
    # 폴더 내 png를 stem 이름으로 로드
    templates: dict[str, np.ndarray] = {}
    if not ARCANE_MONSTER_TEMPLATE_DIR.exists():
        return templates
    for path in sorted(ARCANE_MONSTER_TEMPLATE_DIR.glob("*.png")):
        try:
            templates[path.stem] = session._load_image(path)
        except RuntimeError as exc:
            session.events.put(session.event_class("warning", f"Arcane monster template skipped: {path.name} ({exc})"))
    return templates


# 현재 프레임에서 threshold를 넘는 최고 score monster 이름 반환
def scan_arcane_monsters(session, frame: np.ndarray) -> str | None:
    # threshold를 넘는 최고 score monster 이름 반환
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


# Arcane run에 쓸 action profile 선택
# summoner profile 우선, 없으면 첫 캐릭터 fallback
def resolve_arcane_character_actions(session):
    # summoner profile 우선, 없으면 첫 캐릭터 fallback
    characters = getattr(session.config, "characters", {})
    if not characters:
        return None
    if "abyss_knight" in characters:
        return characters["abyss_knight"].actions
    for profile in characters.values():
        if profile.preferred_run_profile == "summoner":
            return profile.actions
    return next(iter(characters.values())).actions


# 설정 문자열 token을 실제 입력 동작으로 변환
# click token은 마우스 입력, 나머지는 key 입력
def execute_configured_action(session, token: str) -> None:
    # 설정 문자열을 실제 입력으로 변환
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
