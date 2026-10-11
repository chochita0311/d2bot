from __future__ import annotations

from dataclasses import dataclass

from diablo2.common.survival import CharacterSurvivalState, ObservationStamp, SafetyObservation, _number


@dataclass(frozen=True)
class BuffDirective:
    action: str
    reason: str
    index: int | None = None
    token: str | None = None


@dataclass(frozen=True)
class BuffConfirmation:
    stamp: ObservationStamp
    source: str
    effect_verified: bool
    equipment_restored: bool
    sequence_effects_verified: bool

    def __post_init__(self):
        if self.source not in ("supervised_visual", "validated_vision"):
            raise ValueError("Visual confirmation source required; key dispatch is not effect evidence")
        if any(type(value) is not bool for value in (self.effect_verified, self.equipment_restored, self.sequence_effects_verified)):
            raise ValueError("Explicit effect/equipment confirmation required")


class BuffSequence:
    """개인 순서를 한 단계씩 요청하며 전송과 시각적 효과 확인을 분리한다."""

    def __init__(self, state: CharacterSurvivalState, order):
        if state.policy is None:
            raise ValueError("Configured survival policy required")
        if not state.policy.buffs:
            raise ValueError("Timed buff rules required for this confirmation sequence")
        if not isinstance(order, (tuple, list)) or not order or any(not isinstance(token, str) or not token.strip() for token in order):
            raise ValueError("Explicit non-empty buff order required")
        self.state = state
        self.order = tuple(token.strip().lower() for token in order)
        self._timed_indices = {}
        for buff in state.policy.buffs:
            matches = [index for index, token in enumerate(self.order) if token == buff.key.strip().lower()]
            if not matches:
                raise ValueError("Timed buff key must occur in the configured sequence")
            self._timed_indices[matches[-1]] = buff.name
        if len(self._timed_indices) != len(state.policy.buffs):
            raise ValueError("Timed effects require distinct identifiable sequence steps")
        self.phase = "ready"
        self.index = 0
        self._pending: ObservationStamp | None = None
        self._last: ObservationStamp | None = None
        self._casts: dict[str, ObservationStamp] = {}
        self._confirmed: set[str] = set()

    def _accept(self, stamp: ObservationStamp) -> bool:
        return (
            stamp.character_id == self.state.character_id
            and stamp.room_id == self.state.room_id
            and (self._last is None or (stamp.sequence > self._last.sequence and stamp.observed_at >= self._last.observed_at))
        )

    def cancel(self) -> None:
        self.phase = "cancelled"
        self._pending = None

    def _guard(self, observation, now, screen_age, belt_age):
        if not self._accept(observation.stamp):
            return BuffDirective("hold", "wrong_or_old_context")
        decision = self.state.decide(observation, now, max_observation_age_seconds=screen_age, max_belt_age_seconds=belt_age)
        if decision.action in ("use_potion", "exit"):
            return BuffDirective(decision.action, decision.reason)
        if observation.location != "field" or observation.monsters_clear is not True:
            return BuffDirective("hold", "field_safety_unverified")
        if self.phase == "complete" and decision.action == "buff":
            return BuffDirective("hold", "buff_renewal_required")
        if decision.action not in ("buff", "continue", "replenish"):
            return BuffDirective("hold", decision.reason)
        return None

    def next_step(self, observation: SafetyObservation, now: float, *, screen_age: float, belt_age: float) -> BuffDirective:
        if self.phase == "cancelled":
            return BuffDirective("hold", "sequence_cancelled")
        blocked = self._guard(observation, now, screen_age, belt_age)
        if blocked is not None:
            if self.phase != "ready":
                self.cancel()
            return blocked
        if self._pending is not None:
            return BuffDirective("hold", "input_outcome_pending")
        if self.index == len(self.order):
            return BuffDirective(
                "complete" if self.phase == "complete" else "hold", "confirmed" if self.phase == "complete" else "effect_unconfirmed"
            )
        self.phase = "running"
        self._pending = observation.stamp
        self._last = observation.stamp
        return BuffDirective("input_request", "next_step", self.index, self.order[self.index])

    def acknowledge_step(self, index: int, stamp: ObservationStamp, succeeded: bool) -> bool:
        if self.phase != "running" or self._pending is None or type(index) is not int or index != self.index or type(succeeded) is not bool:
            return False
        if not self._accept(stamp):
            return False
        if not succeeded:
            self.cancel()
            return False
        name = self._timed_indices.get(index)
        if name is not None:
            # 완료/검토 시각을 늦춰 버프 수명을 늘리지 않는다.
            self._casts[name] = self._pending
        self._last = stamp
        self._pending = None
        self.index += 1
        if self.index == len(self.order):
            self.phase = "await_effect"
        return True

    def confirm_effect(
        self, name: str, evidence: BuffConfirmation, observation: SafetyObservation, now: float, *, screen_age: float, belt_age: float
    ) -> bool:
        if self.phase != "await_effect" or name in self._confirmed or name not in self._casts:
            return False
        if (
            evidence.stamp != observation.stamp
            or not evidence.effect_verified
            or not evidence.equipment_restored
            or not evidence.sequence_effects_verified
        ):
            return False
        if self._guard(observation, now, screen_age, belt_age) is not None:
            self.cancel()
            return False
        buff = next(buff for buff in self.state.policy.buffs if buff.name == name)
        cast = self._casts[name]
        if now >= cast.observed_at + buff.duration_seconds - buff.renew_before_seconds:
            self.cancel()
            return False
        if not self.state.confirm_buff(cast, name, "field", True):
            return False
        self._confirmed.add(name)
        self._last = observation.stamp
        if self._confirmed == set(self._casts):
            self.phase = "complete"
        return True


