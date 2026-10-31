"""필드 준비 절차의 시각·호출 순서만 재생한다. OS 입력/캡처는 사용하지 않는다."""
import json
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import cv2 as cv
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT))
from diablo2.common import automatic_field as automatic
from diablo2.common import supervised_field as supervised
from diablo2.common.survival import CharacterSurvivalPolicy
from diablo2.common.survival_vision import HudReading, ResourceReading, SlotReading


class Environment:
    def __init__(self, case):
        self.case, self.now, self.index, self.events = case, case.get("now", 10.0), -1, []
        self.stopped = False
        self.stop = SimpleNamespace(is_set=lambda: self.stopped, wait=self.wait)
        self.capture = SimpleNamespace(grab=self.grab)
        self.hud = SimpleNamespace(observe=self.read)
        self.backend = SimpleNamespace(press=self.press)
        self.observer = SimpleNamespace(visible=self.visible, last_evidence=None, last_obscured=False)

    def current(self):
        frames = self.case.get("frames", [{}])
        return frames[min(max(0, self.index), len(frames) - 1)]

    def grab(self):
        self.index += 1
        self.events.append(["capture", self.index, self.now])
        self.now += self.current().get("capture_delay", 0.0)
        frame = SimpleNamespace(index=self.index, shape=(1140, 1922, 3)) if self.case["kind"] == "loot" else self.index
        return SimpleNamespace(frame=frame)

    def read(self, frame):
        self.events.append(["hud", self.index])
        self.now += self.current().get("hud_delay", 0.0)

        def resource(name):
            sample = self.current()
            if name + "_lower_bound" in sample:
                return ResourceReading(reason="orb", lower_bound=sample[name + "_lower_bound"])
            value = sample.get(name, 90)
            return ResourceReading(value, sample.get(name + "_maximum", 100), "read") if value is not None else ResourceReading()

        return HudReading(
            "test",
            "1",
            resource("life"),
            resource("mana"),
            self.current().get("belt_status", "read"),
            tuple(SlotReading(**slot) for slot in self.current().get("slots", [])),
            belt_visibility=self.current().get("belt", "closed"),
            mercenary_status=self.current().get("merc"),
        )

    def focused(self):
        self.events.append(["focus", self.now])
        return self.now < self.case.get("focus_lost_at", 1e9)

    def wait(self, seconds):
        self.events.append(["wait", seconds])
        self.now += self.case.get("wait_advance", seconds)
        self.stopped = self.now >= self.case.get("stop_at", 1e9)
        return self.stopped

    def press(self, key):
        self.events.append(["press", key, self.now])
        if self.case.get("fail_key") == key or self.case.get("fail_press_number") == sum(row[0] == "press" for row in self.events):
            raise RuntimeError("backend request failed")
        self.now += self.case.get("press_delay", 0.0)
        if self.case.get("stop_on_key") == key:
            self.stopped = True
        if self.case.get("focus_after_key") == key:
            self.case["focus_lost_at"] = self.now

    def panels(self, frame):
        self.events.append(["panels", self.index])
        self.now += self.current().get("panels_delay", 0.0)
        return self.current().get("panels", False)

    def visible(self, frame, area):
        self.events.append(["automap", self.index, area])
        self.now += self.current().get("map_delay", 0.0)
        self.observer.last_obscured = self.current().get("obscured", False)
        self.observer.last_evidence = dict(sample=self.index)
        return self.current().get("visible", False)

    def weapon(self, frame):
        self.events.append(["weapon", self.index])
        self.now += self.current().get("weapon_delay", 0.0)
        return self.current().get("weapon")

    def observe(self, frame, acquired):
        self.events.append(["observe", self.index, acquired])
        self.now += self.current().get("observe_delay", 0.0)

    def check(self, frame, acquired, reading):
        self.events.append(["check", self.index, acquired])
        self.now += self.current().get("check_delay", 0.0)
        return self.current().get("blocked")

    def before(self):
        self.events.append(["before", self.now])
        self.now += self.case.get("before_delay", 0.0)
        return self.case.get("before_blocked")


