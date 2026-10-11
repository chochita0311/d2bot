from __future__ import annotations

from dataclasses import dataclass
from types import MappingProxyType
from typing import Mapping

from diablo2.common.survival import ObservationStamp, _integer, _number, _text


WINGS = ("north", "east", "south", "west")
COMMANDS = frozenset(
    (
        "create_room",
        "enter_arcane",
        "prepare_field",
        "search_wing",
        "fight_boss",
        "inspect_loot",
        "return_hub",
        "return_act1",
        "restock",
        "exit_room",
    )
)
EVENTS = frozenset(
    (
        "unknown",
        "failed",
        "character_select",
        "act1_ready",
        "arcane_ready",
        "field_ready",
        "wing_empty_confirmed",
        "summoner_alive",
        "summoner_dead_confirmed",
        "loot_complete",
        "hub_ready",
        "restock_complete",
    )
)


@dataclass(frozen=True)
class CycleLimits:
    screen_age: float
    room_seconds: float
    total_seconds: float
    stage_seconds: Mapping[str, float]

    def __post_init__(self):
        for name in ("screen_age", "room_seconds", "total_seconds"):
            if _number(getattr(self, name), name) == 0:
                raise ValueError(f"{name} must be positive")
        if set(self.stage_seconds) != COMMANDS:
            raise ValueError("explicit timeout for every cycle command required")
        stages = dict(self.stage_seconds)
        for name, value in stages.items():
            if _number(value, name) == 0:
                raise ValueError(f"{name} timeout must be positive")
        # 호출자 사전의 변경으로 실행 중 상한이 바뀌지 않게 한다.
        object.__setattr__(self, "stage_seconds", MappingProxyType(stages))


@dataclass(frozen=True)
class CycleObservation:
    stamp: ObservationStamp
    generation: int
    event: str
    command_id: int | None = None
    keys_dropped: int = 0
    keys_acquired: int = 0
    reason: str = ""

    def __post_init__(self):
        if not isinstance(self.stamp, ObservationStamp):
            raise ValueError("observation stamp required")
        _integer(self.generation, "generation")
        if self.event not in EVENTS:
            raise ValueError("unsupported cycle event")
        if self.command_id is not None:
            _integer(self.command_id, "command_id", 1)
        _integer(self.keys_dropped, "keys_dropped")
        _integer(self.keys_acquired, "keys_acquired", maximum=self.keys_dropped)
        if self.event != "loot_complete" and (self.keys_dropped or self.keys_acquired):
            raise ValueError("key counts require confirmed loot observation")


@dataclass(frozen=True)
class CycleCommand:
    command_id: int
    kind: str
    room_id: str
    wing: str | None
    issued_at: float
    deadline: float
    source_sequence: int
    generation: int


@dataclass(frozen=True)
class CycleDirective:
    action: str
    reason: str
    command: CycleCommand | None = None


@dataclass(frozen=True)
class CycleResult:
    room_id: str
    room_entered: bool
    outcome: str
    reason: str
    checked_wings: tuple[str, ...]
    boss_wing: str | None
    keys_dropped: int
    keys_acquired: int


