from __future__ import annotations

import math
from dataclasses import dataclass

from diablo2.common.survival import _integer, _number
from diablo2.runs.summoner.navigation import ArcanePathObserver, CameraMotion, PathView


WINGS = ("north", "east", "south", "west")


@dataclass(frozen=True)
class SearchLimits:
    screen_age: float
    landing_seconds: float
    wing_seconds: float
    return_seconds: float
    total_seconds: float
    minimum_movement: float
    breadcrumb_radius: float
    maximum_casts: int
    maximum_hover_attempts: int
    settled_frames: int
    settled_motion_pixels: float

    def __post_init__(self):
        for name in self.__dataclass_fields__:
            value = getattr(self, name)
            if name.startswith("maximum_") or name == "settled_frames":
                _integer(value, name, 2 if name == "settled_frames" else 1)
            elif _number(value, name) == 0:
                raise ValueError(f"{name} must be positive")


@dataclass(frozen=True)
class SearchObservation:
    view: PathView
    motion: CameraMotion
    movement_allowed: bool
    terminal: str | None = None
    hub_ready: bool = False
    hover: tuple[str, tuple[int, int, int, int]] | None = None

    def __post_init__(self):
        if type(self.movement_allowed) is not bool or type(self.hub_ready) is not bool:
            raise ValueError("positive movement/hub evidence required")
        if self.terminal not in (None, "nonboss_end", "summoner_present"):
            raise ValueError("confirmed terminal evidence required")
        if self.hover is not None and self.hover[0] not in ("chest", "coffin", "shrine", "teleporter"):
            raise ValueError("Arcane hover objects exclude town NPCs")


@dataclass(frozen=True)
class SearchAction:
    kind: str
    reason: str
    wing: str
    aim: tuple[int, int] | None = None
    request_id: int | None = None
    sequence: int | None = None
    generation: int | None = None
    issued_at: float | None = None