class BuffSequenceExecutor:
    """프로필 간격으로 연속 전송하되 효과 확인은 별도 트랜잭션에 남긴다."""

    def __init__(self, sequence, pauses, *, default_pause, screen_age, belt_age, timeout):
        self.sequence = sequence
        self.pauses = {token: _number(seconds, "buff pause") for token, seconds in pauses.items()}
        self.default_pause = _number(default_pause, "default buff pause")
        self.screen_age, self.belt_age, self.timeout = screen_age, belt_age, _number(timeout, "buff timeout")
        if self.timeout == 0 or any(_number(value, "observation age") == 0 for value in (screen_age, belt_age)):
            raise ValueError("positive timeout and observation ages required")

    def run(self, *, observe, send_token, next_stamp, clock, wait, interrupted):
        began = clock()
        sequence = self.sequence
        for _ in sequence.order:
            if interrupted() or clock() - began >= self.timeout:
                sequence.cancel()
                return BuffDirective("hold", "sequence_interrupted")
            scene = observe()
            now = clock()
            if scene.belt_closed is not True or interrupted() or now - began >= self.timeout:
                sequence.cancel()
                return BuffDirective("hold", "belt_or_execution_unverified")
            directive = sequence.next_step(scene.safety, now, screen_age=self.screen_age, belt_age=self.belt_age)
            if directive.action != "input_request":
                return directive
            try:
                succeeded = send_token(directive.token) is not False
                acknowledged = sequence.acknowledge_step(directive.index, next_stamp(), succeeded)
            except Exception:
                sequence.cancel()
                raise
            if not acknowledged:
                sequence.cancel()
                return BuffDirective("hold", "dispatch_unconfirmed")
            gap = self.pauses.get(directive.token, self.default_pause)
            remaining = self.timeout - (clock() - began)
            if remaining <= 0 or wait(min(gap, remaining)) or interrupted() or clock() - began >= self.timeout:
                sequence.cancel()
                return BuffDirective("hold", "sequence_interrupted")
        return BuffDirective("await_confirmation", "sequence_dispatched")


@dataclass(frozen=True)
class BuffRenewalLimits:
    sequence_seconds: float
    confirmation_seconds: float
    safety_wait_seconds: float

    def __post_init__(self):
        for name in self.__dataclass_fields__:
            if _number(getattr(self, name), name) == 0:
                raise ValueError(f"{name} must be positive")