def exercise(case, policy):
    kind = case["kind"]
    if kind in ("reuse", "renew", "renew_deadlines", "probe"):
        return renewal_case(case, policy)
    if kind == "automatic":
        return automatic_case(case, policy)
    if kind == "site":
        return site_case(case, policy)
    if kind == "recovery":
        return recovery_case(case, policy)
    if kind == "loot":
        return loot_case(case, policy)
    if kind in ("runtime", "permit", "travel", "confirm"):
        dict(
            runtime=lambda: automatic.validate_runtime_permit(case["proof"], case["now"]),
            permit=lambda: supervised.validate_permit(case["proof"], "flash", policy, case["now"], buff=case.get("buff", False)),
            travel=lambda: __import__(
                "diablo2.runs.summoner.continuous_navigation", fromlist=["validate_travel_permit"]
            ).validate_travel_permit(case["proof"], policy, case["now"]),
            confirm=lambda: supervised.confirm_initial_probe(case["proof"], "flash", case["last_cast"], case["now"]),
        )[kind]()
        return None
    env = Environment(case)
    result, error = None, None
    trace = [] if case.get("trace") else None
    with patch.object(supervised.time, "monotonic", lambda: env.now), patch.object(
        supervised.time, "time", lambda: 1000.0 + env.now
    ), patch.dict(supervised.wait_readable_resources.__kwdefaults__, clock=lambda: env.now):
        try:
            if kind == "wait":
                result = supervised.wait_readable_resources(
                    env.capture,
                    env.hud,
                    env.focused,
                    policy,
                    env.stop,
                    case.get("deadline", 11.0),
                    clock=lambda: env.now,
                    observe_frame=env.observe,
                    check_frame=env.check,
                    retry_reasons=case.get("retry", ()),
                )
            elif kind == "dispatch":
                profile = SimpleNamespace(
                    survival=policy,
                    actions=SimpleNamespace(pre_run_buff_order=case["order"], buff_action_pause_seconds=case.get("pauses", {})),
                )
                result = supervised.dispatch_buffs(
                    env.capture,
                    env.hud,
                    env.backend,
                    env.focused,
                    profile,
                    env.stop,
                    trace_frames=trace,
                    trace_before_dispatch=case.get("trace_before", False),
                    observation_wait=case.get("observation_wait", 0.75),
                    sequence_timeout=case.get("sequence_timeout", 8.0),
                    before_step=env.before,
                    before_step_frame=env.check,
                )
            elif kind == "map":
                result = supervised.ensure_automap_on(
                    env.capture,
                    env.backend,
                    env.focused,
                    env.stop,
                    env.hud,
                    policy,
                    env.observer,
                    env.panels,
                    clock=lambda: env.now,
                    supervised_on=case.get("proof"),
                    allow_toggle=case.get("allow_toggle", True),
                    timeout=case.get("timeout", 0.4),
                )
            elif kind == "labels":
                result = supervised.enable_fresh_room_item_labels(
                    env.capture, env.backend, env.focused, env.stop, env.hud, policy, env.panels, clock=lambda: env.now
                )
            elif kind == "weapon":
                result = supervised.ensure_weapon_set(
                    env.capture,
                    env.backend,
                    env.focused,
                    env.stop,
                    SimpleNamespace(observe=env.weapon),
                    case.get("expected", 1),
                    timeout=case.get("timeout", 0.1),
                )
            else:
                raise ValueError("unknown oracle kind")
        except (ValueError, RuntimeError, AttributeError, TypeError) as caught:
            error = dict(kind=type(caught).__name__, message=str(caught))
    return dict(
        result=result,
        error=error,
        events=env.events,
        now=env.now,
        trace=[] if trace is None else [dict(frame=frame, metadata=metadata) for frame, metadata in trace],
    )


