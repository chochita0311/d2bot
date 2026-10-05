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
