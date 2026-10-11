from __future__ import annotations

from dataclasses import dataclass, replace

from diablo2.common.field_control import DryFieldInput, FieldObservation
from diablo2.runs.summoner.search import SearchAction, SearchObservation


@dataclass(frozen=True)
class ArcaneScene:
    field: FieldObservation
    terminal: str | None = None
    hub_ready: bool = False
    hover: tuple[str, tuple[int, int, int, int]] | None = None
    buff_confirmation: object = None
    player_ground: tuple[int, int] | None = None

    def __post_init__(self):
        if not isinstance(self.field, FieldObservation):
            raise ValueError("same-frame field observation required")
        if self.field.travel_aim is not None or self.field.hunt_aim is not None:
            raise ValueError("perceptual search owns movement; held route aims must be absent")
        if self.terminal not in (None, "nonboss_end", "summoner_present") or type(self.hub_ready) is not bool:
            raise ValueError("explicit terminal and hub evidence required")
        if self.player_ground is not None and (
            not isinstance(self.player_ground, tuple)
            or len(self.player_ground) != 2
            or any(type(value) is not int or value < 0 for value in self.player_ground)
        ):
            raise ValueError("same-frame native player ground required")
        if self.hover is not None and (
            not isinstance(self.hover, tuple)
            or len(self.hover) != 2
            or self.hover[0] not in ("chest", "coffin", "shrine", "teleporter")
            or not isinstance(self.hover[1], tuple)
            or len(self.hover[1]) != 4
            or any(type(value) is not int or value < 0 for value in self.hover[1])
            or self.hover[1][0] >= self.hover[1][2]
            or self.hover[1][1] >= self.hover[1][3]
        ):
            raise ValueError("Arcane hover requires a supported object and native box; NPCs are town-only")


class ArcaneSearchRuntime:
    """화면/HUD/경로를 연결한다. 의미 판독과 실제 검증은 scene_reader가 소유한다."""

    def __init__(self, search, field_runtime, capture, *, motion, hud, scene_reader, next_stamp, viewport, anchor):
        if search.limits.screen_age != field_runtime.loop.limits.screen_age:
            raise ValueError("search and shared control must agree on frame freshness")
        self.search, self.field, self.capture = search, field_runtime, capture
        self.motion, self.hud, self.scene_reader, self.next_stamp = motion, hud, scene_reader, next_stamp
        self.viewport, self.anchor = viewport, anchor
        self.target = None
        self.closed = False

    def _window(self, expected=None):
        target = dict(self.capture.current_target())
        if set(target) != {"left", "top", "width", "height"} or any(type(value) is not int for value in target.values()):
            raise ValueError("native integer window rectangle required")
        if min(target["width"], target["height"]) <= 0 or (expected is not None and target != expected):
            raise RuntimeError("window rectangle changed during search")
        return target

    def step(self):
        if self.closed:
            return self.search.fail("runtime_closed")
        clock = self.field.clock
        try:
            if not isinstance(self.field.loop.inputs.backend, DryFieldInput) and not self.field.running:
                raise RuntimeError("live search requires the independent field watchdog")
            status = self.search.tick(clock(), interrupted=self.field.loop.stopped, focused=self.field.focused())
            if status.kind in ("review", "summoner"):
                self.field.loop.inputs.release()
                if status.kind == "review":
                    self.field.stop()
                return status
            before = self._window(self.target)
            self.target = before
            captured_at = clock()
            generation = self.search.expected_generation or 0
            packet = self.capture.grab()
            self._window(before)
            if dict(self.capture.target) != before or packet.frame.shape[:2] != (before["height"], before["width"]):
                raise RuntimeError("capture and current window differ")
            stamp = replace(self.next_stamp(), observed_at=captured_at)
            scene = self.scene_reader(packet.frame, stamp)
            if not isinstance(scene, ArcaneScene) or scene.field.safety.stamp != stamp:
                raise ValueError("scene must use the captured frame's exact stamp")
            reading = self.hud.observe(packet.frame)
            if not self.field.publish_hud(reading, scene.field, buff_confirmation=scene.buff_confirmation):
                status = self.search.tick(clock())
                if status.kind == "review":
                    self.field.stop()
                return status if status.kind == "review" else SearchAction("hold", "shared_observation_rejected", self.search.wing)
            self.field.pump()
            if self.field.loop.last.action == "exit":
                self.field.stop()
                return self.search.fail("shared_safety_exit")
            if self.field.error is not None:
                raise RuntimeError("shared field worker failed") from self.field.error
            anchor = scene.player_ground
            if anchor is None and not isinstance(self.field.loop.inputs.backend, DryFieldInput):
                return SearchAction("hold", "player_ground_unverified", self.search.wing)
            view = self.search.observer.observe(
                packet.frame,
                sequence=stamp.sequence,
                generation=generation,
                captured_at=captured_at,
                viewport=self.viewport,
                anchor=anchor if anchor is not None else self.anchor,
            )
            observation = SearchObservation(
                view,
                self.motion.observe(packet.frame, view),
                self.field.navigation_allowed(stamp),
                scene.terminal,
                scene.hub_ready,
                scene.hover,
            )
            action = self.search.step(observation, clock(), interrupted=self.field.loop.stopped, focused=self.field.focused())
            if action.kind in ("review", "summoner"):
                self.field.loop.inputs.release()
                if action.kind == "review":
                    self.field.stop()
            if action.kind in ("aim", "cast"):
                self._window(before)
                aim = (action.aim[0] + before["left"], action.aim[1] + before["top"])
                if self.field.dispatch_navigation(stamp, aim=aim, cast=action.kind == "cast"):
                    if not self.search.acknowledge(action, clock(), True):
                        return self.search.fail("navigation_ack_rejected")
                else:
                    self.search.defer(action)
                    return SearchAction("hold", "shared_control_preempted", self.search.wing)
            return action
        except Exception:
            self.search.fail("observer_or_input_error")
            self.close()
            raise

    def run(self, *, wait, on_action=lambda action: None):
        """공통 watchdog을 시작한 context 안에서 호출한다. 방 재시작을 수행하지 않는다."""
        while not self.closed:
            action = self.step()
            on_action(action)
            if action.kind in ("review", "summoner"):
                if action.kind == "review":
                    self.field.stop()
                return action
            if wait(0.02):
                self.field.stop()
                return self.search.fail("interrupted")
        return self.search.fail("runtime_closed")

    def close(self):
        if not self.closed:
            self.closed = True
            try:
                self.field.stop()
            finally:
                self.capture.close()

    def __enter__(self):
        self.field.start()
        return self

    def __exit__(self, *args):
        self.close()