def automatic_case(case, policy):
    env = Environment(case)
    attempts, observations, operation = [0], [0], [{}]

    def create():
        index = attempts[0]
        attempts[0] += 1
        env.events.append(["create", index])

        def prepare():
            env.events.append(["prepare", index])
            env.now += operation[0].get("prepare_delay", 0)
            errors = operation[0].get("prepare_errors", [])
            error = errors[index] if index < len(errors) else None
            if error:
                raise RuntimeError(error)
            return dict(guard=index)

        return SimpleNamespace(prepare=prepare, wait_check=index)

    def clear():
        env.events.append(["clear", env.now])
        env.now += operation[0].get("clear_delay", 0)

    def observe():
        index = observations[0]
        observations[0] += 1
        env.events.append(["observation", index, env.now])
        env.now += operation[0].get("observe_delay", 0)
        return dict(sample=index)

    def dispatch(guard):
        env.events.append(["dispatch", guard, env.now])
        op = operation[0]
        at = env.now
        env.now += op.get("dispatch_delay", 0)
        events = op.get("dispatches", [dict(key=rule.key, cast_request_at=1000 + at, cast_request_monotonic=at) for rule in policy.buffs])
        events = [dict(row) for row in events]
        for row in events:
            if row.get("cast_request_monotonic") in ("nan", "inf"):
                row["cast_request_monotonic"] = float(row["cast_request_monotonic"])
        return dict(
            status=op.get("status", "dispatched_unconfirmed"),
            equipment_dispatch_balanced=op.get("balanced", True),
            buff_dispatches=events,
            effect_verified=False,
            held_after=[],
            errors=[],
            trace_phase="after_dispatch",
        )

    profile = SimpleNamespace(actions=SimpleNamespace(pre_run_buff_order=case.get("order", ["w", "s", "w"])), survival=policy)
    driver = automatic.AutomaticBuffDriver(profile, create, dispatch, observe, clear, env.stop, clock=lambda: env.now)
    rows = []
    for op in case["operations"]:
        operation[0] = op
        attempts[0] = 0
        env.now = op.get("now", env.now)
        env.stopped = op.get("stop", False)
        result, error = None, None
        try:
            result = driver.execute()
        except (ValueError, RuntimeError) as caught:
            error = dict(kind=type(caught).__name__, message=str(caught))
        rows.append(
            dict(
                result=result,
                error=error,
                deadlines=driver.deadlines[:],
                preparation_at=driver.preparation_at,
                due=driver.due(),
                now=env.now,
            )
        )
    return dict(rows=rows, events=env.events)


def site_case(case, policy):
    from diablo2.runs.summoner.main_run import BuffSiteGuard

    env = Environment(case)

    def ground(frame, captured_at):
        env.events.append(["ground", env.index, captured_at])
        env.now += env.current().get("ground_delay", 0)
        return SimpleNamespace(point=env.current().get("ground", [960, 580]))

    def prepare(frame, acquired, reading, focused):
        env.events.append(["prepare_ground", env.index, acquired, focused])
        env.now += env.current().get("prepare_ground_delay", 0)

    def hazards(frame, point, acquired, processed):
        env.events.append(["hazards", env.index, point, acquired, processed])
        env.now += env.current().get("hazard_delay", 0)
        return [SimpleNamespace(ground=value) for value in env.current().get("hazards", [])]

    ground_observer = SimpleNamespace(observe=ground)
    if case.get("camera"):
        ground_observer.prepare_frame = prepare
    guard = BuffSiteGuard(
        env.capture,
        env.hud,
        env.focused,
        SimpleNamespace(survival=policy),
        env.stop,
        ground_observer,
        SimpleNamespace(observe=hazards),
        clock=lambda: env.now,
        supervised_ground=case.get("proof"),
    )
    rows = []
    with patch.object(supervised.time, "time", lambda: 1000.0 + env.now):
        for op in case["operations"]:
            env.now = op.get("now", env.now)
            env.stopped = op.get("stop", env.stopped)
            result, error = None, None
            try:
                if op.get("method") == "prepare":
                    result = guard.prepare()
                else:
                    arguments = ()
                    if op.get("supplied"):
                        acquired = env.now
                        frame = env.grab().frame
                        reading = env.read(frame)
                        env.now += op.get("delay", 0)
                        arguments = (frame, None if op.get("missing_at") else acquired, None if op.get("missing_reading") else reading)
                    result = (guard.wait_check if op.get("method") == "wait" else guard.check)(*arguments)
            except (ValueError, RuntimeError) as caught:
                error = dict(kind=type(caught).__name__, message=str(caught))
            rows.append(
                dict(
                    result=result,
                    error=error,
                    baseline=guard.baseline,
                    samples=guard.samples[:],
                    observations=guard.observations[:],
                    now=env.now,
                )
            )
    return dict(rows=rows, events=env.events)


