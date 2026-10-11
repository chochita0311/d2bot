from __future__ import annotations

import json
import math
import threading
import time
from dataclasses import asdict
from pathlib import Path
from queue import Empty

from diablo2.common.automatic_field import AutomaticBuffDriver, CooperativeStop, validate_runtime_permit

from diablo2.common.supervised_field import (
    RecordingInput,
    confirm_initial_probe,
    dispatch_buffs,
    ensure_automap_on,
    enable_fresh_room_item_labels,
    read_probe_signals,
    resource_guard,
    validate_permit,
    wait_probe_signal,
)
from diablo2.runs.summoner.navigation_execution import (
    dispatch_planned_step,
    load_arcane_step_planner,
    observe_step_landing,
    plan_arcane_step,
)


def reusable_buffs(report, profile, *, wall_now, monotonic_now):
    """같은 지형 재개 시 실제 시전 시각을 보존한다. 새 효과 확인은 별도로 받는다."""
    proof = report.get("supervised_effect", {})
    buff = report.get("buff", {})
    if (
        report.get("safe_exit") is not None
        or proof.get("character_id") != "flash"
        or proof.get("source") != "supervised_visual"
        or any(proof.get(name) is not True for name in ("effect_verified", "equipment_restored", "sequence_effects_verified"))
        or buff.get("status") != "dispatched_unconfirmed"
    ):
        return None
    deadlines = []
    for rule in profile.survival.buffs:
        requested = next((item for item in reversed(buff.get("buff_dispatches", [])) if item.get("key") == rule.key), None)
        if requested is None:
            return None
        wall, mono = requested.get("cast_request_at"), requested.get("cast_request_monotonic")
        if any(type(v) not in (int, float) or not math.isfinite(v) for v in (wall, mono, proof.get("observed_at"))):
            return None
        elapsed, monotonic_elapsed = wall_now - wall, monotonic_now - mono
        duration = rule.duration_seconds - rule.renew_before_seconds
        if not 0 <= elapsed < duration - 5 or not 0 <= monotonic_elapsed < duration - 5 or abs(elapsed - monotonic_elapsed) > 2:
            return None
        if proof["observed_at"] < wall:
            return None
        deadlines.append(mono + duration)
    return (buff, deadlines) if deadlines else None


def validate_buff_site_permit(signal, policy, now):
    """원거리 적 유무와 분리하여 사람이 확인한 근접 안전 범위를 검증한다."""
    if signal.get("buff_safety_scope") == "nearby":
        if signal.get("source") != "supervised_visual" or signal.get("nearby_monsters_clear") is not True:
            raise ValueError("fresh supervised nearby-clear confirmation required")
        validate_permit(dict(signal, monsters_clear=True), "flash", policy, now, buff=True)
    else:
        validate_permit(signal, "flash", policy, now, buff=True)


def buff_renewal_deadlines(dispatched, profile):
    deadlines = []
    for buff in profile.survival.buffs:
        requested = next((item for item in reversed(dispatched["buff_dispatches"]) if item["key"] == buff.key), None)
        if requested is None:
            raise RuntimeError("timed buff request missing: " + buff.name)
        deadlines.append(requested["cast_request_monotonic"] + buff.duration_seconds - buff.renew_before_seconds)
    if not deadlines:
        raise RuntimeError("timed buff policy required")
    return deadlines


def renew_supervised_field_buffs(
    signals, capture, hud, backend, focused, profile, stop, ground, hazards, *, record, evidence, prefix, emit=print
):
    """같은 main의 경로 기억을 유지하며 새 감독 안전/효과 근거 사이에 전체 버프만 갱신한다."""

    def accept_safety(signal):
        validate_buff_site_permit(signal, profile.survival, time.time())
        if signal.get("area") != "arcane_sanctuary":
            raise ValueError("current Arcane area confirmation required")

    def wait(phase, accept):
        return wait_probe_signal(
            signals,
            capture=capture,
            hud=hud,
            focused=focused,
            policy=profile.survival,
            stop=stop,
            deadline=time.monotonic() + 300,
            phase=phase,
            accept=accept,
            retry_unverified=True,
            retry_stale=True,
            emit=emit,
        )

    emit("AWAIT_BUFF_RENEWAL_SAFETY: fresh field_safety JSON; current route retained; no movement or weapon-panel input")
    permit = wait("field_safety", accept_safety)
    record["safety"] = permit
    guard = BuffSiteGuard(capture, hud, focused, profile, stop, ground, hazards, supervised_ground=permit)
    record["stability"] = guard.prepare()
    accept_safety(permit)
    trace = []
    dispatched = dispatch_buffs(
        capture,
        hud,
        backend,
        focused,
        profile,
        stop,
        trace_frames=trace,
        observation_wait=2,
        sequence_timeout=12,
        before_step=guard.wait_check,
    )
    record["buff"] = dispatched
    record["resource_observations"] = guard.observations
    evidence.extend((prefix + "-step-{:02d}.png".format(metadata["index"]), frame) for frame, metadata in trace)
    if dispatched["status"] != "dispatched_unconfirmed":
        raise RuntimeError("buff renewal dispatch: " + dispatched["status"])

    def accept_effect(signal):
        confirm_initial_probe(signal, "flash", dispatched["buff_dispatches"][-1]["cast_request_at"], time.time())
        accept_safety(signal)
        if time.monotonic() >= min(buff_renewal_deadlines(dispatched, profile)):
            raise RuntimeError("renewed buff already due before effect confirmation")

    emit("AWAIT_BUFF_RENEWAL_EFFECT: fresh initial_effect JSON; dispatch alone does not confirm renewal")
    proof = wait("initial_effect", accept_effect)
    record["effect"] = proof
    record["deadlines"] = buff_renewal_deadlines(dispatched, profile)
    return proof, record["deadlines"]