class AutomaticBuffRenewal:
    """같은 필드 actor에서 새 관찰마다 진행하여 키 간격이 감시를 막지 않게 한다."""

    def __init__(self, state, actions, limits: BuffRenewalLimits, *, screen_age, belt_age, next_stamp, clock):
        BuffSequence(state, actions.pre_run_buff_order)
        if not isinstance(limits, BuffRenewalLimits) or not callable(next_stamp) or not callable(clock):
            raise ValueError("explicit renewal limits and shared-clock stamp producer required")
        self.state, self.actions, self.limits, self.next_stamp = state, actions, limits, next_stamp
        self.clock = clock
        self.screen_age, self.belt_age = _number(screen_age, "screen_age"), _number(belt_age, "belt_age")
        if not self.screen_age or not self.belt_age:
            raise ValueError("positive observation ages required")
        self.pauses = {key: _number(value, "buff pause") for key, value in actions.buff_action_pause_seconds.items()}
        self.sequence: BuffSequence | None = None
        self.review_reason: str | None = None
        self.requested_at: float | None = None
        self.started_at: float | None = None
        self.confirmation_at: float | None = None
        self.next_step_at = 0.0
        self.last_stamp: ObservationStamp | None = None
        self.last_now: float | None = None

    @property
    def pending(self):
        return self.requested_at is not None

    def cancel(self, reason):
        if self.sequence is not None:
            self.sequence.cancel()
        self.review_reason = self.review_reason or reason
        return BuffDirective("hold", self.review_reason)

    def interrupt(self, reason):
        if self.pending:
            self.cancel(reason)

    def tick(self, now):
        now = _number(now, "now")
        if self.last_now is not None and now < self.last_now:
            return self.cancel("buff_clock_regressed")
        self.last_now = now
        if self.review_reason is not None:
            return BuffDirective("hold", self.review_reason)
        due = self.state.next_buff_renewal_at
        if not self.pending and due is not None and now >= due:
            self.requested_at = due
        if not self.pending:
            return None
        if self.last_stamp is None or not 0 <= now - self.last_stamp.observed_at <= self.screen_age:
            return self.cancel("buff_observation_stale")
        if self.started_at is None:
            if now >= self.requested_at + self.limits.safety_wait_seconds:
                return self.cancel("buff_safety_timeout")
        elif self.confirmation_at is None:
            if now >= self.started_at + self.limits.sequence_seconds:
                return self.cancel("buff_sequence_timeout")
        elif now >= self.confirmation_at + self.limits.confirmation_seconds:
            return self.cancel("buff_confirmation_timeout")
        return BuffDirective("hold", "buff_observation_required")

    def observe(self, scene, now, *, send_token, interrupted, confirmation=None):
        # 이전 관찰의 감시 시한을 새 프레임 도착으로 되살리지 않는다.
        self.tick(now)
        if self.review_reason is not None:
            return BuffDirective("hold", self.review_reason)
        safety, stamp = scene.safety, scene.safety.stamp
        valid = (
            stamp.character_id == self.state.character_id
            and stamp.room_id == self.state.room_id
            and 0 <= now - stamp.observed_at <= self.screen_age
            and (
                self.last_stamp is None or (stamp.sequence > self.last_stamp.sequence and stamp.observed_at >= self.last_stamp.observed_at)
            )
        )
        if not valid:
            return self.cancel("buff_screen_invalid") if self.pending else None
        if (
            self.sequence is not None
            and self.sequence._last is not None
            and (stamp.sequence <= self.sequence._last.sequence or stamp.observed_at < self.sequence._last.observed_at)
        ):
            # press 도중 큐에 들어온 프레임은 다음 단계나 확인에 쓰지 않는다.
            return BuffDirective("hold", "buff_frame_precedes_dispatch")
        self.last_stamp = stamp
        decision = self.state.decide(safety, now, max_observation_age_seconds=self.screen_age, max_belt_age_seconds=self.belt_age)
        if not self.pending:
            if decision.action != "buff" and decision.reason not in ("buff_unsafe", "buff_requires_field"):
                return None
            self.requested_at = now
        if interrupted():
            return self.cancel("buff_interrupted")
        if decision.action in ("use_potion", "exit"):
            self.cancel("buff_preempted_" + decision.reason)
            return BuffDirective(decision.action, decision.reason)
        safe = (
            safety.location == "field"
            and safety.monsters_clear is True
            and scene.belt_closed is True
            and decision.action in ("buff", "continue", "replenish")
        )
        if not safe:
            if self.started_at is not None:
                return self.cancel("buff_safety_lost")
            return BuffDirective("hold", "buff_waiting_for_safe_field")
        if self.sequence is None:
            self.sequence = BuffSequence(self.state, self.actions.pre_run_buff_order)
            self.started_at = now
        sequence = self.sequence
        if sequence._guard(safety, now, self.screen_age, self.belt_age) is not None:
            return self.cancel("buff_screen_precedes_dispatch")
        if now < self.next_step_at:
            return BuffDirective("hold", "buff_gap")
        if sequence.phase == "await_effect":
            if confirmation is None:
                return BuffDirective("hold", "buff_effect_unconfirmed")
            names = [rule.name for rule in self.state.policy.buffs if rule.name not in sequence._confirmed]
            if not all(
                sequence.confirm_effect(name, confirmation, safety, now, screen_age=self.screen_age, belt_age=self.belt_age)
                for name in names
            ):
                return self.cancel("buff_confirmation_rejected")
            self.sequence = None
            self.requested_at = self.started_at = self.confirmation_at = None
            return BuffDirective("complete", "buff_confirmed")
        directive = sequence.next_step(safety, now, screen_age=self.screen_age, belt_age=self.belt_age)
        if directive.action != "input_request" or interrupted():
            return self.cancel("buff_dispatch_blocked")
        try:
            succeeded = send_token(directive.token) is not False
            acknowledged_at = self.next_stamp()
            if not now <= acknowledged_at.observed_at <= self.clock():
                return self.cancel("buff_dispatch_stamp_invalid")
            acknowledged = sequence.acknowledge_step(directive.index, acknowledged_at, succeeded)
        except Exception:
            self.cancel("buff_input_error")
            raise
        if not acknowledged:
            return self.cancel("buff_dispatch_unconfirmed")
        self.next_step_at = acknowledged_at.observed_at + self.pauses.get(directive.token, 0.4)
        if sequence.phase == "await_effect":
            self.confirmation_at = acknowledged_at.observed_at
        return BuffDirective("hold", "buff_sequence_running")


