from __future__ import annotations

import threading
from dataclasses import dataclass
from pathlib import Path
from queue import Empty, Queue

import numpy as np

from diablo2.actions.run_lifecycle import RunLifecycleSession
from diablo2.common.capture import ScreenCapture
from diablo2.common.config import BotConfig
from diablo2.runs.base import RunDefinition, RunPayloadState, RunPort
from diablo2.runs.summoner.routes.common.arcane_common import (
    build_initial_arcane_belief_state,
    commit_arcane_wing,
    load_arcane_assets,
    load_arcane_monster_templates,
    log_arcane_state,
    resolve_arcane_character_actions,
    run_arcane_pre_run_buffs,
    scan_arcane_monsters,
)
from diablo2.runs.summoner.routes.arcane_entry import PORT as ARCANE_ENTRY, run_arcane_entry
from diablo2.runs.summoner.routes.east_go import ROUTE_SEGMENT as EAST_GO
from diablo2.runs.summoner.routes.east_return import ROUTE_SEGMENT as EAST_RETURN
from diablo2.runs.summoner.routes.north_go import ROUTE_SEGMENT as NORTH_GO, run_arcane_north_go
from diablo2.runs.summoner.routes.north_return import ROUTE_SEGMENT as NORTH_RETURN
from diablo2.runs.summoner.runtime.runtime_helpers import (
    aim_relative_ratio,
    apply_offset,
    center_right_anchor,
    check_for_user_interrupt,
    focus_game_window,
    hold_key_down,
    hold_key_up,
    load_image,
    load_optional_image,
    locate_template,
    press_key,
    sleep_range,
)
from diablo2.runs.summoner.routes.south_go import ROUTE_SEGMENT as SOUTH_GO
from diablo2.runs.summoner.routes.south_return import ROUTE_SEGMENT as SOUTH_RETURN
from diablo2.runs.summoner.routes.west_go import ROUTE_SEGMENT as WEST_GO
from diablo2.runs.summoner.routes.west_return import ROUTE_SEGMENT as WEST_RETURN

SUMMONER_PROFILE_ID = "summoner"


@dataclass
class SummonerEvent:
    level: str
    message: str


@dataclass
class SummonerMatchResult:
    top_left: tuple[int, int]
    width: int
    height: int
    score: float
    source_index: int | None = None


@dataclass
class SummonerRunContext:
    profile_id: str
    definition: RunDefinition

    @property
    def profile(self):
        return self.definition.profile