class BuffSiteGuard:
    """같은 화면의 비율과 근접 적 긍정 후보를 검사한다. 미검출을 안전 승인으로 바꾸지 않는다."""

    def __init__(self, capture, hud, focused, profile, stop, ground, hazards, *, clock=time.monotonic, supervised_ground=None):
        self.capture, self.hud, self.focused = capture, hud, focused
        self.profile, self.stop, self.ground, self.hazards, self.clock = profile, stop, ground, hazards, clock
        self.baseline, self.samples = None, []
        self.supervised_ground = supervised_ground
        self.observations = []

    def check(self):
        from diablo2.runs.summoner.continuous_navigation import near_ground

        if self.stop.is_set() or not self.focused():
            return "interrupted_or_unfocused"
        acquired = self.clock()
        frame = self.capture.grab().frame
        reading = self.hud.observe(frame)
        blocked = resource_guard(reading, self.profile.survival)
        if blocked:
            return blocked
        prepare_ground = getattr(self.ground, "prepare_frame", None)
        if prepare_ground is not None:
            prepare_ground(frame, acquired, reading, self.focused())
        ground = self.ground.observe(frame, captured_at=acquired)
        point = ground.point
        if point is None and self.supervised_ground is not None:
            proof = self.supervised_ground
            candidate = proof.get("player_ground")
            if (
                proof.get("source") == "supervised_visual"
                and type(proof.get("observed_at")) in (int, float)
                and 0 <= time.time() - proof["observed_at"] <= 15
                and isinstance(candidate, list)
                and len(candidate) == 2
                and all(type(value) is int for value in candidate)
                and 24 <= candidate[0] < 1900
                and 55 <= candidate[1] < 900
            ):
                # 이동 입력이 없는 초기 버프 구간에만 최신 감독 발밑을 사용한다. 이동 플래너에는 전달하지 않는다.
                point = tuple(candidate)
        if point is None:
            return "buff_ground_unverified"
        candidates = self.hazards.observe(frame, point, acquired, self.clock())
        if any(item.ground is None or near_ground(item.ground, point) for item in candidates):
            return "buff_nearby_hostile"
        if self.clock() - acquired >= 0.25:
            return "screen_stale"
        bounds = tuple(
            (resource.ratio, resource.ratio)
            if resource.current is not None and resource.maximum is not None
            else (resource.lower_bound, 1.0)
            for resource in (reading.life, reading.mana)
        )
        self.observations.append(dict(at=acquired, life_ratio_bounds=bounds[0], mana_ratio_bounds=bounds[1]))
        self.observations = self.observations[-32:]
        recent = [before for at, before in self.samples if acquired - at <= 1]
        # 구체의 '90% 이상'은 [0.9, 1]이다. 정확한 90%로 바꾸어 가짜 급락을 만들지 않는다.
        if self.baseline is not None and any(
            current[1] < before[index][0] - 0.05 for before in recent for index, current in enumerate(bounds)
        ):
            return "buff_resources_unstable"
        self.samples.append((acquired, bounds))
        if self.baseline is not None:
            self.samples = [(at, value) for at, value in self.samples if acquired - at <= 1]
        return None

    def wait_check(self):
        deadline = self.clock() + 0.75
        while True:
            blocked = self.check()
            if blocked not in ("observation_unverified", "buff_ground_unverified", "screen_stale") or self.clock() >= deadline:
                return blocked
            self.stop.wait(0.02)

    def prepare(self):
        began = self.clock()
        while self.clock() - began < 8:
            blocked = self.check()
            if blocked in ("observation_unverified", "buff_ground_unverified", "screen_stale"):
                self.samples = []
            elif blocked:
                raise RuntimeError(blocked)
            if self.samples and self.samples[-1][0] - self.samples[0][0] >= 1:
                values = list(zip(*(bounds for _, bounds in self.samples)))
                if all(max(value[0] for value in series) - min(value[1] for value in series) <= 0.05 for series in values):
                    self.baseline = self.samples[-1][1]
                    return dict(
                        window_seconds=self.samples[-1][0] - self.samples[0][0],
                        maximum_confirmed_ratio_change=0.05,
                        ratio_bounds=self.baseline,
                    )
                self.samples = self.samples[-1:]
            self.stop.wait(0.02)
        raise RuntimeError("buff_resource_stability_timeout")


def clear_buff_site(capture, hud, backend, focused, profile, stop, reference_seconds, *, evidence, clock=time.monotonic):
    """현재 근접 적 후보에 제한된 노바를 사용한다. 공격 종료를 주변 정리로 승인하지 않는다."""
    from diablo2.common.field_control import FieldDirective, HeldFieldInput

    started = clock()
    held = HeldFieldInput(
        backend, profile.actions.movement_skill_key, profile.actions.primary_attack_skill_key, clock=clock, repeat_seconds=reference_seconds
    )
    watchdog_stop, watchdog_errors = threading.Event(), []

    def watchdog():
        while not watchdog_stop.wait(0.02):
            try:
                if stop.is_set() or not focused() or clock() - started >= 6:
                    held.release()
                else:
                    held.expire(clock())
            except Exception as exc:
                watchdog_errors.append(str(exc))
                stop.set()
                return

    worker = threading.Thread(target=watchdog, name="summoner-buff-site-watchdog", daemon=True)
    worker.start()
    frames, last, unavailable_since = 0, None, None
    try:
        while clock() - started < 6:
            if stop.is_set() or not focused():
                raise RuntimeError("buff-site combat interrupted or unfocused")
            acquired = clock()
            last = capture.grab().frame
            blocked = resource_guard(hud.observe(last), profile.survival)
            if blocked is not None:
                held.release()
                if blocked == "resource_low":
                    raise RuntimeError(blocked)
                unavailable_since = acquired if unavailable_since is None else unavailable_since
                if clock() - unavailable_since >= 1.5:
                    raise RuntimeError(blocked)
                stop.wait(0.02)
                continue
            unavailable_since = None
            if clock() - acquired >= 0.25:
                held.release()
                continue
            if not frames:
                evidence.append(("buff-site-combat-before.png", last))
            held.apply(FieldDirective("combat", "supervised_hostile_before_buff", deadline=acquired + 0.25), clock())
            frames += 1
            stop.wait(0.02)
    finally:
        watchdog_stop.set()
        worker.join(timeout=1)
        held.close()
        if last is not None:
            evidence.append(("buff-site-combat-after.png", last))
    if watchdog_errors:
        raise RuntimeError("buff-site watchdog: " + watchdog_errors[0])
    return dict(
        status="attack_finished_clear_unconfirmed",
        frames=frames,
        attack_dispatches=held.snapshot()["dispatches"].get(profile.actions.primary_attack_skill_key, 0),
    )