def recovery_case(case, policy):
    from diablo2.common.emergency_recovery import EmergencyResourceRecovery

    env = Environment(case)
    records, evidence, rows = [], [], []

    def closed(frame, reading):
        env.events.append(["closed_slots", env.index])
        env.now += env.current().get("closed_delay", 0)
        return [SlotReading(**slot) for slot in env.current().get("closed_slots", [])]

    env.hud.closed_belt_slots = closed
    recovery = EmergencyResourceRecovery(
        env.capture, env.hud, env.backend, env.focused, policy, env.stop, records=records, evidence=evidence, clock=lambda: env.now
    )
    for op in case.get("operations", [{}]):
        acquired = env.now
        frame = env.grab().frame
        reading = env.read(frame)
        env.now += op.get("delay", 0)
        result, error = None, None
        try:
            result = recovery(frame, reading, acquired)
        except (ValueError, RuntimeError) as caught:
            error = dict(kind=type(caught).__name__, message=str(caught))
        rows.append(dict(result=result, error=error))
    return dict(
        rows=rows, records=records, evidence=[dict(filename=name, frame=frame) for name, frame in evidence], events=env.events, now=env.now
    )


def loot_case(case, policy):
    from diablo2.common import conditional_loot as loot
    from diablo2.actions.loot_pickup import LootScanHit, MatchResult

    env = Environment(case)
    origin = case.get("origin", [-1922, 0])
    env.capture.target = dict(left=origin[0], top=origin[1])
    pointer, scans = [None], [0]

    def candidates():
        env.events.append(["build"])
        if case.get("build_error"):
            raise ValueError("candidate build failed")
        return [SimpleNamespace(label=label) for label in case.get("candidates", ["key of hate"])]

    def scan(frame, allowed):
        index = scans[0]
        scans[0] += 1
        env.events.append(["scan", env.index, [candidate.label for candidate in allowed]])
        env.now += env.current().get("scan_delay", 0)
        if env.current().get("scan_error"):
            raise ValueError("label scan failed")
        values = case.get("hits", [])
        value = values[index] if index < len(values) else case.get("repeat_hit")
        return (
            None
            if value is None
            else LootScanHit(
                value.get("label", "key of hate"),
                MatchResult(tuple(value.get("top_left", [700, 400])), value.get("width", 100), value.get("height", 20), 0.99),
            )
        )

    def move(x, y):
        env.events.append(["move", [x, y], env.now])
        pointer[0] = [x, y]
        env.now += case.get("move_delay", 0)

    def matches(point):
        env.events.append(["pointer", list(point), env.now])
        env.now += case.get("pointer_delay", 0)
        return not case.get("pointer_drift", False) and list(point) == pointer[0]

    def click():
        env.events.append(["click", env.now])
        if case.get("click_error"):
            raise RuntimeError("click failed")
        env.now += case.get("click_delay", 0)

    def deadline():
        env.events.append(["deadline"])
        value = case.get("deadline")
        return float(value) if value in ("nan", "inf") else value

    def count():
        env.events.append(["count"])
        return case.get("potion_count")

    def recover(frame, reading, acquired):
        env.events.append(["recovery", env.index, acquired])
        env.now += case.get("recovery_delay", 0)
        return case.get("recover", False)

    def observe_ground(frame, captured_at):
        env.events.append(["loot_ground", env.index, captured_at])
        env.now += env.current().get("ground_delay", 0)
        return SimpleNamespace(point=env.current().get("ground", [750, 450]), reason=env.current().get("ground_reason"))

    def prepare_ground(frame, acquired, reading, focused):
        env.events.append(["prepare_ground", env.index, acquired, focused])
        env.now += env.current().get("prepare_ground_delay", 0)

    def ground_supplier():
        env.events.append(["ground_available"])
        if case.get("ground_observer", True) is False:
            return None
        observer = SimpleNamespace(observe=observe_ground)
        if case.get("camera"):
            observer.prepare_frame = prepare_ground
        return observer

    mapper = loot.frame_to_desktop

    def to_desktop(capture, point):
        env.events.append(["map", list(point), env.now])
        if case.get("mapper_error"):
            raise RuntimeError("geometry changed")
        return mapper(capture, point)

    env.backend.move, env.backend.pointer_matches = move, matches
    env.backend.click = click if case.get("supports_click", True) else None
    controller = loot.ConditionalLootController(
        SimpleNamespace(shared_loot=SimpleNamespace(ignore_labels=case.get("ignored", []))),
        env.capture,
        env.hud,
        env.backend,
        env.focused,
        SimpleNamespace(survival=policy, actions=SimpleNamespace(movement_skill_key="f2")),
        env.stop,
        env.panels,
        clock=lambda: env.now,
        scanner=SimpleNamespace(_build_candidates=candidates, scan_frame=scan),
        deadline_supplier=deadline,
        potion_count_supplier=count,
        ground_supplier=ground_supplier,
        recover_resources=recover if "recover" in case else None,
        max_scan_seconds=case.get("scan_seconds", 3),
        max_scan_frames=case.get("scan_frames", 12),
        max_clicks=case.get("clicks", 2),
        cast_reference_seconds=case.get("reference", 0.32),
    )
    rows = []
    with patch.object(loot, "frame_to_desktop", to_desktop):
        for op in case.get("operations", [dict(method="prepare", fresh_room=True), dict(method="scan")]):
            result, error = None, None
            try:
                if op["method"] == "prepare":
                    result = controller.prepare(fresh_room=op.get("fresh_room", True), labels_on=op.get("labels_on"))
                else:
                    result = controller.after_combat(
                        buff_expires_at=op.get("buff"), reason=op.get("reason", "combat_ended_scan"), combat_id=op.get("combat_id")
                    )
            except (ValueError, RuntimeError) as caught:
                error = dict(kind=type(caught).__name__, message=str(caught))
            if error is not None and error["kind"] == "_LootAbort":
                # 비공개 RuntimeError 하위형은 Rust의 내부 Abort/외부 runtime 오류로 대응한다.
                error["kind"] = "RuntimeError"
            rows.append(dict(result=result, error=error))

    def evidence(value):
        return None if value is None else dict(frame=value[0].index, metadata=value[1])

    return dict(
        rows=rows,
        records=controller.records,
        labels_state=controller.labels_state,
        movement_allowed=controller.movement_allowed,
        resource_exit_required=controller.resource_exit_required,
        hud_failure=evidence(controller.last_hud_failure),
        landing=evidence(controller.last_landing_observation),
        events=env.events,
        now=env.now,
    )