class SummonerRunOrchestrator:
    STOP_HOTKEY = "f10"
    event_class = SummonerEvent
    match_result_class = SummonerMatchResult

    ACT1_MAP_TEMPLATE_PATHS = (
        Path("assets/waypoint/act1/act1_자매단_야영지_on_map_1.png"),
        Path("assets/waypoint/act1/act1_자매단_야영지_on_map_2.png"),
    )
    ACT1_WAYPOINT_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_자매단_야영지.png")
    ACT1_WAYPOINT_HOVER_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_자매단_야영지_when_hover.png")
    ACT1_LIST_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_list_when_left_click.png")
    ACT1_LIST_PANEL_TEMPLATE_PATH = Path("assets/waypoint/act1/act1_list.png")
    ACT2_LIST_PANEL_TEMPLATE_PATH = Path("assets/waypoint/act2/act2_list.png")

    MAP_THRESHOLD = 0.5
    WAYPOINT_THRESHOLD = 0.74
    WAYPOINT_HOVER_THRESHOLD = 0.74
    LIST_THRESHOLD = 0.93
    LIST_PANEL_THRESHOLD = 0.88

    USER_INTERRUPT_DISTANCE = 80
    TARGET_JITTER = 3
    MOVE_STEPS = (1, 2)
    MOVE_SLEEP = (0.005, 0.015)
    CLICK_SETTLE = (0.01, 0.03)
    ACTION_SLEEP = (0.01, 0.03)
    FOCUS_SLEEP = (0.12, 0.2)

    MINIMAP_SLEEP = (0.05, 0.12)
    SCOUT_MOVE_SETTLE = (0.01, 0.02)
    SCOUT_HOLD_SETTLE = (0.01, 0.02)
    SCOUT_UP_SETTLE = (0.28, 0.32)
    SCOUT_RIGHT_SETTLE = (0.28, 0.32)
    HOVER_WAIT_SECONDS = 2.4
    WAYPOINT_LIST_WAIT_SECONDS = 1.2
    MAP_SCAN_TIMEOUT = 2.2
    WAYPOINT_SCAN_TIMEOUT = 1.0
    OPEN_ATTEMPTS = 4
    ACT2_TAB_RATIO = (136 / 444, 18 / 599)
    MINIMAP_SCOUT_POINTS = ((0.66, 0.44), (0.74, 0.42), (0.80, 0.40))
    WORLD_SCOUT_ANCHOR_OFFSET = (0.14, 0.0)
    WORLD_SCOUT_UP_HOLD_POINT = (0.0, -0.18)
    WORLD_SCOUT_DOWN_HOLD_POINT = (0.0, 0.24)
    WORLD_SCOUT_UP_HOLD_SECONDS = (0.75, 0.80)
    WORLD_SCOUT_DOWN_HOLD_SECONDS = (1.60, 1.70)
    WORLD_SCOUT_RIGHT_HOLD_POINT = (0.20, 0.0)
    WORLD_SCOUT_RIGHT_HOLD_SECONDS = (0.25, 0.35)

    def __init__(self, config: BotConfig):
        self.config = config
        self.events: Queue[SummonerEvent] = Queue()
        self._thread: threading.Thread | None = None
        self._lock = threading.Lock()
        self._is_running = False
        self._stop_event = threading.Event()
        self._active_lifecycle: RunLifecycleSession | None = None
        self._last_pointer: tuple[int, int] | None = None
        self._current_scout_anchor = (0.5, 0.5)
        self._last_detection_route = "direct"

        self._act1_map_templates = [self._load_image(path) for path in self._resolve_act1_map_template_paths()]
        self._act1_waypoint_template = self._load_image(self._resolve_act1_waypoint_template_path())
        self._act1_waypoint_hover_template = self._load_image(self._resolve_act1_waypoint_hover_template_path())
        self._act1_list_template = self._load_image(self.ACT1_LIST_TEMPLATE_PATH)
        self._act1_list_panel_template = self._load_image(self.ACT1_LIST_PANEL_TEMPLATE_PATH)
        self._act2_list_panel_template = self._load_image(self.ACT2_LIST_PANEL_TEMPLATE_PATH)
        load_arcane_assets(self)

    def update_config(self, config: BotConfig) -> None:
        self.config = config

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._is_running

    def start(self, run_number: int = 1) -> None:
        with self._lock:
            if self._is_running:
                raise RuntimeError("Summoner run is already running.")
            self._stop_event.clear()
            self._thread = threading.Thread(target=self._run, args=(run_number,), daemon=True)
            self._is_running = True
            self._thread.start()
        self.events.put(self.event_class("info", f"Summoner run started for stage make_room -> arcane_entry -> buff_before_run -> north_go (run {run_number})."))

    def stop(self) -> None:
        self._stop_event.set()
        active_lifecycle = self._active_lifecycle
        if active_lifecycle is not None:
            active_lifecycle.request_stop()
        thread = self._thread
        if thread is not None:
            thread.join(timeout=3.0)
        with self._lock:
            self._thread = None
            self._is_running = False
        self.events.put(self.event_class("info", "Summoner run stopped."))

    def _run(self, run_number: int) -> None:
        try:
            self.run_current_stage(run_number)
        except Exception as exc:  # pragma: no cover
            self.events.put(self.event_class("error", f"Summoner run failed: {exc}"))
        finally:
            with self._lock:
                self._thread = None
                self._is_running = False

    def request_stop(self) -> None:
        self._stop_event.set()

    def run_current_stage(self, run_number: int = 1) -> None:
        self.make_room(run_number=run_number)
        capture = ScreenCapture(self.config.capture)
        focus_game_window(self, capture)
        self.enter_arcane(capture)
        self.buff_before_run()
        self.run_north_go(capture)

    def make_room(self, run_number: int = 1) -> None:
        lifecycle = RunLifecycleSession(self.config.capture)
        self._active_lifecycle = lifecycle
        capture = ScreenCapture(self.config.capture)
        self.events.put(self.event_class("info", f"Summoner: make_room starting for run {run_number}."))
        try:
            lifecycle._focus_game_window(capture)
            self._drain_lifecycle_events(lifecycle)
            lifecycle.create_room(capture, run_number)
            self._drain_lifecycle_events(lifecycle)
            self.events.put(self.event_class("info", f"Summoner: make_room completed for run {run_number}."))
        finally:
            self._drain_lifecycle_events(lifecycle)
            self._active_lifecycle = None

    def enter_arcane(self, capture: ScreenCapture) -> None:
        run_arcane_entry(self, capture)
        self.events.put(self.event_class("info", "Summoner: arcane_entry completed."))

    def buff_before_run(self) -> None:
        run_arcane_pre_run_buffs(self)
        self.events.put(self.event_class("info", "Summoner: buff_before_run stage completed."))

    def run_north_go(self, capture: ScreenCapture) -> None:
        run_arcane_north_go(self, capture)

    def start_north_go_test(self) -> None:
        with self._lock:
            if self._is_running:
                raise RuntimeError("Summoner run is already running.")
            self._stop_event.clear()
            self._thread = threading.Thread(target=self._run_north_go_test, daemon=True)
            self._is_running = True
            self._thread.start()
        self.events.put(self.event_class("info", "Arcane North Test started from GUI button."))

    def _run_north_go_test(self) -> None:
        try:
            capture = ScreenCapture(self.config.capture)
            focus_game_window(self, capture)
            self.run_north_go(capture)
        except Exception as exc:  # pragma: no cover
            self.events.put(self.event_class("error", f"Arcane North Test failed: {exc}"))
        finally:
            with self._lock:
                self._thread = None
                self._is_running = False

    def _drain_lifecycle_events(self, lifecycle: RunLifecycleSession) -> None:
        while True:
            try:
                event = lifecycle.events.get_nowait()
            except Empty:
                break
            self.events.put(self.event_class(event.level, event.message))

    def _resolve_single_asset(self, directory: str, suffix: str) -> Path:
        base = Path(directory)
        matches = sorted(base.glob(suffix))
        if not matches:
            raise RuntimeError(f"Required asset is missing under {base.as_posix()} matching {suffix}.")
        return matches[0]

    def _resolve_act1_map_template_paths(self) -> tuple[Path, Path]:
        base = Path("assets/waypoint/act1")
        first = self._resolve_single_asset(base.as_posix(), "*_on_map_1.png")
        second = self._resolve_single_asset(base.as_posix(), "*_on_map_2.png")
        return first, second

    def _resolve_act1_waypoint_template_path(self) -> Path:
        base = Path("assets/waypoint/act1")
        candidates = [path for path in sorted(base.glob("*.png")) if "on_map" not in path.name and "when_hover" not in path.name and "list" not in path.name]
        if not candidates:
            raise RuntimeError("Required Act 1 waypoint template is missing.")
        return candidates[0]

    def _resolve_act1_waypoint_hover_template_path(self) -> Path:
        base = Path("assets/waypoint/act1")
        candidates = sorted(base.glob("*_when_hover.png"))
        if not candidates:
            raise RuntimeError("Required Act 1 waypoint hover template is missing.")
        return candidates[0]

    def _load_image(self, path: Path) -> np.ndarray:
        return load_image(path)

    def _load_optional_image(self, path: Path, label: str) -> np.ndarray | None:
        return load_optional_image(self, path, label)

    def _locate_template(self, frame: np.ndarray, template: np.ndarray, threshold: float):
        return locate_template(self, frame, template, threshold)

    def _sleep_range(self, low: float, high: float) -> None:
        sleep_range(self, low, high)

    def _press_key(self, key: str) -> None:
        press_key(self, key)

    def _hold_key_down(self, key: str) -> None:
        hold_key_down(self, key)

    def _hold_key_up(self, key: str) -> None:
        hold_key_up(self, key)

    def _aim_relative_ratio(self, capture, ratio_x: float, ratio_y: float, apply_jitter: bool = True) -> None:
        aim_relative_ratio(self, capture, ratio_x, ratio_y, apply_jitter)

    def _apply_offset(self, point: tuple[float, float], offset: tuple[float, float]) -> tuple[float, float]:
        return apply_offset(point, offset)

    def _center_right_anchor(self) -> tuple[float, float]:
        return center_right_anchor(self)

    def _check_for_user_interrupt(self) -> bool:
        return check_for_user_interrupt(self)

    def _resolve_arcane_character_actions(self):
        return resolve_arcane_character_actions(self)

    def _build_initial_arcane_belief_state(self):
        return build_initial_arcane_belief_state()

    def _commit_arcane_wing(self, state, wing) -> None:
        commit_arcane_wing(state, wing)

    def _log_arcane_state(self, state) -> None:
        log_arcane_state(self, state)

    def _scan_arcane_monsters(self, frame: np.ndarray) -> str | None:
        if self._arcane_monster_templates is None:
            self._arcane_monster_templates = load_arcane_monster_templates(self)
        return scan_arcane_monsters(self, frame)


