from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Mapping


def _number(value, name: str, minimum: float = 0.0, maximum: float | None = None) -> float:
    try:
        finite = not isinstance(value, bool) and isinstance(value, (int, float)) and math.isfinite(value)
    except OverflowError:
        finite = False
    if not finite:
        raise ValueError(f"{name} must be a finite number")
    if value < minimum or (maximum is not None and value > maximum):
        raise ValueError(f"{name} is out of range")
    return float(value)


def _integer(value, name: str, minimum: int = 0, maximum: int | None = None) -> int:
    if type(value) is not int or value < minimum or (maximum is not None and value > maximum):
        raise ValueError(f"{name} must be an integer in range")
    return value


def _text(value, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{name} must be a non-empty string")
    return value


def _fields(raw, allowed: set[str], required: set[str], name: str) -> None:
    if not isinstance(raw, dict):
        raise ValueError(f"{name} must be an object")
    if set(raw) - allowed or required - set(raw):
        raise ValueError(f"{name} has missing or unsupported fields")


@dataclass(frozen=True)
class TimedBuffPolicy:
    name: str
    key: str
    duration_seconds: float
    renew_before_seconds: float

    def __post_init__(self):
        _text(self.name, "buff.name")
        _text(self.key, "buff.key")
        duration = _number(self.duration_seconds, "buff.duration_seconds")
        margin = _number(self.renew_before_seconds, "buff.renew_before_seconds")
        if duration == 0 or margin >= duration:
            raise ValueError("buff renewal margin must be shorter than a positive duration")


@dataclass(frozen=True)
class BeltColumnPolicy:
    column: int
    key: str
    kind: str
    capacity: int
    target_count: int | None

    def __post_init__(self):
        _integer(self.column, "belt.column", 1, 4)
        _text(self.key, "belt.key")
        _text(self.kind, "belt.kind")
        capacity = _integer(self.capacity, "belt.capacity", 1, 4)
        if self.target_count is not None:
            _integer(self.target_count, "belt.target_count", 0, capacity)


@dataclass(frozen=True)
class CharacterSurvivalPolicy:
    life_potion_below: float
    mana_potion_below: float
    potion_kind: str
    replenish_at_or_below: int
    exit_on_empty: bool
    belt_toggle_key: str
    belt_columns: tuple[BeltColumnPolicy, ...]
    buffs: tuple[TimedBuffPolicy, ...] = ()

    def __post_init__(self):
        for name in ("life_potion_below", "mana_potion_below"):
            if _number(getattr(self, name), name, maximum=1.0) == 0:
                raise ValueError(f"{name} must be greater than zero")
        _text(self.potion_kind, "potion_kind")
        _text(self.belt_toggle_key, "belt_toggle_key")
        if type(self.exit_on_empty) is not bool:
            raise ValueError("exit_on_empty must be a boolean")
        object.__setattr__(self, "belt_columns", tuple(self.belt_columns))
        object.__setattr__(self, "buffs", tuple(self.buffs))
        if not self.belt_columns or any(not isinstance(column, BeltColumnPolicy) for column in self.belt_columns):
            raise ValueError("belt_columns must contain column policies")
        if any(not isinstance(buff, TimedBuffPolicy) for buff in self.buffs):
            raise ValueError("buffs must contain timed buff policies")
        if len({column.column for column in self.belt_columns}) != len(self.belt_columns):
            raise ValueError("belt column numbers must be unique")
        if len({column.key for column in self.belt_columns}) != len(self.belt_columns):
            raise ValueError("belt usage keys must be unique")
        if len({buff.name for buff in self.buffs}) != len(self.buffs):
            raise ValueError("buff names must be unique")
        potion_columns = [column for column in self.belt_columns if column.kind == self.potion_kind]
        if not potion_columns or any(column.target_count is None for column in potion_columns):
            raise ValueError("potion columns require explicit targets")
        potion_target = sum(column.target_count for column in potion_columns)
        if potion_target == 0:
            raise ValueError("potion target must be positive")
        _integer(self.replenish_at_or_below, "replenish_at_or_below", 0, potion_target)

    @classmethod
    def from_dict(cls, raw: dict) -> CharacterSurvivalPolicy:
        required = {
            "life_potion_below",
            "mana_potion_below",
            "potion_kind",
            "replenish_at_or_below",
            "exit_on_empty",
            "belt_toggle_key",
            "belt_columns",
        }
        _fields(raw, required | {"buffs"}, required, "survival")
        if not isinstance(raw["belt_columns"], list) or not isinstance(raw.get("buffs", []), list):
            raise ValueError("belt_columns and buffs must be lists")
        columns = []
        for column in raw["belt_columns"]:
            fields = {"column", "key", "kind", "capacity", "target_count"}
            _fields(column, fields, fields, "belt column")
            columns.append(BeltColumnPolicy(**column))
        buffs = []
        for buff in raw.get("buffs", []):
            fields = {"name", "key", "duration_seconds", "renew_before_seconds"}
            _fields(buff, fields, fields, "buff")
            buffs.append(TimedBuffPolicy(**buff))
        return cls(**{name: raw[name] for name in required - {"belt_columns"}}, belt_columns=tuple(columns), buffs=tuple(buffs))


@dataclass(frozen=True)
class ObservationStamp:
    character_id: str
    room_id: str
    sequence: int
    observed_at: float

    def __post_init__(self):
        _text(self.character_id, "character_id")
        _text(self.room_id, "room_id")
        _integer(self.sequence, "sequence")
        _number(self.observed_at, "observed_at")


@dataclass(frozen=True)
class SafetyObservation:
    stamp: ObservationStamp
    life_ratio: float | None
    mana_ratio: float | None
    location: str = "unknown"
    monsters_clear: bool | None = None

    def __post_init__(self):
        if not isinstance(self.stamp, ObservationStamp):
            raise ValueError("stamp must be an ObservationStamp")
        for name in ("life_ratio", "mana_ratio"):
            value = getattr(self, name)
            if value is not None:
                _number(value, name, maximum=1.0)
        if self.location not in ("field", "town", "unknown"):
            raise ValueError("location must be field, town or unknown")
        if self.monsters_clear is not None and type(self.monsters_clear) is not bool:
            raise ValueError("monsters_clear must be boolean or unknown")


@dataclass(frozen=True)
class SurvivalDecision:
    action: str
    reason: str
    potion_key: str | None = None
    buffs_due: tuple[str, ...] = ()
    deficits: tuple[tuple[int, int], ...] = ()
    replenishment_required: bool = False


class CharacterSurvivalState:
    """한 캐릭터/방의 확인된 상태만 보유하며 입력 없는 요청을 반환한다."""

    def __init__(self, character_id: str, room_id: str, policy: CharacterSurvivalPolicy | None):
        self._character_id = _text(character_id, "character_id")
        self._room_id = _text(room_id, "room_id")
        if policy is not None and not isinstance(policy, CharacterSurvivalPolicy):
            raise ValueError("policy must be CharacterSurvivalPolicy or None")
        self._policy = policy
        self._columns = {column.column: column for column in policy.belt_columns} if policy else {}
        self._targets = {number: column.target_count for number, column in self._columns.items()}
        self._counts: dict[int, int] | None = None
        self._belt_stamp: ObservationStamp | None = None
        self._buff_stamps: dict[str, ObservationStamp] = {}

    @property
    def character_id(self) -> str:
        return self._character_id

    @property
    def room_id(self) -> str:
        return self._room_id

    @property
    def policy(self) -> CharacterSurvivalPolicy | None:
        return self._policy

    def _same_context(self, stamp: ObservationStamp) -> bool:
        return stamp.character_id == self.character_id and stamp.room_id == self.room_id

    @staticmethod
    def _newer(stamp: ObservationStamp, previous: ObservationStamp | None) -> bool:
        return previous is None or (stamp.sequence > previous.sequence and stamp.observed_at >= previous.observed_at)

    def _accept_belt_stamp(self, stamp: ObservationStamp) -> bool:
        if not self.policy or not self._same_context(stamp) or not self._newer(stamp, self._belt_stamp):
            return False
        self._belt_stamp = stamp
        return True

    @property
    def belt_counts(self) -> dict[int, int] | None:
        return None if self._counts is None else dict(self._counts)

    @property
    def potions_remaining(self) -> int | None:
        if self._counts is None or self.policy is None:
            return None
        return sum(self._counts[number] for number, column in self._columns.items() if column.kind == self.policy.potion_kind)

    @property
    def next_buff_renewal_at(self) -> float | None:
        if self.policy is None or not self.policy.buffs:
            return None
        if any(buff.name not in self._buff_stamps for buff in self.policy.buffs):
            return None
        return min(
            self._buff_stamps[buff.name].observed_at + buff.duration_seconds - buff.renew_before_seconds for buff in self.policy.buffs
        )

    def observe_belt(self, stamp: ObservationStamp, contents: Mapping[int, tuple[str | None, int]]) -> bool:
        if not self._accept_belt_stamp(stamp):
            return False
        # 불일치를 합계로 숨기지 않고 다음 완전한 관찰까지 잔량을 미확인으로 둔다.
        self._counts = None
        if not isinstance(contents, Mapping) or set(contents) != set(self._columns) or any(type(number) is not int for number in contents):
            return False
        counts = {}
        for number, column in self._columns.items():
            value = contents[number]
            if not isinstance(value, (tuple, list)) or len(value) != 2:
                return False
            kind, count = value
            try:
                _integer(count, "observed count", 0, column.capacity)
            except ValueError:
                return False
            if count > 0 and kind != column.kind:
                return False
            counts[number] = count
        self._counts = counts
        for number, count in counts.items():
            if self._targets[number] is None:
                self._targets[number] = count
        return True

    def confirm_resource_change(self, stamp: ObservationStamp, column: int, kind: str, delta: int) -> bool:
        if not self._accept_belt_stamp(stamp):
            return False
        rule = self._columns.get(column) if type(column) is int else None
        if self._counts is None:
            return False
        if rule is None or kind != rule.kind or type(delta) is not int or delta == 0:
            self._counts = None
            return False
        count = self._counts[column] + delta
        if not 0 <= count <= rule.capacity:
            self._counts = None
            return False
        self._counts[column] = count
        return True

    def invalidate_belt(self, stamp: ObservationStamp) -> bool:
        if not self._accept_belt_stamp(stamp):
            return False
        self._counts = None
        return True

    def confirm_buff(self, stamp: ObservationStamp, name: str, location: str, monsters_clear: bool | None) -> bool:
        if not self.policy or not self._same_context(stamp) or name not in {buff.name for buff in self.policy.buffs}:
            return False
        if location != "field" or monsters_clear is not True or not self._newer(stamp, self._buff_stamps.get(name)):
            return False
        self._buff_stamps[name] = stamp
        return True

    def _due_buffs(self, now: float) -> tuple[str, ...]:
        due = []
        for buff in self.policy.buffs:
            cast = self._buff_stamps.get(buff.name)
            if cast is None or now >= cast.observed_at + buff.duration_seconds - buff.renew_before_seconds:
                due.append(buff.name)
        return tuple(due)

    def _deficits(self) -> tuple[tuple[int, int], ...]:
        return tuple(
            (number, target - self._counts[number])
            for number, target in sorted(self._targets.items())
            if target is not None and target > self._counts[number]
        )

    def decide(
        self, observation: SafetyObservation, now: float, *, max_observation_age_seconds: float, max_belt_age_seconds: float
    ) -> SurvivalDecision:
        now = _number(now, "now")
        for name, limit in (("max_observation_age_seconds", max_observation_age_seconds), ("max_belt_age_seconds", max_belt_age_seconds)):
            if _number(limit, name) == 0:
                raise ValueError(f"{name} must be positive")
        if self.policy is None:
            return SurvivalDecision("hold", "policy_unconfigured")
        stamp = observation.stamp
        if not self._same_context(stamp):
            return SurvivalDecision("hold", "wrong_context")
        if not 0 <= now - stamp.observed_at <= max_observation_age_seconds:
            return SurvivalDecision("hold", "observation_stale")
        # 상태보다 오래된 화면을 후속 입력의 근거로 사용하지 않는다.
        known_stamps = list(self._buff_stamps.values()) + ([self._belt_stamp] if self._belt_stamp else [])
        if any(stamp.sequence < known.sequence or stamp.observed_at < known.observed_at for known in known_stamps):
            return SurvivalDecision("hold", "observation_precedes_state")
        if self._counts is None or self._belt_stamp is None:
            return SurvivalDecision("hold", "belt_unknown")
        if now - self._belt_stamp.observed_at > max_belt_age_seconds:
            return SurvivalDecision("hold", "belt_stale")
        potions = self.potions_remaining
        deficits = self._deficits()
        replenish = potions <= self.policy.replenish_at_or_below
        if potions == 0:
            action = "exit" if self.policy.exit_on_empty else "hold"
            return SurvivalDecision(action, "potions_empty", deficits=deficits, replenishment_required=True)
        if observation.life_ratio is None or observation.mana_ratio is None:
            return SurvivalDecision("hold", "hud_unknown", deficits=deficits, replenishment_required=replenish)
        if observation.life_ratio < self.policy.life_potion_below or observation.mana_ratio < self.policy.mana_potion_below:
            column = next(
                column
                for number, column in sorted(self._columns.items())
                if column.kind == self.policy.potion_kind and self._counts[number]
            )
            return SurvivalDecision("use_potion", "resource_low", column.key, deficits=deficits, replenishment_required=replenish)
        due = self._due_buffs(now)
        if due:
            if observation.location != "field":
                return SurvivalDecision("hold", "buff_requires_field", buffs_due=due, deficits=deficits, replenishment_required=replenish)
            if observation.monsters_clear is not True:
                return SurvivalDecision("hold", "buff_unsafe", buffs_due=due, deficits=deficits, replenishment_required=replenish)
            return SurvivalDecision("buff", "buff_due", buffs_due=due, deficits=deficits, replenishment_required=replenish)
        if observation.location == "unknown":
            return SurvivalDecision("hold", "location_unknown", deficits=deficits, replenishment_required=replenish)
        return SurvivalDecision(
            "replenish" if replenish else "continue",
            "potions_low" if replenish else "ready",
            deficits=deficits,
            replenishment_required=replenish,
        )
