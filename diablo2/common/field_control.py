from __future__ import annotations

import threading
from collections import deque
from dataclasses import dataclass

from diablo2.common.casting import CastingEstimate
from diablo2.common.survival import CharacterSurvivalState, ObservationStamp, SafetyObservation, _number


@dataclass(frozen=True)
class FieldLimits:
    screen_age: float
    belt_age: float
    combat_seconds: float
    arrival_seconds: float
    potion_confirmation_seconds: float

    def __post_init__(self):
        for name in self.__dataclass_fields__:
            if _number(getattr(self, name), name) == 0:
                raise ValueError(f"{name} must be positive")


@dataclass(frozen=True)
class FieldObservation:
    safety: SafetyObservation
    belt_closed: bool | None
    enemy: str = "unknown"
    travel_aim: tuple[int, int] | None = None
    reachable: bool | None = None
    arrived: bool | None = None
    loot_checked: bool = False
    hunt_aim: tuple[int, int] | None = None

    def __post_init__(self):
        if not isinstance(self.safety, SafetyObservation):
            raise ValueError("safety observation required")
        if self.enemy not in ("near", "far", "clear", "unknown"):
            raise ValueError("enemy must be admitted proximity/clear evidence")
        for value in (self.belt_closed, self.reachable, self.arrived):
            if value is not None and type(value) is not bool:
                raise ValueError("explicit boolean or unknown required")
        if type(self.loot_checked) is not bool:
            raise ValueError("loot_checked must be boolean")
        for point in (self.travel_aim, self.hunt_aim):
            if point is not None and (not isinstance(point, tuple) or len(point) != 2 or any(type(v) is not int or v < 0 for v in point)):
                raise ValueError("aim must be a nonnegative absolute integer point")
        if self.enemy == "clear" and self.safety.monsters_clear is not True:
            raise ValueError("clear requires positive current safety evidence")
        if self.enemy != "clear" and self.safety.monsters_clear is True:
            raise ValueError("unresolved or visible enemies cannot be a clear buff site")


@dataclass(frozen=True)
class FieldDirective:
    action: str
    reason: str
    key: str | None = None
    aim: tuple[int, int] | None = None
    deadline: float | None = None
    hunt_visible: bool = False

    def __post_init__(self):
        if self.action not in ("travel", "combat", "hold", "use_potion", "buff", "exit", "inspect_loot"):
            raise ValueError("unsupported field action")
        if self.deadline is not None:
            _number(self.deadline, "input deadline")
        if self.action == "use_potion" and (not isinstance(self.key, str) or not self.key.strip()):
            raise ValueError("explicit potion key required")


class AdaptiveTeleportCadence:
    """참고 프레임은 하한이며 확인된 도착 지연만 조준 간격에 반영한다."""

    def __init__(self, estimate: CastingEstimate, arrival_timeout: float):
        if estimate.action != "estimate" or estimate.frames is None:
            raise ValueError("configured class/FCR reference required")
        self.reference_seconds = estimate.frames / 25.0
        self.timeout = _number(arrival_timeout, "arrival_timeout")
        if self.timeout <= self.reference_seconds:
            raise ValueError("arrival timeout must exceed reference animation")
        self.interval = self.reference_seconds
        self._sent_at: float | None = None
        self._aim: tuple[int, int] | None = None
        self._next_aim_at = 0.0

    @property
    def deadline(self):
        return None if self._sent_at is None else self._sent_at + self.timeout

    def confirm_arrival(self, now: float) -> None:
        if self._sent_at is not None and self._sent_at < now < self.deadline:
            delay = now - self._sent_at
            self.interval = min(self.timeout, max(self.reference_seconds, 0.75 * self.interval + 0.25 * delay))
            self._sent_at = None

    def reset(self) -> None:
        self._sent_at = None
        self._aim = None
        self._next_aim_at = 0.0

    def aim(self, point: tuple[int, int], now: float):
        if self._sent_at is not None:
            # 조준 갱신으로 미확인 도착의 상한을 계속 뒤로 미루지 않는다.
            return self._aim
        if now < self._next_aim_at:
            return self._aim
        self._aim = point
        self._sent_at = now
        self._next_aim_at = now + self.interval
        return point