SUMMONER_PAYLOAD_STATES: tuple[RunPayloadState, ...] = (
    RunPayloadState(
        key="room_created",
        description="The room is created and the character can act.",
        success_signal="Town is loaded and input is available.",
        next_action="Enter the payload organizer.",
    ),
    RunPayloadState(
        key="payload_started",
        description="The payload organizer has attached its threaded runtime for the active route piece.",
        success_signal="Capture, fast vision, slow vision, and decision loops are running.",
        next_action="Replay buffs and activate the selected route piece.",
    ),
    RunPayloadState(
        key="route_piece_active",
        description="A route piece such as north go owns movement while hunting and looting may interrupt.",
        success_signal="The current route piece continues making progress.",
        next_action="Keep deferring to route, hunting, loot, or escape based on the active run state.",
    ),
    RunPayloadState(
        key="summoner_encountered",
        description="The Summoner encounter or escape condition is confirmed.",
        success_signal="Boss or danger evidence is strong enough to change intent.",
        next_action="Switch to hunt, loot, or escape planning as needed.",
    ),
    RunPayloadState(
        key="payload_complete",
        description="The run payload is complete and ready to detach runtime threads.",
        success_signal="No more route, combat, or loot work is pending.",
        next_action="Detach runtime threads and hand off to exit-room logic.",
    ),
    RunPayloadState(
        key="room_exit_complete",
        description="The room exit wrapper has finished.",
        success_signal="The run is back at a safe handoff point.",
        next_action="Prepare the next run if scheduled.",
    ),
)