class ArcaneSearch:
    """새 지형과 확인된 상대 이동으로 네 날개를 잇는 단일 입력 요청 소유자."""

    def __init__(self, observer: ArcanePathObserver, limits: SearchLimits, *, reference_seconds, started_at):
        self.observer, self.limits = observer, limits
        self.reference = _number(reference_seconds, "reference_seconds")
        if self.reference == 0 or self.reference >= limits.landing_seconds:
            raise ValueError("positive casting reference below landing timeout required")
        self.started_at = self.stage_at = self.last_now = _number(started_at, "started_at")
        self.interval = self.reference
        self.wing_index, self.phase = 0, "outbound"
        self.position = (0.0, 0.0)
        self.breadcrumbs = [(0.0, 0.0)]
        self.return_points = []
        self.checked_wings = []
        self.last_view = None
        self.expected_generation = None
        self.pending = None
        self.cursor = None
        self.landing = None
        self.hover_attempts = self.cast_count = self.serial = 0
        self.landing_stable = 0
        self.review_reason = None

    @property
    def wing(self):
        return WINGS[self.wing_index]

    def _action(self, kind, reason, **values):
        return SearchAction(kind, reason, self.wing, **values)

    def fail(self, reason):
        self.review_reason = self.review_reason or reason
        self.phase = "review"
        self.pending = self.cursor = self.landing = None
        return self._action("review", self.review_reason)

    def tick(self, now, *, interrupted=False, focused=True):
        _number(now, "now")
        if now < self.last_now:
            return self.fail("clock_regressed")
        self.last_now = now
        if self.phase == "review":
            return self._action("review", self.review_reason)
        if self.phase == "summoner":
            return self._action("summoner", "handoff_combat")
        if interrupted or not focused:
            return self.fail("interrupted_or_unfocused")
        if now >= self.started_at + self.limits.total_seconds:
            return self.fail("search_timeout")
        stage_limit = self.limits.return_seconds if self.phase == "return" else self.limits.wing_seconds
        if now >= self.stage_at + stage_limit:
            return self.fail("return_timeout" if self.phase == "return" else "wing_timeout")
        if self.landing is not None and now >= self.landing[0] + self.limits.landing_seconds:
            return self.fail("landing_unconfirmed")
        if self.pending is not None and now >= self.pending.issued_at + self.limits.screen_age:
            return self.fail("input_ack_timeout")
        if self.last_view is not None and now - self.last_view.captured_at > self.limits.screen_age:
            return self._action("hold", "fresh_screen_required")
        return self._action("hold", "new_observation_required")

    def acknowledge(self, request, now, succeeded):
        if request != self.pending or type(succeeded) is not bool:
            return False
        if not succeeded or not request.issued_at <= now < request.issued_at + self.limits.screen_age:
            self.fail("input_failed_or_late")
            return False
        self.pending = None
        if request.kind == "aim":
            self.cursor = (request.aim, request.sequence, now)
        else:
            self.landing = (now, self.position, request.sequence)
            self.landing_stable = 0
            self.cursor = None
            self.cast_count += 1
            self.expected_generation += 1
        return True

    def _request(self, kind, reason, aim, view, now):
        self.serial += 1
        self.pending = self._action(
            kind,
            reason,
            aim=aim,
            request_id=self.serial,
            sequence=view.sequence,
            generation=view.generation,
            issued_at=now,
        )
        return self.pending

    def defer(self, request):
        """아직 시전하지 않은 요청이 생존/버프에 선점되면 다음 화면에서 다시 조준한다."""
        if self.pending != request:
            return False
        self.pending = self.cursor = None
        return True

    def step(self, observation: SearchObservation, now, *, interrupted=False, focused=True):
        status = self.tick(now, interrupted=interrupted, focused=focused)
        if status.kind in ("review", "summoner"):
            return status
        view = observation.view
        if not 0 <= now - view.captured_at <= self.limits.screen_age:
            return self._action("hold", "screen_stale")
        if self.expected_generation is None:
            self.expected_generation = view.generation
        if view.generation < self.expected_generation:
            return self._action("hold", "frame_precedes_cast")
        if view.generation > self.expected_generation:
            return self.fail("unexpected_scene_generation")
        if self.last_view is not None:
            if view.sequence <= self.last_view.sequence or view.captured_at < self.last_view.captured_at:
                return self._action("hold", "frame_not_newer")
            if view.frame_size != self.last_view.frame_size or view.viewport != self.last_view.viewport:
                return self.fail("window_layout_changed")
        had_view = self.last_view is not None
        self.last_view = view
        delta = observation.motion.displacement
        if had_view and delta is None:
            # 배경 대응이 끊긴 화면에서는 이전 커서가 그대로 유효하다고 추정하지 않는다.
            self.cursor = None
        if had_view and delta is not None:
            self.position = (self.position[0] + delta[0], self.position[1] + delta[1])
            if self.cursor is not None and math.hypot(*delta) > view.cell_pixels / 2:
                self.cursor = None
        if self.pending is not None:
            return self._action("hold", "input_outcome_pending")
        if observation.terminal == "summoner_present":
            self.phase = "summoner"
            self.cursor = self.landing = None
            return self._action("summoner", "handoff_combat")
        if self.landing is not None:
            began, origin, source = self.landing
            if view.sequence <= source or view.captured_at < began:
                return self._action("hold", "frame_precedes_cast")
            distance = math.hypot(self.position[0] - origin[0], self.position[1] - origin[1])
            # 이동량만 먼저 보이는 프레임에서 다음 굴곡을 조준하지 않는다.
            self.landing_stable = (
                self.landing_stable + 1 if delta is not None and math.hypot(*delta) <= self.limits.settled_motion_pixels else 0
            )
            if (
                delta is None
                or distance < self.limits.minimum_movement
                or now < began + self.interval
                or self.landing_stable < self.limits.settled_frames
            ):
                return self._action("hold", "landing_pending")
            self.interval = max(self.reference, min(self.limits.landing_seconds, 0.75 * self.interval + 0.25 * (now - began)))
            self.landing = None
            self.hover_attempts = 0
            if self.phase == "outbound":
                self.breadcrumbs.append(self.position)
        if not observation.movement_allowed:
            self.cursor = None
            return self._action("hold", "survival_or_buff_preempts_navigation")
        if self.phase == "outbound" and observation.terminal == "nonboss_end":
            self.checked_wings.append(self.wing)
            self.phase, self.stage_at = "return", now
            self.return_points = list(reversed(self.breadcrumbs))
            self.cursor = None
        if self.phase == "return":
            while (
                self.return_points
                and math.hypot(self.position[0] - self.return_points[0][0], self.position[1] - self.return_points[0][1])
                <= self.limits.breadcrumb_radius
            ):
                self.return_points.pop(0)
            if not self.return_points and observation.hub_ready:
                if self.wing_index == len(WINGS) - 1:
                    return self.fail("all_wings_empty")
                self.wing_index += 1
                self.phase, self.stage_at = "outbound", now
                self.position, self.breadcrumbs = (0.0, 0.0), [(0.0, 0.0)]
                self.cursor = None
                return self._action("wing_start", "hub_confirmed")
            if not self.return_points:
                self.cursor = None
                return self._action("hold", "hub_arrival_unconfirmed")
        if self.cast_count >= self.limits.maximum_casts:
            return self.fail("cast_budget_exhausted")
        if self.cursor is not None and observation.hover is None:
            aim, source, aimed_at = self.cursor
            if view.sequence > source and view.captured_at >= aimed_at:
                x = int((aim[0] - view.viewport[0]) // view.cell_pixels)
                y = int((aim[1] - view.viewport[1]) // view.cell_pixels)
                if 0 <= y < view.floor.shape[0] and 0 <= x < view.floor.shape[1] and view.floor[y, x]:
                    return self._request("cast", "fresh_floor_and_hover_clear", aim, view, now)
            self.cursor = None
        if observation.hover is not None:
            self.hover_attempts += 1
            self.cursor = None
            if self.hover_attempts > self.limits.maximum_hover_attempts:
                return self.fail("hover_unresolved")
        destination = self.return_points[0] if self.return_points else ((0.0, 0.0) if self.phase == "return" else None)
        choice = self.observer.choose(
            view,
            self.wing,
            position=self.position,
            visited=tuple(self.breadcrumbs[:-1]) if self.phase == "outbound" else (),
            destination=destination,
            excluded=(observation.hover[1],) if observation.hover is not None else (),
        )
        if choice.aim is None:
            return self._action("hold", choice.reason)
        return self._request("aim", "hover_reacquire" if observation.hover is not None else choice.reason, choice.aim, view, now)


def run_search(search, *, observe, backend, movement_key, focused, interrupted, clock, wait, on_action=lambda action: None):
    """필드 관찰 생산자와 표준 입력 backend를 연결하며 한 cast 뒤 새 화면을 기다린다."""
    try:
        while True:
            now = clock()
            action = search.tick(now, interrupted=interrupted(), focused=focused())
            if action.kind not in ("review", "summoner"):
                observation = observe(search.expected_generation)
                if observation is not None:
                    action = search.step(observation, clock(), interrupted=interrupted(), focused=focused())
            on_action(action)
            if action.kind in ("review", "summoner"):
                return action
            if action.kind in ("aim", "cast"):
                if interrupted() or not focused() or clock() - search.last_view.captured_at > search.limits.screen_age:
                    return search.fail("input_guard_rejected")
                if backend.move(*action.aim) is False:
                    search.acknowledge(action, clock(), False)
                    return search.fail("input_move_rejected")
                if action.kind == "cast":
                    if interrupted() or not focused() or clock() - search.last_view.captured_at > search.limits.screen_age:
                        return search.fail("input_guard_rejected")
                    if backend.press(movement_key) is False:
                        search.acknowledge(action, clock(), False)
                        return search.fail("input_cast_rejected")
                if not search.acknowledge(action, clock(), True):
                    return search.fail("input_ack_rejected")
            if wait(0.02):
                return search.fail("interrupted")
    except Exception:
        search.fail("observer_or_input_error")
        raise
    finally:
        backend.key_up(movement_key)
