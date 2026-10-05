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


class FieldControlRuntime:
    """최신 관찰 한 개만 보관하며 비전/기록과 독립적으로 입력 시한을 감시한다."""

    def __init__(self, loop: FieldControlLoop, *, clock=time.monotonic, interval: float = 0.02, focused=lambda: False):
        self.loop, self.clock, self.interval, self.focused = loop, clock, _number(interval, "interval"), focused
        if interval == 0:
            raise ValueError("positive watchdog interval required")
        self._stop = threading.Event()
        self._lock = threading.Lock()
        self._pending = None
        self._latest_stamp = None
        self._thread = None
        self.error: Exception | None = None

    def publish(self, observation: FieldObservation) -> bool:
        return self._publish(observation)

    def publish_hud(self, reading, scene: FieldObservation) -> bool:
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
        return self._publish(observation, reading if visibility == "expanded" else None)

    def _publish(self, observation, expanded_belt=None) -> bool:
        stamp = observation.safety.stamp
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
            self._pending = (observation, expanded_belt)
        return True

    def pump(self):
        now = self.clock()
        if not self.focused():
            self.loop.pause(True)
            return self.loop.tick(now)
        self.loop.tick(now)
        with self._lock:
            pending, self._pending = self._pending, None
        if pending is None:
            return self.loop.last
        observation, expanded_belt = pending
        if expanded_belt is not None and 0 <= now - observation.safety.stamp.observed_at <= self.loop.limits.screen_age:
            self.loop.reconcile_belt(observation.safety.stamp, expanded_belt.belt_contents(self.loop.state.policy))
        return self.loop.observe(observation, now)

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
                self.loop.stop()
            except Exception as exc:
                self.error = self.error or exc

    def pause(self, paused=True):
        self.loop.pause(paused)
        # 다시 시작할 때 일시정지 전 관찰을 재사용하지 않는다.
        with self._lock:
            self._pending = None

    def stop(self, timeout=1.0):
        self._stop.set()
        self.loop.stop()
        if self._thread is not None and self._thread is not threading.current_thread():
            self._thread.join(timeout)
            if self._thread.is_alive():
                raise RuntimeError("field input backend failed to stop within timeout")