SUMMONER_ROUTE_SEGMENTS = (
    NORTH_GO,
    NORTH_RETURN,
    EAST_GO,
    EAST_RETURN,
    SOUTH_GO,
    SOUTH_RETURN,
    WEST_GO,
    WEST_RETURN,
)

SUMMONER_SHARED_PORTS: tuple[RunPort, ...] = (
    RunPort(
        key="make_room",
        kind="wrapper",
        description="Reusable room-creation wrapper that runs before the payload organizer.",
        status="ready",
    ),
    ARCANE_ENTRY,
    RunPort(
        key="buff_before_run",
        kind="payload",
        description="Reusable pre-run buff piece invoked when the payload starts.",
        status="ready",
    ),
    RunPort(
        key="hunting",
        kind="payload",
        description="Combat decision piece that can temporarily preempt route movement.",
        status="planned",
    ),
    RunPort(
        key="looting",
        kind="payload",
        description="Loot decision piece that can temporarily preempt route movement.",
        status="planned",
    ),
    RunPort(
        key="escape_plan",
        kind="payload",
        description="Encounter or danger escape piece, including Summoner-specific bail-out logic.",
        status="planned",
    ),
    RunPort(
        key="exit_room",
        kind="wrapper",
        description="Reusable room-exit wrapper that runs after the payload organizer detaches runtime threads.",
        status="planned",
    ),
    RunPort(
        key="threaded_payload_runtime",
        kind="runtime",
        description="Attach capture, fast vision, slow vision, and decision threads only during payload execution, then detach them at payload end.",
        status="ready",
        notes=(
            "The current north_go test is the first user of this runtime.",
            "Future run organizers like diablo should reuse the same runtime contract.",
        ),
    ),
)


def build_summoner_definition(config: BotConfig) -> RunDefinition:
    profile = config.run_profiles.get(SUMMONER_PROFILE_ID)
    if profile is None:
        raise RuntimeError(f"Summoner Run requires the '{SUMMONER_PROFILE_ID}' profile in run_profiles.")
    return RunDefinition(
        profile_id=SUMMONER_PROFILE_ID,
        profile=profile,
        payload_states=SUMMONER_PAYLOAD_STATES,
        route_segments=SUMMONER_ROUTE_SEGMENTS,
        shared_ports=SUMMONER_SHARED_PORTS,
        metadata={
            "organizer": "runs/summoner/summoner_run.py",
            "route_owner": "individual route files under runs/summoner",
            "current_stage": "make_room -> arcane_entry -> buff_before_run",
        },
    )


def resolve_summoner_run(config: BotConfig) -> SummonerRunContext:
    return SummonerRunContext(profile_id=SUMMONER_PROFILE_ID, definition=build_summoner_definition(config))


def build_summoner_orchestrator(config: BotConfig) -> SummonerRunOrchestrator:
    return SummonerRunOrchestrator(config)


def iter_summoner_payload_states() -> tuple[RunPayloadState, ...]:
    return SUMMONER_PAYLOAD_STATES


def summarize_summoner_payload(context: SummonerRunContext) -> list[str]:
    return context.definition.summarize()