def maintain_paused_field(
    capture,
    hud,
    focused,
    profile,
    stop,
    *,
    report,
    evidence,
    clock=time.monotonic,
    backend=None,
    ground=None,
    hazards=None,
    buff_expires_at=None,
    engage_monsters=True,
    target_vision=None,
    recover_resources=None,
    renew_buffs=None,
    loot_controller=None,
):
    """경로 정지는 게임 정지가 아니다. 사람이 인계받거나 실제 생존 위험이 생길 때까지 관찰한다."""
    started, missing_since, last = clock(), None, None
    guard_report = dict(status="watching", observations=0, started_at=started)
    report["paused_survival_guard"] = guard_report
    from diablo2.common.field_control import FieldDirective, HeldFieldInput
    from diablo2.runs.summoner.continuous_navigation import near_ground

    enabled = all(value is not None for value in (backend, ground, hazards, buff_expires_at)) and engage_monsters
    guard_report["nearby_defense_enabled"] = enabled
    held = (
        HeldFieldInput(backend, profile.actions.movement_skill_key, profile.actions.primary_attack_skill_key, clock=clock)
        if enabled
        else None
    )
    watchdog_stop = threading.Event()
    watchdog_errors = []
    attack_started = None
    attack_dispatch_baseline = 0
    probe = None
    if enabled and target_vision is not None:
        from diablo2.common.hostile_probe import NearbyHostileProbe

        probe = NearbyHostileProbe(target_vision, backend, (capture.target["left"], capture.target["top"]), clock=clock)

    def watchdog():
        while not watchdog_stop.wait(0.02):
            try:
                if stop.is_set() or not focused() or clock() >= buff_expires_at:
                    held.release()
                else:
                    held.service(clock())
            except Exception as exc:
                watchdog_errors.append(str(exc))
                stop.set()
                return

    worker = threading.Thread(target=watchdog, name="summoner-paused-defense", daemon=True) if enabled else None
    if worker is not None:
        worker.start()
    try:
        while not stop.is_set():
            if not focused():
                guard_report["status"] = "focus_handoff"
                return
            if renew_buffs is not None and buff_expires_at is not None and clock() >= buff_expires_at:
                if held is not None:
                    held.release()
                buff_expires_at = renew_buffs()
                attack_started = None
                continue
            acquired = clock()
            last = capture.grab().frame
            reading = hud.observe(last)
            blocked = resource_guard(reading, profile.survival)
            if clock() - acquired >= 0.25:
                blocked = "observation_unverified"
            guard_report.update(
                observations=guard_report["observations"] + 1,
                last_observed_at=acquired,
                life_ratio=reading.life.ratio,
                mana_ratio=reading.mana.ratio,
                blocked=blocked,
            )
            if blocked == "resource_low":
                if held is not None:
                    held.release()
                if probe is not None:
                    probe.reset()
                if recover_resources is not None and recover_resources(last, reading, acquired):
                    attack_started, missing_since = None, None
                    continue
                guard_report["status"] = "resource_low"
                raise RuntimeError("resource_low")
            if blocked:
                if probe is not None:
                    probe.reset()
                if held is not None:
                    held.release()
                missing_since = acquired if missing_since is None else missing_since
                if clock() - missing_since >= 2:
                    guard_report["status"] = "hud_unverified"
                    raise RuntimeError("paused_hud_unverified")
            else:
                missing_since = None
                if held is not None:
                    prepare = getattr(ground, "prepare_frame", None)
                    if prepare is not None:
                        prepare(last, acquired, reading, True)
                    player = ground.observe(last, captured_at=acquired)
                    candidates = () if player.point is None else hazards.observe(last, player.point, acquired, clock())
                    close = any(near_ground(candidate.ground, player.point) for candidate in candidates)
                    if not close and probe is not None and clock() < buff_expires_at:
                        target = probe.observe(last, player.point, acquired)
                        close = target is not None
                        if target is not None:
                            guard_report["hover_hostile_confirmations"] = guard_report.get("hover_hostile_confirmations", 0) + 1
                    now = clock()
                    if close and 0 <= now - acquired < 0.25 and now < buff_expires_at and not stop.is_set() and focused():
                        if attack_started is None:
                            attack_dispatch_baseline = held.snapshot()["dispatches"].get(profile.actions.primary_attack_skill_key, 0)
                        attack_started = now if attack_started is None else attack_started
                        if now - attack_started < 6:
                            held.apply(
                                FieldDirective(
                                    "combat",
                                    "paused_current_near_hostile",
                                    deadline=min(acquired + 0.25, buff_expires_at, attack_started + 6),
                                ),
                                now,
                            )
                            guard_report["nova_observations"] = guard_report.get("nova_observations", 0) + 1
                        else:
                            held.release()
                            guard_report["defense_status"] = "near_hostile_requires_review"
                    else:
                        held.release()
                        attack_dispatches = held.snapshot()["dispatches"].get(profile.actions.primary_attack_skill_key, 0)
                        if attack_started is not None and attack_dispatches > attack_dispatch_baseline and loot_controller is not None:
                            loot_controller.after_combat(
                                buff_expires_at=buff_expires_at,
                                reason="paused_defense_ended_scan",
                                combat_id=("paused", attack_started, attack_dispatches),
                            )
                        attack_started = None
            stop.wait(0.02)
        guard_report["status"] = "operator_stop"
    finally:
        watchdog_stop.set()
        if worker is not None:
            worker.join(timeout=1)
            held.close()
        guard_report["ended_at"] = clock()
        if last is not None:
            evidence.append(("paused-survival-last.png", last))
    if watchdog_errors:
        raise RuntimeError("paused defense watchdog: " + watchdog_errors[0])


