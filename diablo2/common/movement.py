from __future__ import annotations

from dataclasses import dataclass

from diablo2.common.config import CharacterActions


# 전 run 공통 화면 제약 메모
# 하단 상태창 UI가 실제 게임 월드 일부를 가리는 구조
# 기준 자산: assets/character/play/status.png
# east, south, south-east 계열 조준/경로 샘플링에서는
# 화면 하단 ratio를 곧바로 믿지 말고 UI 점유 영역 고려 필요
MOVEMENT_MODE_HOLD = "hold"
MOVEMENT_MODE_PRESS = "press"
MOVEMENT_INTENT_TRAVEL = "travel"
MOVEMENT_INTENT_REPOSITION = "reposition"


@dataclass
class MovementExecutionState:
    movement_key_held: bool = False


def apply_movement_intent(session, actions: CharacterActions, state: MovementExecutionState, intent: str) -> str:
    movement_key = actions.movement_skill_key
    if not movement_key:
        raise RuntimeError("Movement intent requires movement_skill_key.")

    mode = _resolve_movement_mode(actions, intent)
    if mode == MOVEMENT_MODE_HOLD:
        if not state.movement_key_held:
            session._hold_key_down(movement_key)
            state.movement_key_held = True
        return f"with movement_skill_key '{movement_key}' still held."

    if state.movement_key_held:
        session._hold_key_up(movement_key)
        state.movement_key_held = False
    session._press_key(movement_key)
    return f"and cast movement_skill_key '{movement_key}'."


def release_movement_intent(session, actions: CharacterActions, state: MovementExecutionState) -> None:
    movement_key = actions.movement_skill_key
    if movement_key and state.movement_key_held:
        session._hold_key_up(movement_key)
        state.movement_key_held = False


def _resolve_movement_mode(actions: CharacterActions, intent: str) -> str:
    raw_mode = actions.movement_reposition_mode if intent == MOVEMENT_INTENT_REPOSITION else actions.movement_travel_mode
    normalized = str(raw_mode).strip().lower()
    if normalized in (MOVEMENT_MODE_HOLD, MOVEMENT_MODE_PRESS):
        return normalized
    return MOVEMENT_MODE_PRESS if intent == MOVEMENT_INTENT_REPOSITION else MOVEMENT_MODE_HOLD