class HeldFieldInput:
    """한 backend의 유지 입력을 소유하고 해제 실패도 잊지 않는다."""

    def __init__(self, backend, movement_key: str, attack_key: str, *, clock=None, repeat_seconds=None, aim_settle_seconds=0):
        if (
            any(not isinstance(key, str) or not key.strip() for key in (movement_key, attack_key))
            or movement_key.lower() == attack_key.lower()
        ):
            raise ValueError("distinct movement and attack keys required")
        self.backend = backend
        self.clock = clock
        if repeat_seconds is not None and _number(repeat_seconds, "held key repeat interval") <= 0:
            raise ValueError("positive held key repeat interval required")
        self.repeat_seconds, self.pressed_at = repeat_seconds, None
        self.aim_settle_seconds = _number(aim_settle_seconds, "held aim settling interval")
        self.pointer, self.aimed_at = None, float("-inf")
        self.pending_key, self.cast_pointer = None, None
        self.keys = (movement_key.lower(), attack_key.lower())
        self.dispatches = {key: 0 for key in self.keys}
        self.last_dispatch_at = None
        self.held: set[str] = set()
        self.deadline: float | None = None
        self.closed = False
        self.fault: Exception | None = None
        self._lock = threading.RLock()

    def release(self, *, preserve_aim=False) -> None:
        with self._lock:
            self.deadline = None
            self.pressed_at = None
            # lease 만료는 키를 해제한다. 같은 조준의 재승인은 실제 포인터를 재확인할 수 있을 때만 허용한다.
            if not preserve_aim or not callable(getattr(self.backend, "pointer_matches", None)):
                self.pointer, self.aimed_at = None, float("-inf")
            self.pending_key = None
            errors = []
            for key in tuple(self.held):
                try:
                    self.backend.key_up(key)
                    self.held.discard(key)
                except Exception as exc:
                    errors.append(exc)
            if errors:
                self.closed = True
                self.fault = errors[0]
                raise errors[0]

    def close(self) -> None:
        with self._lock:
            self.closed = True
            self.release()

    def snapshot(self):
        with self._lock:
            return dict(
                held=tuple(sorted(self.held)),
                pending=self.pending_key,
                dispatches=dict(self.dispatches),
                last_dispatch_at=self.last_dispatch_at,
                cast_pointer=self.cast_pointer,
                pointer=self.pointer,
            )

    def renew_lease(self, key, pointer, deadline, now):
        """현재 유지 중인 같은 조준만 새 관찰로 연장한다. 해제된 키를 다시 누르지 않는다."""
        with self._lock:
            if self.closed or self.held != {key} or self.pointer != pointer or self.deadline is None:
                return False
            if now >= self.deadline or now >= deadline:
                self.release(preserve_aim=True)
                return False
            matches = getattr(self.backend, "pointer_matches", None)
            if not callable(matches) or not matches(pointer):
                self.release()
                return False
            checked = self.clock() if self.clock is not None else now
            if checked >= self.deadline or checked >= deadline:
                self.release(preserve_aim=True)
                return False
            self.deadline = deadline
            return True

    def apply(self, directive: FieldDirective, now: float) -> None:
        with self._lock:
            if self.closed:
                self.release()
                return
            try:
                if directive.action not in ("travel", "combat"):
                    self.release()
                    if directive.action == "use_potion":
                        self.backend.press(directive.key)
                    return
                desired = self.keys[0 if directive.action == "travel" else 1]
                if directive.deadline is None or now >= directive.deadline:
                    self.release()
                    return
                if self.held and self.held != {desired}:
                    self.release()
                matches = getattr(self.backend, "pointer_matches", None)
                if self.pointer is not None and callable(matches) and not matches(self.pointer):
                    self.release()
                    if directive.aim is None:
                        return
                if directive.aim is not None and directive.aim != self.pointer:
                    self.backend.move(*directive.aim)
                    self.pointer = directive.aim
                    self.aimed_at = self.clock() if self.clock is not None else now
                # 포인터 이동 중 원본 화면의 유효 시간이 끝났으면 새 유지 입력을 시작하지 않는다.
                if self.clock is not None and self.clock() >= directive.deadline:
                    self.release()
                    return
                self.pending_key = desired
                self.deadline = directive.deadline
                self._press_if_ready(now)
            except Exception as exc:
                self.closed = True
                self.fault = exc
                try:
                    self.release()
                finally:
                    raise

    def _press_if_ready(self, now):
        desired = self.pending_key
        repeated = self.repeat_seconds is not None and self.pressed_at is not None and now - self.pressed_at >= self.repeat_seconds
        if desired is not None and now - self.aimed_at >= self.aim_settle_seconds and (desired not in self.held or repeated):
            matches = getattr(self.backend, "pointer_matches", None)
            if self.pointer is not None and callable(matches) and not matches(self.pointer):
                self.release()
                return
            if self.clock is not None and self.deadline is not None and self.clock() >= self.deadline:
                self.release(preserve_aim=True)
                return
            self.held.add(desired)
            self.backend.key_down(desired)
            self.pressed_at = self.clock() if self.clock is not None else now
            self.dispatches[desired] += 1
            self.last_dispatch_at = self.pressed_at
            self.cast_pointer = self.pointer

    def service(self, now):
        """새 화면이 승인한 짧은 lease 안에서만 분석 루프와 독립적으로 반복 입력을 처리한다."""
        with self._lock:
            if self.closed or self.deadline is None:
                return
            if now >= self.deadline:
                self.release(preserve_aim=True)
                return
            try:
                self._press_if_ready(now)
            except Exception as exc:
                self.closed, self.fault = True, exc
                try:
                    self.release()
                finally:
                    raise

    def expire(self, now: float) -> bool:
        with self._lock:
            if self.deadline is not None and now >= self.deadline:
                self.release(preserve_aim=True)
                return True
            return False