def run_north_segment(
    capture,
    hud,
    backend,
    focused,
    profile,
    stop,
    planner,
    proof,
    casts,
    reference,
    *,
    evidence,
    emit=print,
    hub_vision=None,
    hazards=None,
    progress=None,
    hover_vision=None,
    buff_expires_at=None,
):
    """감독한 짧은 구간을 도착 확인마다 다시 계획한다. 미확인 시전은 재전송하지 않는다."""
    if type(casts) is not int or not 1 <= casts <= 12:
        raise ValueError("supervised north segment requires one to twelve casts")
    if proof.get("source") == "runtime_screen":
        validate_runtime_permit(proof, time.time())
    else:
        validate_permit(proof, "flash", profile.survival, time.time(), buff=True)
    started = time.monotonic()
    expires_at = started + 12 if buff_expires_at is None else min(started + 12, buff_expires_at)
    if expires_at <= started:
        raise RuntimeError("buff_renewal_required")
    progress = dict(steps=[], position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)]) if progress is None else progress
    steps = progress["steps"]
    offset = len(steps)
    for index in range(offset, offset + casts):
        if stop.is_set() or not focused():
            raise RuntimeError("north segment interrupted or unfocused")
        # 감독 확인15초는 이 제한 구간의 시작 조건이다. 이후는 최신 HUD/발판/도착을 검사한다.
        if time.monotonic() >= expires_at:
            raise RuntimeError("supervised segment time limit")
        deadline = min(expires_at, time.monotonic() + 2)
        attempts = 0
        row, retained_frames = None, {}

        def retain(stage, image):
            item = ("north-{:02d}-{}.png".format(index, stage), image)
            if stage in retained_frames:
                evidence[retained_frames[stage]] = item
            else:
                retained_frames[stage] = len(evidence)
                evidence.append(item)

        while True:
            if stop.is_set() or not focused():
                raise RuntimeError("north segment interrupted or unfocused")
            if time.monotonic() >= expires_at:
                raise RuntimeError("supervised segment time limit")
            if time.monotonic() >= deadline:
                raise RuntimeError("step preparation time limit")
            acquired = time.monotonic()
            attempts += 1
            frame = capture.grab().frame
            blocked = resource_guard(hud.observe(frame), profile.survival)
            if blocked:
                if blocked == "resource_low" or time.monotonic() >= deadline:
                    raise RuntimeError(blocked)
                stop.wait(0.02)
                continue
            try:
                hub = None if hub_vision is None else hub_vision.find(hub_vision.normalize(frame), "world_arcane_waypoint")
                hub_point = None if hub is None else (hub["x"] * frame.shape[1] / 1267, hub["y"] * frame.shape[0] / 753)

                def plan():
                    return plan_arcane_step(
                        frame,
                        "north",
                        *planner,
                        acquired,
                        hub_point=hub_point,
                        position=tuple(progress["position"]),
                        visited=tuple(progress["breadcrumbs"]),
                    )

                if hazards is None:
                    choice, reading, view = plan()
                    candidates = ()
                else:
                    current_ground = planner[0].observe(frame, captured_at=acquired)
                    if current_ground.point is None:
                        raise RuntimeError(current_ground.reason)
                    choice, reading, view = plan()
                    # 같은 화면의 현재 위치와 착지 목표 주변을 모두 검사한다.
                    candidates = hazards.observe(frame, current_ground.point, acquired, time.monotonic(), aim=choice.aim)
                # 첫 CV 준비가 느린 화면은 보내지 않고 새 화면으로 다시 계산한다.
                # 포인터 처리50ms와 필드의 새 캡처/HUD/호버 검사에150ms를 남긴다.
                if not candidates and time.monotonic() - acquired >= 0.10:
                    if time.monotonic() >= deadline:
                        raise RuntimeError("planning_budget_exhausted")
                    continue
            except RuntimeError as exc:
                # 표식/발판을 순간 가린 효과는 입력 없이 새 화면에서 다시 읽는다.
                if (
                    str(exc)
                    not in (
                        "ground_unverified",
                        "planned step frame is stale",
                        "no_observed_progress",
                        "player_floor_unobserved",
                    )
                    or time.monotonic() >= deadline
                ):
                    retain("planning-failed", frame)
                    progress["planning_failure"] = dict(index=index, reason=str(exc), attempts=attempts)
                    raise
                stop.wait(0.02)
                continue
            if row is None:
                row = dict(index=index, dispatch_retries=[])
                steps.append(row)
            row.update(
                choice=asdict(choice),
                ground=asdict(reading),
                planned_at=acquired,
                planning_seconds=time.monotonic() - acquired,
                planning_attempts=attempts,
            )
            row.pop("aim_check", None)
            retain("before", frame)
            if candidates:
                row.update(status="monster_candidate_review", candidates=[asdict(candidate) for candidate in candidates])
                emit("NORTH_REVIEW: current monster appearance candidate; no next cast")
                return dict(status="north_hazard_review", **progress)
            aim = (capture.target["left"] + choice.aim[0], capture.target["top"] + choice.aim[1])

            def verify_aim():
                if stop.is_set() or not focused():
                    return False
                aimed_at = time.monotonic()
                aimed_frame = capture.grab().frame
                grabbed_at = time.monotonic()
                blocked = resource_guard(hud.observe(aimed_frame), profile.survival)
                hud_at = time.monotonic()
                hover = hover_vision.observe(aimed_frame, choice.aim)
                checked_at = time.monotonic()
                row["aim_check"] = dict(
                    captured_at=aimed_at,
                    capture_seconds=grabbed_at - aimed_at,
                    hud_seconds=hud_at - grabbed_at,
                    hover_seconds=checked_at - hud_at,
                    checked_at=checked_at,
                    original_frame_age=checked_at - acquired,
                    resources=blocked,
                    hover=[asdict(candidate) for candidate in hover],
                )
                clear = not blocked and not hover and checked_at - aimed_at < 0.25
                if not clear:
                    retain("aim-blocked", aimed_frame)
                return clear

            if dispatch_planned_step(
                backend,
                focused,
                stop,
                aim,
                profile.actions.movement_skill_key,
                acquired,
                verify_aim=None if hover_vision is None else verify_aim,
                expires_at=expires_at,
            ):
                break
            row["status"] = "dispatch_blocked"
            check, now = row.get("aim_check", {}), time.monotonic()
            # 시전하지 않은 조준만 폐기한다. 새 화면의 전체 계획/자원/호버 검사를 다시 거친다.
            retry = (
                not stop.is_set()
                and focused()
                and now < deadline
                and check.get("resources") in (None, "observation_unverified")
                and not check.get("hover")
                and (check.get("resources") == "observation_unverified" or now - acquired >= 0.25)
            )
            if not retry:
                return dict(status="north_failed", **progress)
            row["dispatch_retries"].append(dict(at=now, reason=check.get("resources") or "screen_stale"))
            stop.wait(0.02)
        landing = observe_step_landing(
            capture,
            hud,
            focused,
            profile.survival,
            stop,
            frame,
            view,
            choice,
            *planner,
            reference_seconds=reference,
            expires_at=expires_at,
        )
        row.update(status=landing["status"], landing=landing)
        retain("after", capture.grab().frame)
        emit("NORTH_STEP {}: {} displacement={}".format(index + 1, row["status"], landing["displacement"]))
        if row["status"] != "arrival_observed":
            return dict(status="north_failed", **progress)
        progress["position"] = [value + delta for value, delta in zip(progress["position"], landing["displacement"])]
        progress["breadcrumbs"].append(tuple(progress["position"]))
    return dict(status="north_segment_observed", **progress)


def enter_arcane(session, difficulty, emit, *, resume_town=False, stop=None):
    options = dict(town_loop=True, destination="arcane", arrival_only=True)
    if difficulty == "hell":
        options["reset_act2_start"] = True
    if resume_town:
        options["resume_town"] = True
    if stop is not None and stop.is_set():
        raise RuntimeError("Arcane entry interrupted")
    session.start(1, difficulty, **options)
    try:
        while session.is_running or not session.events.empty():
            if stop is not None and stop.is_set():
                session.request_stop()
            try:
                event = session.events.get(timeout=0.2)
            except Empty:
                continue
            emit("{}: {}".format(event.level, event.message))
    except KeyboardInterrupt:
        session.stop()
        raise
    if session.last_result != "passed":
        raise RuntimeError("Arcane entry did not pass: {}".format(session.last_result))