class BeltInspection:
    """잔량 관찰과 닫힘의 긍정 증거를 분리해 펼친 벨트의 이동을 막는다."""

    def __init__(self, state: CharacterSurvivalState):
        if state.policy is None:
            raise ValueError("Configured belt policy required")
        self.state = state
        self.visibility = "unknown"
        self._last: ObservationStamp | None = None

    def request_toggle(self) -> str:
        self.visibility = "unknown"
        return self.state.policy.belt_toggle_key

    def observe(self, stamp: ObservationStamp, reading) -> bool:
        if stamp.character_id != self.state.character_id or stamp.room_id != self.state.room_id:
            return False
        if self._last is not None and (stamp.sequence <= self._last.sequence or stamp.observed_at < self._last.observed_at):
            return False
        self._last = stamp
        self.visibility = reading.belt_visibility
        if self.visibility == "expanded":
            contents = reading.belt_contents(self.state.policy)
            if contents is None:
                self.state.invalidate_belt(stamp)
                return False
            return self.state.observe_belt(stamp, contents)
        return self.visibility == "closed"

    def permits_movement(self, observation: SafetyObservation, now: float, *, screen_age: float, belt_age: float) -> bool:
        if self.visibility != "closed" or self._last is None:
            return False
        if observation.stamp.sequence < self._last.sequence or observation.stamp.observed_at < self._last.observed_at:
            return False
        if not 0 <= now - self._last.observed_at <= screen_age:
            return False
        decision = self.state.decide(observation, now, max_observation_age_seconds=screen_age, max_belt_age_seconds=belt_age)
        return decision.action in ("continue", "replenish")
