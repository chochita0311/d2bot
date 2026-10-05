from __future__ import annotations

from dataclasses import dataclass
import math

from diablo2.common.survival import ObservationStamp, _integer, _number, _text


def _point(value, size: tuple[int, int], name: str) -> tuple[float, float]:
    if not isinstance(value, (tuple, list)) or len(value) != 2:
        raise ValueError(f"{name} requires x/y")
    return tuple(_number(v, name, maximum=limit - 1) for v, limit in zip(value, size))


@dataclass(frozen=True)
class SceneContext:
    stamp: ObservationStamp
    region_id: str
    movement_generation: int
    frame_size: tuple[int, int]
    viewport: tuple[int, int, int, int]

    def __post_init__(self):
        if not isinstance(self.stamp, ObservationStamp):
            raise ValueError("stamp must be an ObservationStamp")
        _text(self.region_id, "region_id")
        _integer(self.movement_generation, "movement_generation")
        if len(self.frame_size) != 2 or len(self.viewport) != 4:
            raise ValueError("invalid frame/viewport dimensions")
        object.__setattr__(self, "frame_size", tuple(self.frame_size))
        object.__setattr__(self, "viewport", tuple(self.viewport))
        width, height = (_integer(v, "frame_size", 1) for v in self.frame_size)
        x0, y0, x1, y1 = (_integer(v, "viewport") for v in self.viewport)
        if not (x0 < x1 <= width and y0 < y1 <= height):
            raise ValueError("viewport must be inside the native frame")

    def same_scene(self, other: SceneContext) -> bool:
        return isinstance(other, SceneContext) and (
            self.stamp.character_id,
            self.stamp.room_id,
            self.region_id,
            self.movement_generation,
            self.frame_size,
            self.viewport,
        ) == (other.stamp.character_id, other.stamp.room_id, other.region_id, other.movement_generation, other.frame_size, other.viewport)

    def fresh(self, now: float, max_age: float) -> bool:
        _number(now, "now")
        _number(max_age, "max_age")
        return 0 <= now - self.stamp.observed_at <= max_age


@dataclass(frozen=True)
class VisualCandidate:
    context: SceneContext
    asset_id: str
    kind: str
    category: str
    appearance: str
    score: float
    box: tuple[int, int, int, int]
    ground: tuple[float, float] | None

    def __post_init__(self):
        if not isinstance(self.context, SceneContext):
            raise ValueError("candidate context required")
        for name in ("asset_id", "kind"):
            _text(getattr(self, name), name)
        if self.category not in ("monster", "monster-group", "landmark"):
            raise ValueError("unsupported candidate category")
        if self.appearance not in ("alive", "corpse", "landmark"):
            raise ValueError("unsupported appearance")
        _number(self.score, "score", maximum=1)
        if len(self.box) != 4:
            raise ValueError("box requires four coordinates")
        x0, y0, x1, y1 = (_integer(v, "box") for v in self.box)
        vx0, vy0, vx1, vy1 = self.context.viewport
        if not (vx0 <= x0 < x1 <= vx1 and vy0 <= y0 < y1 <= vy1):
            raise ValueError("candidate must be inside the observed viewport")
        object.__setattr__(self, "box", tuple(self.box))
        if self.ground is not None:
            ground = _point(self.ground, self.context.frame_size, "ground")
            if not (x0 <= ground[0] < x1 and y0 <= ground[1] < y1):
                raise ValueError("ground must be inside candidate box")
            object.__setattr__(self, "ground", ground)


@dataclass(frozen=True)
class CombatRadius:
    enter: float
    leave: float

    def __post_init__(self):
        if not (0 < _number(self.enter, "enter", maximum=1) < _number(self.leave, "leave", maximum=1)):
            raise ValueError("radius requires 0 < enter < leave <= 1")


@dataclass(frozen=True)
class Proximity:
    state: str
    distance: float | None
    reason: str
    target_id: str | None