class SummonerCycle:
    """긍정 관찰만 소비하는 단일 owner planner. OS 입력은 adapter가 소유한다."""

    def __init__(self, character_id: str, room_ids: tuple[str, ...], limits: CycleLimits, *, started_at: float):
        self.character_id = _text(character_id, "character_id")
        if not isinstance(room_ids, tuple) or not room_ids:
            raise ValueError("explicit finite room IDs required")
        for room_id in room_ids:
            _text(room_id, "room_id")
        if len(set(room_ids)) != len(room_ids):
            raise ValueError("room IDs must be unique")
        if not isinstance(limits, CycleLimits):
            raise ValueError("cycle limits required")
        self.room_ids, self.limits = room_ids, limits
        self.started_at = self.room_started_at = self._last_now = _number(started_at, "started_at")
        self.room_index = 0
        self.pending: CycleCommand | None = None
        self.results: list[CycleResult] = []
        self.checked_wings: list[str] = []
        self.boss_wing: str | None = None
        self.keys_dropped = self.keys_acquired = 0
        self.done = False
        self._command_id = 0
        self._last_sequence = -1
        self._last_observed_at = started_at
        self._generation = 0
        self._wing_index = 0
        self._loot_complete = False
        self._room_entered = False
        self._terminal_reason = ""

    @property
    def room_id(self):
        return self.room_ids[self.room_index]

    def _record(self, outcome, reason):
        self.results.append(
            CycleResult(
                self.room_id,
                self._room_entered,
                outcome,
                reason,
                tuple(self.checked_wings),
                self.boss_wing,
                self.keys_dropped,
                self.keys_acquired,
            )
        )

    def _halt(self, reason):
        if not self.done:
            self._record("review_required", reason)
        self.done, self.pending, self._terminal_reason = True, None, reason
        return CycleDirective("release", reason)

    def _issue(self, kind, observation, now, wing=None):
        self._command_id += 1
        self.pending = CycleCommand(
            self._command_id,
            kind,
            self.room_id,
            wing,
            now,
            min(
                now + self.limits.stage_seconds[kind],
                self.room_started_at + self.limits.room_seconds,
                self.started_at + self.limits.total_seconds,
            ),
            observation.stamp.sequence,
            observation.generation,
        )
        return CycleDirective("request", kind, self.pending)

    def step(self, observation: CycleObservation | None, now: float, *, focused: bool, stopped: bool = False):
        _number(now, "now")
        if type(focused) is not bool or type(stopped) is not bool:
            raise ValueError("explicit focus/stop state required")
        if self.done:
            return CycleDirective("release", self._terminal_reason or "complete")
        if now < self._last_now:
            return self._halt("clock_regressed")
        self._last_now = now
        if stopped or not focused:
            return self._halt("stopped" if stopped else "focus_lost")
        if now >= self.started_at + self.limits.total_seconds:
            return self._halt("total_timeout")
        if now >= self.room_started_at + self.limits.room_seconds:
            return self._halt("room_timeout")
        if self.pending is not None and now >= self.pending.deadline:
            return self._halt("stage_timeout:" + self.pending.kind)
        if observation is None:
            return CycleDirective("wait" if self.pending is not None else "release", "waiting_observation")
        stamp = observation.stamp
        if stamp.character_id != self.character_id or stamp.room_id != self.room_id:
            return self._halt("wrong_context")
        if not 0 <= now - stamp.observed_at <= self.limits.screen_age:
            return self._halt("screen_stale")
        if stamp.sequence <= self._last_sequence or stamp.observed_at < self._last_observed_at or observation.generation < self._generation:
            return CycleDirective("wait", "observation_not_newer")
        self._last_sequence, self._last_observed_at, self._generation = stamp.sequence, stamp.observed_at, observation.generation
        if observation.event == "failed":
            return self._halt(observation.reason or "observed_failure")
        if self.pending is None:
            if observation.event == "character_select" and observation.command_id is None:
                return self._issue("create_room", observation, now)
            return CycleDirective("release", "initial_select_unconfirmed")
        command = self.pending
        if (
            observation.command_id != command.command_id
            or stamp.sequence <= command.source_sequence
            or stamp.observed_at < command.issued_at
            or observation.generation < command.generation
        ):
            return CycleDirective("wait", "command_unconfirmed")
        return self._advance(command, observation, now)

    def _advance(self, command, observation, now):
        event, kind = observation.event, command.kind
        transitions = {
            ("create_room", "act1_ready"): "enter_arcane",
            ("enter_arcane", "arcane_ready"): "prepare_field",
            ("return_act1", "act1_ready"): "restock",
            ("restock", "restock_complete"): "exit_room",
        }
        following = transitions.get((kind, event))
        if following is not None:
            if kind == "create_room":
                self._room_entered = True
            return self._issue(following, observation, now)
        if kind == "prepare_field" and event == "field_ready":
            return self._issue("search_wing", observation, now, WINGS[self._wing_index])
        if kind == "search_wing":
            if event == "summoner_alive":
                self.boss_wing = command.wing
                return self._issue("fight_boss", observation, now, command.wing)
            if event == "wing_empty_confirmed":
                self.checked_wings.append(command.wing)
                return self._issue("return_hub", observation, now, command.wing)
        if kind == "fight_boss" and event == "summoner_dead_confirmed":
            return self._issue("inspect_loot", observation, now, command.wing)
        if kind == "inspect_loot" and event == "loot_complete":
            self.keys_dropped, self.keys_acquired = observation.keys_dropped, observation.keys_acquired
            if self.keys_acquired != self.keys_dropped:
                return self._halt("key_pickup_incomplete")
            self._loot_complete = True
            return self._issue("return_hub", observation, now, command.wing)
        if kind == "return_hub" and event == "hub_ready":
            if self._loot_complete:
                return self._issue("return_act1", observation, now)
            self._wing_index += 1
            if self._wing_index == len(WINGS):
                return self._halt("summoner_not_found")
            return self._issue("search_wing", observation, now, WINGS[self._wing_index])
        if kind == "exit_room" and event == "character_select":
            self._record("normal", "act1_restock_and_exit_confirmed")
            if self.room_index + 1 == len(self.room_ids):
                self.done, self.pending = True, None
                return CycleDirective("release", "complete")
            self.room_index += 1
            self.room_started_at = now
            self.checked_wings, self.boss_wing = [], None
            self.keys_dropped = self.keys_acquired = self._wing_index = 0
            self._loot_complete = False
            self._room_entered = False
            return self._issue("create_room", observation, now)
        return CycleDirective("wait", "postcondition_unconfirmed")
