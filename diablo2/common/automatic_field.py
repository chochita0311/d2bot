from __future__ import annotations

import math
import threading
import time


class CooperativeStop:
    """방별 정리 신호와 GUI의 전체 중단 신호를 분리한다."""

    def __init__(self, external=None):
        self.local = threading.Event()
        self.external = external

    def is_set(self):
        return self.local.is_set() or (self.external is not None and self.external.is_set())

    def set(self):
        self.local.set()

    def request_stop(self):
        self.local.set()
        if self.external is not None:
            self.external.set()

    def wait(self, seconds):
        end = time.monotonic() + seconds
        while not self.is_set():
            remaining = end - time.monotonic()
            if remaining <= 0:
                break
            self.local.wait(min(remaining, 0.02))
        return self.is_set()


def validate_runtime_permit(proof, now):
    """실제 화면 생산자의 제한된 근접 관찰을 감독 확인과 구분한다."""
    observed = proof.get("observed_at")
    if (
        proof.get("source") != "runtime_screen"
        or proof.get("area") != "arcane_sanctuary"
        or proof.get("panels_closed") is not True
        or proof.get("belt_closed") is not True
        or type(proof.get("nearby_candidates")) is not int
        or proof.get("nearby_candidates") != 0
        or proof.get("safety_scope") != "current_nearby_candidates_and_stable_ratios"
        or type(observed) not in (int, float)
        or not math.isfinite(observed)
        or not 0 <= now - observed < 0.5
    ):
        raise ValueError("fresh runtime field observation required")


class AutomaticBuffDriver:
    """현재 안전 관찰과 실제 시전 요청 시각으로 버프를 실행한다."""

    def __init__(self, profile, guard_factory, dispatch, observe, clear_hostiles, stop, *, clock=time.monotonic):
        self.profile = profile
        self.guard_factory, self.dispatch, self.observe = guard_factory, dispatch, observe
        self.clear_hostiles, self.stop, self.clock = clear_hostiles, stop, clock
        self.deadlines = []

    def execute(self):
        order = tuple(token.lower() for token in self.profile.actions.pre_run_buff_order)
        if not order or order.count("w") % 2:
            raise ValueError("buff sequence must restore the starting main weapon set")
        if not self.profile.survival.buffs:
            raise ValueError("timed buff policy required")
        guard, stability = None, None
        for attempt in range(3):
            if self.stop.is_set():
                raise RuntimeError("automatic buff interrupted")
            guard = self.guard_factory()
            try:
                stability = guard.prepare()
                break
            except RuntimeError as exc:
                if str(exc) != "buff_nearby_hostile" or attempt == 2:
                    raise
                self.clear_hostiles()
        before = self.observe()
        dispatched = self.dispatch(guard.wait_check)
        if dispatched.get("status") != "dispatched_unconfirmed" or dispatched.get("equipment_dispatch_balanced") is not True:
            raise RuntimeError("automatic buff dispatch: " + str(dispatched.get("status")))
        # 효과 확인을 입력 성공으로 위조하지 않는다. 시한은 실제 해당 키 요청에서 시작한다.
        deadlines = []
        for rule in self.profile.survival.buffs:
            requested = next((row for row in reversed(dispatched["buff_dispatches"]) if row["key"] == rule.key), None)
            if requested is None:
                raise RuntimeError("timed buff request missing: " + rule.name)
            requested_at = requested.get("cast_request_monotonic")
            if type(requested_at) not in (int, float) or not math.isfinite(requested_at) or not 0 <= requested_at <= self.clock():
                raise RuntimeError("invalid actual buff request time")
            deadlines.append(requested_at + rule.duration_seconds - rule.renew_before_seconds)
        proof = self.observe()
        if self.clock() >= min(deadlines):
            raise RuntimeError("automatic buff deadline elapsed during preparation")
        self.deadlines = deadlines
        return dict(
            buff=dispatched,
            safety=before,
            observation=proof,
            stability=stability,
            deadlines=deadlines,
            equipment_basis="main_weapon_start_precondition_and_balanced_sequence",
            effect_status="not_independently_verified",
        )

    def due(self):
        return bool(self.deadlines) and self.clock() >= min(self.deadlines)
