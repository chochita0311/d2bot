"""현재 발판의 검토된 시체를 연결하며 생존 관찰은 선택 증거로 남긴다."""

import math

from diablo2.common.encounters import LifeWitness, SceneContext, life_state
from diablo2.common.survival import ObservationStamp


class SummonerLifeTracker:
    MAX_LINK_SECONDS = 12
    MAX_TARGET_SHIFT = 120
    MAX_CORPSE_SHIFT = 24

    def __init__(self, character_id, room_id):
        self.character_id, self.room_id = character_id, room_id
        self.generation = 0
        self.alive, self.dead, self.alive_position, self.dead_position, self.anchor_kind = None, None, None, None, None
        self.last_stamp = None

    def moved(self):
        # F2 뒤에는 이동 전 좌표를 폐기하고 현재 발판의 외형부터 다시 관찰한다.
        self.generation += 1
        self.alive, self.dead, self.alive_position, self.dead_position, self.anchor_kind = None, None, None, None, None

    def observe(self, alive, corpse, platform, *, sequence, acquired_at, now):
        result = dict(state="unknown", generation=self.generation, sequence=sequence, acquired_at=acquired_at)
        anchor = getattr(platform, "ground_point", None)
        kind = getattr(platform, "evidence_kind", None)
        if not 0 <= now - acquired_at < 0.25 or anchor is None or kind is None or (alive is not None and corpse is not None):
            self.dead, self.dead_position = None, None
            return result
        if self.last_stamp is not None and (sequence <= self.last_stamp[0] or acquired_at <= self.last_stamp[1]):
            self.dead, self.dead_position = None, None
            return result
        self.last_stamp = sequence, acquired_at
        if kind != self.anchor_kind:
            self.alive, self.dead, self.alive_position, self.dead_position = None, None, None, None
        self.anchor_kind = kind
        context = SceneContext(
            ObservationStamp(self.character_id, self.room_id, sequence, acquired_at),
            "arcane-sanctuary",
            self.generation,
            (1922, 1140),
            (24, 55, 1900, 900),
        )
        target_id = self.room_id + ":summoner"
        if alive is not None:
            position = tuple(a - b for a, b in zip(alive.ground, anchor))
            self.alive = LifeWitness(context, target_id, "summoner", "alive", "linked-observation")
            self.alive_position, self.anchor_kind = position, kind
            self.dead, self.dead_position = None, None
            result.update(state=life_state(context, self.alive, now=now, max_age=0.25), asset_id=alive.asset_id, relative_ground=position)
            return result
        if corpse is None:
            self.dead, self.dead_position = None, None
            return result
        position = tuple(a - b for a, b in zip(corpse.ground, anchor))
        # 주변 노바로 먼저 죽을 수 있으므로 전용 생존 관찰/타겟팅을 사망의 전제로 삼지 않는다.
        linked_alive = (
            self.alive
            if (
                self.alive is not None
                and context.same_scene(self.alive.context)
                and self.alive.context.fresh(now, self.MAX_LINK_SECONDS)
                and math.hypot(*(a - b for a, b in zip(position, self.alive_position))) <= self.MAX_TARGET_SHIFT
            )
            else None
        )
        witness = LifeWitness(
            context,
            target_id,
            "summoner",
            "dead",
            "linked-observation",
            None if linked_alive is None else linked_alive.context.stamp.sequence,
        )
        if self.dead is not None and (
            not context.same_scene(self.dead.context)
            or not self.dead.context.fresh(now, self.MAX_LINK_SECONDS)
            or math.hypot(*(a - b for a, b in zip(position, self.dead_position))) > self.MAX_CORPSE_SHIFT
        ):
            self.dead, self.dead_position = None, None
        state = "dead" if self.dead is not None else "unknown"
        if self.dead is None:
            self.dead, self.dead_position = witness, position
        result.update(
            state=state,
            asset_id=corpse.asset_id,
            relative_ground=position,
            linked_alive_sequence=witness.linked_alive_sequence,
            first_dead_sequence=self.dead.context.stamp.sequence,
            confirmation_kind="current_platform_corpse_pair" if state == "dead" else None,
        )
        return result
