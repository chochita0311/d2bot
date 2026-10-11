from __future__ import annotations

import threading
import time
from dataclasses import replace

from diablo2.common.field_control import FieldControlLoop, FieldObservation
from diablo2.common.survival import _number


def create_field_control(config, character_id, room_id, limits, *, backend=None):
    from diablo2.common.field_control import AdaptiveTeleportCadence, DryFieldInput, HeldFieldInput
    from diablo2.common.survival import CharacterSurvivalState

    profile = config.characters[character_id]
    estimate = config.casting_rules.estimate(profile.ruleset_family, profile.character_class, profile.casting)
    cadence = AdaptiveTeleportCadence(estimate, limits.arrival_seconds)
    actions = profile.actions
    if actions.movement_travel_mode != "hold":
        raise ValueError("this field controller requires an explicit held travel profile")
    inputs = HeldFieldInput(
        backend if backend is not None else DryFieldInput(), actions.movement_skill_key, actions.primary_attack_skill_key
    )
    return FieldControlLoop(CharacterSurvivalState(character_id, room_id, profile.survival), inputs, limits, cadence)


def create_field_runtime(config, character_id, room_id, limits, *, buff_limits, next_stamp, backend=None, **runtime_options):
    from diablo2.common.buffing import AutomaticBuffRenewal

    loop = create_field_control(config, character_id, room_id, limits, backend=backend)
    buffs = AutomaticBuffRenewal(
        loop.state,
        config.characters[character_id].actions,
        buff_limits,
        screen_age=limits.screen_age,
        belt_age=limits.belt_age,
        next_stamp=next_stamp,
        clock=runtime_options.get("clock", time.monotonic),
    )
    return FieldControlRuntime(loop, buffs=buffs, **runtime_options)