def renewal_case(case, policy):
    from queue import Empty
    from diablo2.runs.summoner import main_run

    if case["kind"] == "reuse":
        return main_run.reusable_buffs(case["report"], SimpleNamespace(survival=policy), wall_now=case["wall"], monotonic_now=case["mono"])
    if case["kind"] == "renew_deadlines":
        return main_run.buff_renewal_deadlines(case["dispatched"], SimpleNamespace(survival=policy))
    env = Environment(case)
    signal_index = [0]
    emits = []

    class Signals:
        def get_nowait(self):
            i = signal_index[0]
            env.events.append(["signal", i, env.now])
            rows = case.get("signals", [])
            if i >= len(rows) or env.now < rows[i].get("available_at", 0):
                raise Empty()
            signal_index[0] += 1
            row = dict(rows[i])
            if "observed_age" in row:
                row["observed_at"] = 1000.0 + env.now - row.pop("observed_age")
            return row

    def ground(frame, captured_at):
        env.events.append(["ground", env.index, captured_at])
        env.now += env.current().get("ground_delay", 0)
        return SimpleNamespace(point=env.current().get("ground", [960, 580]))

    def hazards(frame, point, acquired, processed):
        env.events.append(["hazards", env.index, point, acquired, processed])
        env.now += env.current().get("hazard_delay", 0)
        return [SimpleNamespace(ground=value) for value in env.current().get("hazards", [])]

    def wait(*args, **kwargs):
        return supervised.wait_probe_signal(*args, clock=lambda: env.now, **kwargs)

    original_guard = main_run.BuffSiteGuard

    def guard(*args, **kwargs):
        return original_guard(*args, clock=lambda: env.now, **kwargs)

    record, evidence, result, error = {}, [], None, None
    with patch.object(main_run.time, "monotonic", lambda: env.now), patch.object(
        main_run.time, "time", lambda: 1000.0 + env.now
    ), patch.dict(supervised.wait_readable_resources.__kwdefaults__, clock=lambda: env.now), patch.object(
        main_run, "wait_probe_signal", wait
    ), patch.object(
        main_run, "BuffSiteGuard", guard
    ):
        try:
            if case["kind"] == "probe":

                def accept(signal):
                    main_run.validate_buff_site_permit(signal, policy, 1000.0 + env.now)

                result = wait(
                    Signals(),
                    capture=env.capture,
                    hud=env.hud,
                    focused=env.focused,
                    policy=policy,
                    stop=env.stop,
                    deadline=case.get("deadline", 11.0),
                    phase=case.get("phase", "field_safety"),
                    accept=accept,
                    retry_unverified=case.get("retry_unverified", False),
                    retry_stale=case.get("retry_stale", False),
                    emit=emits.append,
                )
            else:
                result = main_run.renew_supervised_field_buffs(
                    Signals(),
                    env.capture,
                    env.hud,
                    env.backend,
                    env.focused,
                    SimpleNamespace(
                        survival=policy,
                        actions=SimpleNamespace(pre_run_buff_order=case["order"], buff_action_pause_seconds=case.get("pauses", {})),
                    ),
                    env.stop,
                    SimpleNamespace(observe=ground),
                    SimpleNamespace(observe=hazards),
                    record=record,
                    evidence=evidence,
                    prefix="renew",
                    emit=emits.append,
                )
        except (ValueError, RuntimeError) as caught:
            error = dict(kind=type(caught).__name__, message=str(caught))
    return dict(
        result=result,
        error=error,
        record=record,
        evidence=[dict(file=name, frame=frame) for name, frame in evidence],
        events=env.events,
        emits=emits,
        now=env.now,
    )


def main():
    baseline = json.loads(Path(__file__).with_name("motion-baseline.json").read_text(encoding="utf-8"))
    if (sys.version.split()[0], np.__version__, cv.__version__) != (baseline["python"], baseline["numpy"], baseline["opencv"]):
        raise RuntimeError("field preparation baseline changed")
    request = json.load(sys.stdin)
    policy = CharacterSurvivalPolicy.from_dict(request["policy"])
    rows = []
    for case in request["cases"]:
        try:
            selected = CharacterSurvivalPolicy.from_dict(case["policy"]) if "policy" in case else policy
            rows.append(exercise(case, selected))
        except (ValueError, RuntimeError, AttributeError, TypeError) as caught:
            rows.append(dict(kind=type(caught).__name__, message=str(caught)))
    print(json.dumps(rows, ensure_ascii=False, allow_nan=False))


if __name__ == "__main__":
    main()