class DryFieldInput:
    def __init__(self):
        self.events = deque(maxlen=1000)
        self.pointer = None

    def key_down(self, key):
        self.events.append(("down", key))

    def key_up(self, key):
        self.events.append(("up", key))

    def press(self, key):
        self.events.append(("press", key))

    def move(self, x, y):
        self.pointer = (x, y)
        self.events.append(("move", x, y))

    def pointer_matches(self, point):
        return self.pointer == point


class FieldControlLoop:
    """검증된 관찰의 의미는 생산자가 소유하고 입력 우선순위는 여기서 통합한다."""

    def __init__(self, state: CharacterSurvivalState, inputs: HeldFieldInput, limits: FieldLimits, cadence: AdaptiveTeleportCadence):
        if state.policy is None:
            raise ValueError("character survival policy required")
        if limits.arrival_seconds != cadence.timeout:
            raise ValueError("cadence and control arrival limits must agree")
        self.state, self.inputs, self.limits, self.cadence = state, inputs, limits, cadence
        self.phase = "travel"
        self.last_stamp: ObservationStamp | None = None
        self.combat_started: float | None = None
        self.potion_pending: tuple[ObservationStamp, str, float] | None = None
        self.replenishing = False
        self.paused = False
        self.stopped = False
        self.last = FieldDirective("hold", "not_observed")
        self._last_now = None
        self._lock = threading.RLock()

    def _clock_valid(self, now):
        _number(now, "now")
        if self._last_now is not None and now < self._last_now:
            self.stop()
            return False
        self._last_now = now
        return True

    def _emit(self, result: FieldDirective, now: float) -> FieldDirective:
        self.last = result
        self.inputs.apply(result, now)
        if result.action in ("combat", "inspect_loot", "buff", "use_potion", "exit"):
            self.cadence.reset()
        return result

    def pause(self, paused: bool = True) -> None:
        with self._lock:
            self.paused = paused
            self.inputs.release()
            self.cadence.reset()

    def stop(self) -> None:
        with self._lock:
            self.stopped = True
            self.inputs.close()

    def acknowledge_potion(self, stamp: ObservationStamp, column: int) -> bool:
        with self._lock:
            if self.potion_pending is None:
                return False
            sent, key, _ = self.potion_pending
            policy = self.state.policy
            rule = next((item for item in policy.belt_columns if item.column == column and item.key == key), None)
            if rule is None or stamp.sequence <= sent.sequence or stamp.observed_at < sent.observed_at:
                return False
            if not self.state.confirm_resource_change(stamp, column, rule.kind, -1):
                return False
            self.potion_pending = None
            return True

    def reconcile_belt(self, stamp: ObservationStamp, contents) -> bool:
        with self._lock:
            if self.potion_pending is not None:
                sent, _, _ = self.potion_pending
                if stamp.sequence <= sent.sequence or stamp.observed_at < sent.observed_at:
                    return False
            if not self.state.observe_belt(stamp, contents):
                return False
            # 완전한 재관찰은 절대 잔량이다. 소비 delta를 추가로 차감하지 않는다.
            self.potion_pending = None
            return True

    def tick(self, now: float) -> FieldDirective:
        with self._lock:
            if not self._clock_valid(now):
                return self._emit(FieldDirective("hold", "clock_regressed"), now)
            if self.stopped or self.paused or self.inputs.closed:
                return self._emit(FieldDirective("hold", "stopped_or_paused"), now)
            if self.state.potions_remaining == 0 and self.state.policy.exit_on_empty:
                return self._emit(FieldDirective("exit", "potions_empty"), now)
            due = self.state.next_buff_renewal_at
            if due is not None and now >= due:
                return self._emit(FieldDirective("hold", "buff_renewal_required"), now)
            if self.inputs.expire(now):
                return self._emit(FieldDirective("hold", "input_lease_expired"), now)
            return self.last

    def observe(self, observation: FieldObservation, now: float, *, focused: bool = True) -> FieldDirective:
        with self._lock:
            if not self._clock_valid(now):
                return self._emit(FieldDirective("hold", "clock_regressed"), now)
            if self.stopped or self.paused or self.inputs.closed or not focused:
                return self._emit(FieldDirective("hold", "stopped_paused_or_unfocused"), now)
            if self.state.potions_remaining == 0 and self.state.policy.exit_on_empty:
                return self._emit(FieldDirective("exit", "potions_empty"), now)
            safety, stamp = observation.safety, observation.safety.stamp
            if stamp.character_id != self.state.character_id or stamp.room_id != self.state.room_id:
                return self._emit(FieldDirective("hold", "wrong_context"), now)
            if not 0 <= now - stamp.observed_at <= self.limits.screen_age:
                return self._emit(FieldDirective("hold", "screen_stale"), now)
            if self.last_stamp is not None and (
                stamp.sequence <= self.last_stamp.sequence or stamp.observed_at < self.last_stamp.observed_at
            ):
                return self._emit(FieldDirective("hold", "screen_not_newer"), now)
            self.last_stamp = stamp
            decision = self.state.decide(
                safety, now, max_observation_age_seconds=self.limits.screen_age, max_belt_age_seconds=self.limits.belt_age
            )
            if self.potion_pending is not None:
                _, _, sent_at = self.potion_pending
                reason = (
                    "potion_confirmation_timeout" if now >= sent_at + self.limits.potion_confirmation_seconds else "potion_outcome_pending"
                )
                return self._emit(FieldDirective("hold", reason), now)
            if decision.action == "use_potion":
                self.potion_pending = (stamp, decision.potion_key, now)
                return self._emit(FieldDirective("use_potion", decision.reason, decision.potion_key), now)
            if decision.action not in ("continue", "replenish"):
                return self._emit(FieldDirective(decision.action, decision.reason), now)
            if safety.location != "field":
                return self._emit(FieldDirective("hold", "field_required"), now)
            if observation.belt_closed is not True:
                return self._emit(FieldDirective("hold", "belt_not_confirmed_closed"), now)
            target = sum(c.target_count for c in self.state.policy.belt_columns if c.kind == self.state.policy.potion_kind)
            if self.state.potions_remaining <= self.state.policy.replenish_at_or_below:
                self.replenishing = True
            elif self.state.potions_remaining >= target:
                self.replenishing = False
            due = self.state.next_buff_renewal_at
            deadline = stamp.observed_at + self.limits.screen_age
            if due is not None:
                deadline = min(deadline, due)
            if observation.enemy == "unknown":
                return self._emit(FieldDirective("hold", "enemy_unknown"), now)
            if observation.enemy == "near":
                if self.combat_started is None:
                    self.combat_started = now
                deadline = min(deadline, self.combat_started + self.limits.combat_seconds)
                if now >= deadline:
                    return self._emit(FieldDirective("hold", "combat_timeout"), now)
                self.phase = "combat"
                return self._emit(FieldDirective("combat", "near_enemy", deadline=deadline, hunt_visible=self.replenishing), now)
            if self.phase == "combat":
                if observation.enemy != "clear":
                    return self._emit(FieldDirective("hold", "combat_clear_unconfirmed"), now)
                self.phase = "loot"
                self.combat_started = None
                return self._emit(FieldDirective("inspect_loot", "combat_clear"), now)
            if self.phase == "loot":
                if observation.enemy != "clear" or not observation.loot_checked:
                    return self._emit(FieldDirective("inspect_loot", "loot_unconfirmed"), now)
                self.phase = "travel"
            point = observation.hunt_aim if self.replenishing and observation.enemy == "far" else observation.travel_aim
            if point is None or observation.reachable is not True:
                return self._emit(FieldDirective("hold", "destination_unconfirmed", hunt_visible=self.replenishing), now)
            if observation.arrived is True:
                self.cadence.confirm_arrival(stamp.observed_at)
            if self.cadence.deadline is not None and now >= self.cadence.deadline:
                return self._emit(FieldDirective("hold", "arrival_timeout"), now)
            aim = self.cadence.aim(point, now)
            if self.cadence.deadline is not None:
                deadline = min(deadline, self.cadence.deadline)
            return self._emit(
                FieldDirective(
                    "travel", "hunt_visible" if self.replenishing else "route", aim=aim, deadline=deadline, hunt_visible=self.replenishing
                ),
                now,
            )