class FieldControlRuntime:
    """최신 관찰 한 개만 보관하며 비전/기록과 독립적으로 입력 시한을 감시한다."""

    def __init__(self, loop: FieldControlLoop, *, clock=time.monotonic, interval: float = 0.02, focused=lambda: False, buffs=None):
        self.loop, self.clock, self.interval, self.focused = loop, clock, _number(interval, "interval"), focused
        if interval == 0:
            raise ValueError("positive watchdog interval required")
        if buffs is not None and (
            buffs.state is not loop.state
            or buffs.screen_age != loop.limits.screen_age
            or buffs.belt_age != loop.limits.belt_age
            or buffs.clock is not clock
        ):
            raise ValueError("buff consumer must share this loop's state and observation limits")
        self.buffs = buffs
        self._stop = threading.Event()
        self._pause_requested = threading.Event()
        self._actor_lock = threading.RLock()
        self._lock = threading.Lock()
        self._pending = None
        self._latest_stamp = None
        self._thread = None
        self.error: Exception | None = None

    def publish(self, observation: FieldObservation, *, buff_confirmation=None) -> bool:
        return self._publish(observation, buff_confirmation=buff_confirmation)

    def publish_hud(self, reading, scene: FieldObservation, *, buff_confirmation=None) -> bool:
        from diablo2.common.survival_vision import HudReading

        if not isinstance(reading, HudReading):
            raise ValueError("same-frame HUD reading required")
        safety = replace(
            reading.safety_observation(scene.safety.stamp),
            location=scene.safety.location,
            monsters_clear=scene.safety.monsters_clear,
        )
        visibility = reading.belt_visibility
        observation = replace(
            scene, safety=safety, belt_closed=True if visibility == "closed" else False if visibility == "expanded" else None
        )
        return self._publish(observation, reading if visibility == "expanded" else None, buff_confirmation=buff_confirmation)

    def _publish(self, observation, expanded_belt=None, *, buff_confirmation=None) -> bool:
        from diablo2.common.buffing import BuffConfirmation

        stamp = observation.safety.stamp
        if buff_confirmation is not None and (
            not isinstance(buff_confirmation, BuffConfirmation) or buff_confirmation.stamp != stamp or self.buffs is None
        ):
            return False
        if stamp.character_id != self.loop.state.character_id or stamp.room_id != self.loop.state.room_id:
            return False
        if not 0 <= self.clock() - stamp.observed_at <= self.loop.limits.screen_age:
            return False
        with self._lock:
            if self._stop.is_set():
                return False
            previous = self._latest_stamp
            if previous is not None and (stamp.sequence <= previous.sequence or stamp.observed_at < previous.observed_at):
                return False
            self._latest_stamp = stamp
            self._pending = (observation, expanded_belt, buff_confirmation)
        return True

    def pump(self):
        with self._actor_lock:
            try:
                return self._pump()
            except Exception:
                if self.buffs is not None:
                    self.buffs.interrupt("buff_worker_error")
                self.loop.stop()
                raise

    def _pump(self):
        now = self.clock()
        if not self.focused():
            self.pause(True)
            return self.loop.tick(now)
        self.loop.tick(now)
        with self._lock:
            pending, self._pending = self._pending, None
        if pending is None:
            if self.buffs is not None:
                result = self.buffs.tick(now)
                if result is not None:
                    return self._buff_result(result, now)
            return self.loop.last
        observation, expanded_belt, confirmation = pending
        if expanded_belt is not None and 0 <= now - observation.safety.stamp.observed_at <= self.loop.limits.screen_age:
            self.loop.reconcile_belt(observation.safety.stamp, expanded_belt.belt_contents(self.loop.state.policy))
        if self.buffs is not None and not self.loop.stopped and not self.loop.paused and not self.loop.inputs.closed:
            if self.loop.state.potions_remaining == 0 and self.loop.state.policy.exit_on_empty:
                self.buffs.interrupt("buff_potions_empty")
                return self.loop.observe(observation, now)
            if self.loop.potion_pending is None:
                # 버프 키를 보내기 전에 유지 입력을 해제한다. 호출 자체는 sleep하지 않는다.
                def send_token(token):
                    self.loop.inputs.release()
                    if self._stop.is_set() or self._pause_requested.is_set() or self.loop.paused or self.loop.stopped or not self.focused():
                        return False
                    if not 0 <= self.clock() - observation.safety.stamp.observed_at <= self.loop.limits.screen_age:
                        return False
                    self.loop.inputs.backend.press(token)

                result = self.buffs.observe(
                    observation,
                    now,
                    send_token=send_token,
                    interrupted=lambda: self._stop.is_set()
                    or self._pause_requested.is_set()
                    or self.loop.paused
                    or self.loop.stopped
                    or not self.focused(),
                    confirmation=confirmation,
                )
                if result is not None:
                    if result.action in ("use_potion", "exit"):
                        return self.loop.observe(observation, now)
                    return self._buff_result(result, now)
        return self.loop.observe(observation, now)

    def _buff_result(self, result, now):
        from diablo2.common.field_control import FieldDirective

        if self.loop.state.potions_remaining == 0 and self.loop.state.policy.exit_on_empty:
            return self.loop.tick(now)
        return self.loop._emit(FieldDirective("hold", result.reason), now)

    @property
    def running(self):
        return self._thread is not None and self._thread.is_alive() and not self._stop.is_set()

    def _navigation_ready(self, stamp):
        from diablo2.common.field_control import DryFieldInput
        from diablo2.common.survival import ObservationStamp

        if not isinstance(stamp, ObservationStamp):
            return False
        now = self.clock()
        due = self.loop.state.next_buff_renewal_at
        with self._lock:
            current = self._latest_stamp
        return (
            not self._stop.is_set()
            and not self._pause_requested.is_set()
            and not self.loop.stopped
            and not self.loop.paused
            and not self.loop.inputs.closed
            and (isinstance(self.loop.inputs.backend, DryFieldInput) or self.running)
            and self.focused()
            and current == stamp == self.loop.last_stamp
            and 0 <= now - stamp.observed_at <= self.loop.limits.screen_age
            and (due is None or now < due)
            and self.loop.phase == "travel"
            and self.loop.last.action == "hold"
            and self.loop.last.reason == "destination_unconfirmed"
        )

    def navigation_allowed(self, stamp):
        """공통 판단이 목적지만 기다릴 때 현재 관찰의 외부 경로 제안을 허용한다."""
        with self._actor_lock:
            return self._navigation_ready(stamp)

    def dispatch_navigation(self, stamp, *, aim, cast=False):
        """버프/생존과 같은 소유자 안에서 한 번만 시전한다. 선점되면 False다."""
        if not isinstance(aim, tuple) or len(aim) != 2 or any(type(value) is not int or value < 0 for value in aim):
            raise ValueError("absolute nonnegative integer navigation aim required")
        if type(cast) is not bool:
            raise ValueError("explicit cast flag required")
        with self._actor_lock:
            if not self._navigation_ready(stamp):
                return False
            self.loop.inputs.release()
            backend = self.loop.inputs.backend
            if backend.move(*aim) is False:
                raise RuntimeError("navigation aim was not dispatched")
            if not self._navigation_ready(stamp):
                return False
            if cast and backend.press(self.loop.inputs.keys[0]) is False:
                raise RuntimeError("navigation cast was not dispatched")
            return True

    def start(self):
        if self._thread is not None or self._stop.is_set():
            raise RuntimeError("runtime can only be started once")
        self._thread = threading.Thread(target=self._run, name="d2-field-control", daemon=True)
        self._thread.start()

    def _run(self):
        try:
            while not self._stop.is_set():
                self.pump()
                self._stop.wait(self.interval)
        except Exception as exc:
            self.error = exc
            self._stop.set()
        finally:
            try:
                if self.buffs is not None:
                    self.buffs.interrupt("buff_worker_stopped")
                self.loop.stop()
            except Exception as exc:
                self.error = self.error or exc

    def pause(self, paused=True):
        if paused:
            self._pause_requested.set()
        with self._actor_lock:
            if paused and self.buffs is not None:
                self.buffs.interrupt("buff_paused_or_unfocused")
            self.loop.pause(paused)
            # 다시 시작할 때 일시정지 전 관찰을 재사용하지 않는다.
            with self._lock:
                self._pending = None
            if not paused:
                self._pause_requested.clear()

    def stop(self, timeout=1.0):
        self._stop.set()
        with self._actor_lock:
            if self.buffs is not None:
                self.buffs.interrupt("buff_stopped")
            self.loop.stop()
        if self._thread is not None and self._thread is not threading.current_thread():
            self._thread.join(timeout)
            if self._thread.is_alive():
                raise RuntimeError("field input backend failed to stop within timeout")