def proximity(
    candidate: VisualCandidate,
    current: SceneContext,
    player_ground: tuple[float, float] | None,
    radius: CombatRadius,
    *,
    now: float,
    max_age: float,
    target_id: str | None,
    previous: Proximity | None = None,
    previous_context: SceneContext | None = None,
) -> Proximity:
    # 이전 화면의 근접 상태는 같은 장면의 직전 관찰에만 유지한다.
    if not isinstance(radius, CombatRadius):
        raise ValueError("explicit combat radius required")
    if target_id is not None:
        _text(target_id, "target_id")
    if candidate.context != current or not current.fresh(now, max_age):
        return Proximity("unknown", None, "stale-or-different-frame", target_id)
    if candidate.category != "monster" or candidate.appearance != "alive":
        return Proximity("unknown", None, "not-an-alive-monster-candidate", target_id)
    if player_ground is None or candidate.ground is None:
        return Proximity("unknown", None, "ground-not-observed", target_id)
    player = _point(player_ground, current.frame_size, "player_ground")
    vx0, vy0, vx1, vy1 = current.viewport
    if not (vx0 <= player[0] < vx1 and vy0 <= player[1] < vy1):
        return Proximity("unknown", None, "player-outside-viewport", target_id)
    width, height = current.frame_size
    distance = math.hypot((candidate.ground[0] - player[0]) / width, (candidate.ground[1] - player[1]) / height)
    retain = (
        previous is not None
        and previous.state == "near"
        and target_id is not None
        and previous.target_id == target_id
        and previous_context is not None
        and current.same_scene(previous_context)
        and previous_context.stamp.sequence < current.stamp.sequence
        and previous_context.stamp.observed_at < current.stamp.observed_at
        and previous_context.fresh(now, max_age)
    )
    if distance <= radius.enter or (retain and distance < radius.leave):
        return Proximity("near", distance, "candidate-radius", target_id)
    if distance >= radius.leave:
        return Proximity("far", distance, "candidate-radius", target_id)
    return Proximity("boundary", distance, "candidate-radius", target_id)


@dataclass(frozen=True)
class LifeWitness:
    context: SceneContext
    target_id: str
    kind: str
    state: str
    provenance: str
    linked_alive_sequence: int | None = None

    def __post_init__(self):
        if not isinstance(self.context, SceneContext):
            raise ValueError("witness context required")
        _text(self.target_id, "target_id")
        _text(self.kind, "kind")
        if self.state not in ("alive", "dead", "unknown"):
            raise ValueError("unsupported life state")
        if self.provenance not in ("human-reviewed", "linked-observation", "template-candidate"):
            raise ValueError("unsupported witness provenance")
        if self.linked_alive_sequence is not None:
            _integer(self.linked_alive_sequence, "linked_alive_sequence")


def life_state(
    current: SceneContext,
    witness: LifeWitness | None,
    *,
    now: float,
    max_age: float,
    previous_alive: LifeWitness | None = None,
    first_dead: LifeWitness | None = None,
) -> str:
    # 템플릿/미검출/노바 효과로 개체의 사망을 확정하지 않는다.
    if not current.fresh(now, max_age) or witness is None or witness.context != current or witness.provenance == "template-candidate":
        return "unknown"
    if witness.state == "alive":
        return "alive"
    if witness.state != "dead" or previous_alive is None or first_dead is None:
        return "unknown"
    if previous_alive.state != "alive" or first_dead.state != "dead":
        return "unknown"
    for item in (previous_alive, first_dead):
        if (
            item.provenance == "template-candidate"
            or item.target_id != witness.target_id
            or item.kind != witness.kind
            or not current.same_scene(item.context)
            or not item.context.fresh(now, max_age)
        ):
            return "unknown"
    stamps = [item.context.stamp for item in (previous_alive, first_dead, witness)]
    if not (
        stamps[0].sequence < stamps[1].sequence < stamps[2].sequence
        and stamps[0].observed_at < stamps[1].observed_at < stamps[2].observed_at
    ):
        return "unknown"
    if first_dead.linked_alive_sequence != stamps[0].sequence or witness.linked_alive_sequence != stamps[0].sequence:
        return "unknown"
    return "dead"