def exit_to_character(session, capture, focused, stop, vision):
    """현재 배치의 마을 실행기 표식으로 종료 화면까지 확인한다."""

    def guard():
        if stop.is_set() or not focused():
            raise RuntimeError("safe exit interrupted or unfocused")

    def wait_for(names, seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            guard()
            gray = vision.normalize(capture.grab().frame)
            matches = [vision.find(gray, name) for name in names]
            if all(match is not None for match in matches):
                return matches
            stop.wait(0.05)
        raise RuntimeError("safe exit state unverified: " + ",".join(names))

    guard()
    session._town_loop = True
    session._town_guard = guard
    session._last_pointer = None
    session._press_key("esc")
    match = wait_for(("save_exit",), 8)[0]
    x = round(match["x"] * capture.target["width"] / 1267)
    y = round(match["y"] * capture.target["height"] / 753)
    session._click_relative(capture, (x, y))
    wait_for(("play",), 20)


def return_to_act1_and_exit(session, capture, focused, stop, vision, *, evidence, emit=print):
    """현재 중앙 WP를 다시 관찰해 Act 1과 캐릭터 선택까지 기존 실행기로 잇는다."""
    from diablo2.actions.town_loop_vision import REFERENCE_SIZE, TownLoopPlanner

    planner = TownLoopPlanner(vision, 1, "hell", "arcane")
    planner.data["phase"] = "destination_arrival"
    previous, actions = None, []
    cursor_pending_since = None

    def guard():
        if stop.is_set() or not focused():
            raise RuntimeError("north-room finish interrupted or unfocused")

    session._town_loop, session._town_guard, session._last_pointer = True, guard, None
    try:
        while planner.data["phase"] != "done":
            guard()
            acquired = time.monotonic()
            frame = capture.grab().frame
            if session._accept_waypoint_cursor_shift(frame, planner, capture.target["width"]) == "pending":
                cursor_pending_since = acquired if cursor_pending_since is None else cursor_pending_since
                if acquired - cursor_pending_since < 0.45:
                    stop.wait(0.04)
                    continue
            else:
                cursor_pending_since = None
            action = planner.step(frame)
            if time.monotonic() - acquired >= 0.25:
                if action is not None:
                    raise RuntimeError("north-room finish action frame stale")
                continue
            if action is not None:
                guard()
                session._dispatch_town_action(capture, frame, action, REFERENCE_SIZE)
                actions.append(action)
                emit("NORTH_ROOM_FINISH: " + action["reason"])
            phase = planner.data["phase"]
            if phase != previous:
                evidence.append(("room-finish-{}-{}.png".format(len(actions), phase), frame))
                previous = phase
            stop.wait(0.04)
        return dict(status="act1_then_character_select_observed", actions=actions, planner=planner.data)
    finally:
        session._release_town_mouse()
        session._town_guard = None


def run(config, *, north_cycles=None, **options):
    """북쪽 개발 시험을 새 방 단위로 반복한다. 전체 Summoner 수용 실적과 분리한다."""
    if north_cycles is None:
        return run_once(config, **options)
    if type(north_cycles) is not int or not 1 <= north_cycles <= 5:
        raise ValueError("north room cycles must be one to five")
    if not options.get("continuous") or options.get("all_wings") or options.get("wing", "north") != "north":
        raise ValueError("new-room north tests require continuous north without all-wings")
    emit = options.get("emit", print)
    if not options.get("live"):
        emit("DRY: north end -> central waypoint -> Act 1 -> save/exit -> new room; {} north room(s)".format(north_cycles))
        return 0
    output = Path(options["output"])
    output.mkdir(parents=True, exist_ok=False)
    if options.get("automatic", True) is not True:
        raise ValueError("normal Summoner runs use automatic screen observations")
    signals = None
    summary = dict(status="running", requested=north_cycles, completed=0, recovery_closed=False, rooms=[], acceptance_rooms=0)
    recovery = options.get("resume_arcane") is True or options.get("return_from") is not None
    try:
        for index in range(north_cycles + int(recovery)):
            if options.get("stop_event") is not None and options["stop_event"].is_set():
                summary["status"] = "interrupted"
                return 2
            recovering = recovery and index == 0
            room_output = output / ("initial-return" if recovering else "room-{:02d}".format(summary["completed"] + 1))
            current = dict(options, output=room_output, finish_room=True, signals=signals)
            if index:
                current.update(resume_arcane=False, resume_town=False, return_from=None, continue_navigation=None)
            result = run_once(config, **current)
            report = json.loads((room_output / "report.json").read_text(encoding="utf-8"))
            summary["rooms"].append(dict(report=room_output.name + "/report.json", status=report["status"], recovery=recovering))
            if result or report["status"] != "north_room_closed":
                summary["status"] = "needs_navigation_or_encounter_review"
                return 2
            if recovering:
                summary["recovery_closed"] = True
            else:
                summary["completed"] += 1
            emit("NORTH_ROOM_CYCLES: {}/{} new-room tests closed through Act 1".format(summary["completed"], north_cycles))
        summary["status"] = "north_room_tests_finished"
        return 0
    finally:
        (output / "report.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")


def run_once(
    config,
    *,
    live,
    resume_arcane,
    casts,
    difficulty,
    output,
    resume_town=False,
    segments=1,
    continuous=False,
    field_seconds=20,
    wing="north",
    return_from=None,
    engage_monsters=True,
    continue_navigation=None,
    all_wings=False,
    finish_room=False,
    signals=None,
    automap="on",
    automatic=True,
    stop_event=None,
    character_key=None,
    emit=print,
):
    """방 입장→자동 버프→북쪽 개발 이동 시험. 연속 모드는 F2를 유지하며 같은 방에서 보정한다."""
    import cv2 as cv
    import keyboard
    from diablo2.actions.run_lifecycle import RunLifecycleSession
    from diablo2.common.capture import ScreenCapture, USER32, resolve_window_from_config
    from diablo2.common.controller import BotController, ControllerFieldInput
    from diablo2.common.survival_vision import SurvivalHudObserver
    from diablo2.common.buffing_vision import WeaponSetObserver
    from diablo2.common.automap_vision import AutomapObserver
    from diablo2.actions.town_loop_vision import TownLoopVision
    from diablo2.runs.summoner.scene_vision import ArcaneHazardObserver, ArcaneHoverObserver, ArcaneTargetObserver, ArcaneHubObserver
    from diablo2.runs.summoner.continuous_navigation import run_continuous_route, validate_travel_permit, restore_route_flow
    from diablo2.runs.summoner.route_graph import ArcaneRouteGraph
    from diablo2.runs.summoner.terrain_patterns import ArcaneTerrainPatterns
    from diablo2.runs.summoner.terminal_vision import ArcaneTerminalObserver
    from diablo2.runs.summoner.objective_vision import ArcaneObjectiveObserver
    from diablo2.runs.summoner.encounter_execution import SummonerAppearanceObserver, run_platform_encounter
    from diablo2.runs.summoner.camera_ground import CalibratedCameraGroundObserver

    if type(casts) is not int or not 1 <= casts <= 12:
        raise ValueError("north casts must be one to twelve")
    if automap not in ("on", "preserve"):
        raise ValueError("automap must be on or explicit diagnostic preserve")
    if type(segments) is not int or not 1 <= segments <= 4:
        raise ValueError("supervised north segments must be one to four")
    if type(field_seconds) is not int or not 1 <= field_seconds <= 60:
        raise ValueError("field seconds must be one to sixty")
    if continue_navigation is not None and not (continuous and resume_arcane):
        raise ValueError("navigation continuation requires current-field continuous mode")
    if type(all_wings) is not bool or (all_wings and (not continuous or wing != "north" or return_from is not None)):
        raise ValueError("all-wing search requires continuous north outbound start")
    if type(finish_room) is not bool or (finish_room and (not continuous or all_wings or wing != "north")):
        raise ValueError("north-room finish requires continuous north mode without all-wings")
    if automatic is not True:
        raise ValueError("normal Summoner execution uses automatic screen observations; use the separate probe for supervised diagnostics")
    from diablo2.common.config import get_active_character_key

    character_key = character_key or get_active_character_key(config)
    if character_key not in config.characters:
        raise ValueError("selected character profile required")
    profile = config.characters[character_key]
    if profile.survival is None:
        raise ValueError("selected character survival policy required")
    estimate = config.casting_rules.estimate(profile.ruleset_family, profile.character_class, profile.casting)
    if estimate.action != "estimate" or estimate.frames is None:
        raise ValueError("configured class/FCR reference required")
    planner = load_arcane_step_planner(automap=automap == "on", character_class=profile.character_class)
    hud = SurvivalHudObserver(allow_high_orb_fallback=True, allow_partial_orb_fallback=True)
    weapon, town_vision = WeaponSetObserver(), TownLoopVision()

    def panels_open(frame):
        if weapon.observe(frame) is not None:
            return True
        gray = town_vision.normalize(frame)
        return any(town_vision.find(gray, name) is not None for name in ("waypoint_header", "save_exit"))

    hazards = ArcaneHazardObserver()
    hover_vision = ArcaneHoverObserver()
    target_vision = ArcaneTargetObserver() if continuous else None
    hub_vision = ArcaneHubObserver() if continuous else None
    session = RunLifecycleSession(config.capture)
    if not live:
        mode = "{} seconds held-F2 wide steering".format(field_seconds) if continuous else "{} north casts".format(casts)
        emit("DRY: entry -> automatic buff -> {} segment(s) of {}; no game input".format(segments, mode))
        return 0
    output = Path(output)
    output.mkdir(parents=True, exist_ok=False)
    capture, hotkeys, evidence, trace = None, [], [], []
    stop = CooperativeStop(stop_event)
    report = dict(status="preparing", errors=[], acceptance_room=False, resume_arcane=resume_arcane, resume_town=resume_town)
    report.update(segments_requested=segments, segments_completed=0, continuations=[])
    report.update(continuous=continuous, field_seconds=field_seconds if continuous else None)
    report.update(wing=wing, return_from=return_from, engage_monsters=engage_monsters)
    report["all_wings"] = all_wings
    report["finish_north_room"] = finish_room
    report["automap_preference"] = automap
    backend, camera_ground, buff_expires_at, recover_resources = None, None, None, None
    automatic_driver, loot_controller = None, None
    report.update(automatic=automatic, character_key=character_key, main_weapon_start_required=True)
    route_memory = ArcaneRouteGraph() if continuous else None
    terrain_patterns = ArcaneTerrainPatterns() if continuous else None
    terminal_vision = ArcaneTerminalObserver() if continuous else None
    objective_vision = ArcaneObjectiveObserver() if continuous else None

    def pause_field():
        def renew_paused():
            nonlocal buff_expires_at
            record = automatic_driver.execute()
            report.setdefault("buff_renewals", []).append(record)
            buff_expires_at = min(record["deadlines"])
            return buff_expires_at

        maintain_paused_field(
            capture,
            hud,
            focused,
            profile,
            stop,
            report=report,
            evidence=evidence,
            backend=backend,
            ground=planner[0],
            hazards=hazards,
            buff_expires_at=buff_expires_at,
            engage_monsters=engage_monsters,
            target_vision=target_vision,
            recover_resources=recover_resources,
            renew_buffs=renew_paused if automatic_driver is not None else None,
            loot_controller=loot_controller,
        )

    try:
        if not resume_arcane:
            enter_arcane(session, difficulty, emit, resume_town=resume_town, stop=stop)
            report["entry"] = "arcane_hub_observed"
        capture = ScreenCapture(config.capture)
        session._focus_game_window(capture)
        window = resolve_window_from_config(config.capture)
        target = dict(capture.target)

        def focused():
            return (
                window is not None
                and USER32.GetForegroundWindow() == window.handle
                and not USER32.IsIconic(window.handle)
                and capture.current_target() == target
            )

        for key in {config.hotkeys.stop, config.hotkeys.pause, "f10"}:
            hotkeys.append(keyboard.add_hotkey(key, stop.request_stop))
        controller = BotController(dry_run=False)
        backend = RecordingInput(
            ControllerFieldInput(
                controller,
                focused=lambda: focused() and not stop.is_set(),
                bounds=lambda: (target["left"], target["top"], target["left"] + target["width"], target["top"] + target["height"]),
            )
        )
        from diablo2.common.emergency_recovery import EmergencyResourceRecovery

        report["resource_recoveries"] = []
        recover_resources = EmergencyResourceRecovery(
            capture, hud, backend, focused, profile.survival, stop, records=report["resource_recoveries"], evidence=evidence
        )
        from diablo2.common.conditional_loot import ConditionalLootController

        report["loot_scans"] = []
        loot_controller = ConditionalLootController(
            config,
            capture,
            hud,
            backend,
            focused,
            profile,
            stop,
            panels_open,
            deadline_supplier=lambda: buff_expires_at,
        )
        report["item_label_policy"] = "off_while_moving_on_for_post_combat_scan"
        report["item_label_preparation"] = loot_controller.prepare(fresh_room=not resume_arcane)
        if automap == "on":
            report["automap"] = ensure_automap_on(capture, backend, focused, stop, hud, profile.survival, AutomapObserver(), panels_open)
        else:
            report["automap"] = dict(status="preserved_for_diagnostic", visibility="unverified")

        camera_ground = CalibratedCameraGroundObserver(planner[0], panels_open, persist_reviewed_layout=True)
        acquired = time.monotonic()
        frame = capture.grab().frame
        camera_ground.confirm_runtime_viewport(
            frame,
            hud.observe(frame),
            focused(),
            acquired,
            time.monotonic(),
            reviewed_layout=Path(__file__).resolve().parents[3] / "assets/ui/layouts/remastered-1922x1140/camera-ground.json",
        )
        planner = (camera_ground, planner[1])
        area_vision = AutomapObserver()

        def observe_runtime():
            if stop.is_set() or not focused():
                raise RuntimeError("runtime safety interrupted or unfocused")
            acquired = time.monotonic()
            frame = capture.grab().frame
            reading = hud.observe(frame)
            blocked = resource_guard(reading, profile.survival)
            if blocked or panels_open(frame):
                raise RuntimeError(blocked or "panel_open")
            planner[0].prepare_frame(frame, acquired, reading, True)
            player = planner[0].observe(frame, captured_at=acquired)
            if player.point is None:
                raise RuntimeError(player.reason)
            from diablo2.runs.summoner.continuous_navigation import near_ground

            candidates = hazards.observe(frame, player.point, acquired, time.monotonic())
            close = [item for item in candidates if item.ground is None or near_ground(item.ground, player.point)]
            if close:
                raise RuntimeError("buff_nearby_hostile")
            if not area_vision.visible(frame, "arcane_sanctuary"):
                raise RuntimeError("current Arcane title unverified")
            if time.monotonic() - acquired >= 0.25:
                raise RuntimeError("screen_stale")
            return dict(
                source="runtime_screen",
                character_id=character_key,
                area="arcane_sanctuary",
                observed_at=time.time(),
                captured_at=acquired,
                panels_closed=True,
                belt_closed=True,
                nearby_candidates=0,
                safety_scope="current_nearby_candidates_and_stable_ratios",
                player_ground=list(player.point),
                life_ratio=reading.life.ratio,
                mana_ratio=reading.mana.ratio,
                item_labels_on=loot_controller.labels_state == "on",
            )

        def clear_runtime_hostiles():
            if not engage_monsters:
                raise RuntimeError("buff-site unsafe while combat disabled")
            result = clear_buff_site(capture, hud, backend, focused, profile, stop, estimate.frames / 25.0, evidence=evidence)
            if result["attack_dispatches"]:
                report["loot_scans"].append(loot_controller.after_combat(reason="buff_site_combat_ended_scan"))

        def dispatch_runtime(before_step):
            current_trace = []
            result = dispatch_buffs(
                capture,
                hud,
                backend,
                focused,
                profile,
                stop,
                trace_frames=current_trace,
                observation_wait=2,
                sequence_timeout=12,
                before_step=before_step,
            )
            evidence.extend(
                ("automatic-buff-{:02d}-step-{:02d}.png".format(len(report.get("buff_renewals", [])), metadata["index"]), image)
                for image, metadata in current_trace
            )
            return result

        automatic_driver = AutomaticBuffDriver(
            profile,
            lambda: BuffSiteGuard(capture, hud, focused, profile, stop, planner[0], hazards),
            dispatch_runtime,
            observe_runtime,
            clear_runtime_hostiles,
            stop,
        )
        report["primary_set"] = dict(status="main_weapon_start_precondition", inventory_input=False)
        initial = automatic_driver.execute()
        proof = initial["observation"]
        report["preparation_safety"] = report["initial_safety"] = initial["safety"]
        report["automatic_buff"] = initial
        report["buff"] = initial["buff"]
        report["camera_projection"] = dict(source="shared_layout_and_current_runtime_viewport", maximum_occlusion_seconds=None)
        buff_deadlines = initial["deadlines"]
        buff_expires_at = min(buff_deadlines)
        emit("AUTOMATIC_BUFF_READY: balanced configured sequence; no inventory or supervisor input; timer uses actual cast requests")

        segment, route_return, phase_transition = 0, return_from, None
        phase_number = 0
        renewed_current_field = False
        report["buff_renewals"] = []
        wing_order = ("north", "east", "south", "west")
        report["phases"] = []
        while segment < segments:
            if segment and continuous:
                emit("CONTINUE_CURRENT_FIELD: immediate next interval; fresh HUD/ground/focus and original buff deadline required")
                report["continuations"].append(dict(source="same_process_transition", segment=segment, at=time.monotonic()))
            elif segment:
                proof = observe_runtime()
                report["continuations"].append(proof)
            if continuous:
                phase_evidence = []
                prefix = "phase-{:02d}-{}-{}-{:02d}-".format(phase_number, wing, "outbound" if route_return is None else "return", segment)
                report["navigation"] = progress
                try:
                    report["navigation"] = run_continuous_route(
                        capture,
                        hud,
                        backend,
                        focused,
                        profile,
                        stop,
                        planner,
                        proof,
                        field_seconds,
                        evidence=phase_evidence,
                        hazards=hazards,
                        hover_vision=hover_vision,
                        target_vision=target_vision,
                        reference_seconds=estimate.frames / 25.0,
                        hub_vision=hub_vision,
                        wing=wing,
                        return_from=route_return,
                        engage_monsters=engage_monsters,
                        progress=progress,
                        buff_expires_at=min(buff_deadlines) if buff_deadlines else None,
                        emit=emit,
                        continue_current_field=bool(segment) and not renewed_current_field,
                        transition_from=phase_transition,
                        route_memory=route_memory,
                        terrain_patterns=terrain_patterns,
                        terminal_vision=terminal_vision,
                        objective_vision=objective_vision,
                        recover_resources=recover_resources,
                        loot_controller=loot_controller,
                    )
                finally:
                    evidence.extend((prefix + name, frame) for name, frame in phase_evidence)
                    report["navigation"]["evidence_files"] = [prefix + name for name, _ in phase_evidence]
                    report["navigation"]["final_frame"] = next(
                        (prefix + name for name, _ in reversed(phase_evidence) if name.endswith("-after.png")), None
                    )
                phase_transition = None
                renewed_current_field = False
            else:
                report["navigation"] = run_north_segment(
                    capture,
                    hud,
                    backend,
                    focused,
                    profile,
                    stop,
                    planner,
                    proof,
                    casts,
                    estimate.frames / 25.0,
                    evidence=evidence,
                    emit=emit,
                    hub_vision=town_vision,
                    hazards=hazards,
                    progress=progress,
                    hover_vision=hover_vision,
                    buff_expires_at=min(buff_deadlines) if buff_deadlines else None,
                )
            report["status"] = report["navigation"]["status"]
            if continuous and report["navigation"].get("continuous_status") == "buff_renewal_due":
                if len(report["buff_renewals"]) >= 8:
                    raise RuntimeError("automatic buff renewal limit reached")
                renewal = dict(wing=wing, return_from=route_return, previous_deadline=buff_expires_at)
                report["buff_renewals"].append(renewal)
                renewal.update(automatic_driver.execute())
                proof, buff_deadlines = renewal["observation"], renewal["deadlines"]
                buff_expires_at = min(buff_deadlines)
                renewed_current_field = True
                emit("BUFF_RENEWED_AUTOMATIC: fresh nearby/ratio observations and balanced sequence; same route retained")
                continue
            if continuous and report["status"] == "wing_end_observed" and route_return is None:
                report["phases"].append(report["navigation"])
                emit("WING_END_CONFIRMED: same-room central return begins from current observed terminal")
                phase_transition = report["navigation"]
                route_return = wing
                progress = dict(steps=[], position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)])
                route_memory = ArcaneRouteGraph()
                segment = 0
                phase_number += 1
                continue
            if report["status"] == "hub_arrived":
                report["segments_completed"] += 1
                emit("CENTRAL_WAYPOINT_ARRIVED: current waypoint and player proximity/settling verified; room retained")
                if finish_room:
                    report["phases"].append(report["navigation"])
                    report["room_finish"] = return_to_act1_and_exit(
                        session, capture, focused, stop, town_vision, evidence=evidence, emit=emit
                    )
                    report["status"] = "north_room_closed"
                    break
                if all_wings:
                    report["phases"].append(report["navigation"])
                    if wing == wing_order[-1]:
                        report["status"] = "all_wings_empty_review"
                        emit("ALL_WINGS_EMPTY_REVIEW: four positive chest endings; Summoner result unresolved")
                        pause_field()
                        break
                    phase_transition = report["navigation"]
                    wing = wing_order[wing_order.index(wing) + 1]
                    emit("NEXT_WING: {} begins from current settled central waypoint".format(wing))
                    route_return, segment = None, 0
                    progress = dict(steps=[], position=[0.0, 0.0], breadcrumbs=[(0.0, 0.0)])
                    route_memory = ArcaneRouteGraph()
                    continue
                break
            if continuous and report["status"] in ("field_paused", "wing_end_observed", "summoner_platform_observed"):
                if report["status"] == "summoner_platform_observed":
                    emit("SUMMONER_PLATFORM: current platform landmarks confirmed; remaining wing search cancelled; kill/loot unconfirmed")
                    if engage_monsters:
                        report["encounter"] = run_platform_encounter(
                            capture,
                            hud,
                            backend,
                            focused,
                            profile,
                            stop,
                            planner,
                            objective_vision,
                            SummonerAppearanceObserver(),
                            buff_expires_at=buff_expires_at,
                            reference_seconds=estimate.frames / 25.0,
                            evidence=evidence,
                            emit=emit,
                            target_observer=target_vision,
                            recover_resources=recover_resources,
                            loot_controller=loot_controller,
                        )
                emit("FIELD_PAUSED: inputs released; current room retained for navigation correction")
                emit("PAUSED_SURVIVAL_GUARD: HUD monitoring remains active; actual low resources or sustained HUD loss trigger safety exit")
                pause_field()
                break
            if report["status"] != "north_segment_observed":
                raise RuntimeError("north landing not verified")
            report["segments_completed"] += 1
            segment += 1
        if continuous and report["status"] in ("north_segment_observed", "hub_arrived"):
            emit("FIELD_INTERVAL_FINISHED: development movement finished; HUD monitoring remains active until operator handoff")
            pause_field()
    except KeyboardInterrupt:
        stop.set()
        report["status"] = "interrupted"
        report["errors"].append("keyboard interrupt")
    except Exception as exc:
        report["status"] = "interrupted" if stop.is_set() else "failed"
        report["errors"].append(str(exc))
        emit("SUMMONER_STOP: " + str(exc))
        failure = str(exc)
        # 명시적 중단이나 포커스 상실 뒤에는 종료 입력도 보내지 않는다.
        if continuous and failure not in ("resource_low", "paused_hud_unverified"):
            report["field_retained"] = True
            emit("FIELD_PAUSED: inputs released; no room exit during navigation training")
            if (
                (report.get("initial_safety") is not None or report.get("preparation_safety") is not None)
                and capture is not None
                and not stop.is_set()
                and focused()
            ):
                emit(
                    "PAUSED_SURVIVAL_GUARD: preparation/navigation failure; continue HUD monitoring until operator handoff or survival risk"
                )
                try:
                    pause_field()
                except KeyboardInterrupt:
                    stop.set()
                    report["status"] = "interrupted"
                    report["errors"].append("keyboard interrupt")
                except RuntimeError as guard_error:
                    failure = str(guard_error)
                    report["errors"].append("paused guard: " + failure)
        if (
            (not continuous or failure in ("resource_low", "paused_hud_unverified"))
            and capture is not None
            and backend is not None
            and not stop.is_set()
            and focused()
        ):
            try:
                session._last_pointer = None

                exit_to_character(session, capture, focused, stop, town_vision)
                report["safe_exit"] = "character_select_observed"
                report["field_retained"] = False
            except Exception as exit_error:
                report["safe_exit"] = "unverified"
                report["errors"].append("exit: " + str(exit_error))
    finally:
        stop.set()
        if loot_controller is not None:
            report["loot_scans"] = list(loot_controller.records)
            report["item_labels_final"] = loot_controller.labels_state
        if camera_ground is not None:
            report.setdefault("camera_projection", {}).update(
                calibrations=camera_ground.calibrations,
                projected_observations=camera_ground.projections,
                final_calibrated_center=camera_ground.center,
                final_authorized=camera_ground.authorized,
                reset_events=list(camera_ground.reset_events),
            )
        for handle in hotkeys:
            try:
                keyboard.remove_hotkey(handle)
            except Exception as cleanup_error:
                report["errors"].append("hotkey cleanup: " + str(cleanup_error))
        if capture is not None:
            capture.close()
        report["input_events"] = [] if backend is None else backend.events

        def save_report():
            pending = output / "report.json.pending"
            pending.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
            pending.replace(output / "report.json")

        # 이미지 직렬화 중 재중단돼도 입력/판정 보고서를 남기고 불완전한 이미지 보관 상태를 명시한다.
        report["evidence_write_status"] = "in_progress"
        save_report()
        for filename, frame in evidence:
            cv.imwrite(str(output / filename), frame)
        for frame, metadata in trace:
            filename = "buff-step-{:02d}.png".format(metadata["index"])
            cv.imwrite(str(output / filename), frame)
        report["evidence_write_status"] = "complete"
        save_report()
    emit("SUMMONER_RESULT: {} ({})".format(report["status"], output / "report.json"))
    return 0 if report["status"] in ("north_segment_observed", "hub_arrived", "north_room_closed") and not report["errors"] else 2
