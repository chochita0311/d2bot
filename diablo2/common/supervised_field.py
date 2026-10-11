from __future__ import annotations

import json
import math
import queue
import threading
import time
from dataclasses import asdict


class StaleSupervision(ValueError):
    """유효한 형식의 감독 확인이 만료되어 새 관측이 필요한 경우다."""


def ensure_automap_on(capture, backend, focused, stop, hud, policy, observer, panels_open, *, clock=time.monotonic, supervised_on=None):
    """입력 없는 최신 화면에서 지도 제목을 확인하고 필요한 경우 Tab 한 번 뒤 다시 확인한다."""
    toggled, missing_since = False, None
    deadline = clock() + 3
    while clock() < deadline:
        if stop.is_set() or not focused():
            raise RuntimeError("automap preparation interrupted or unfocused")
        acquired = clock()
        frame = capture.grab().frame
        blocked = resource_guard(hud.observe(frame), policy)
        if blocked or panels_open(frame):
            raise RuntimeError("automap preparation: " + (blocked or "panel_open"))
        visible = observer.visible(frame, "arcane_sanctuary")
        if clock() - acquired >= 0.25:
            continue
        if visible:
            return dict(status="on_observed", tab_sent=toggled, evidence=observer.last_evidence)
        if not toggled and supervised_on is not None and supervised_on.get("automap_on") is True:
            observed = supervised_on.get("observed_at")
            if (
                supervised_on.get("source") != "supervised_visual"
                or supervised_on.get("area") != "arcane_sanctuary"
                or type(observed) not in (int, float)
                or not math.isfinite(observed)
                or not 0 <= time.time() - observed <= 15
            ):
                raise RuntimeError("fresh supervised automap ON proof required")
            # 이름표가 제목을 가린 재개 화면의 최신 감독 근거다. 자동 지도 인식으로 기록하지 않는다.
            return dict(status="on_supervised", tab_sent=False, evidence=supervised_on)
        missing_since = acquired if missing_since is None else missing_since
        if not toggled and getattr(observer, "last_obscured", False) is not True and acquired - missing_since >= 0.15:
            if stop.is_set() or not focused():
                raise RuntimeError("automap preparation interrupted or unfocused")
            backend.press("tab")
            toggled = True
        stop.wait(0.02)
    raise RuntimeError("automap ON unverified after bounded preparation")


def enable_fresh_room_item_labels(capture, backend, focused, stop, hud, policy, panels_open, *, clock=time.monotonic):
    """토글 모드의 새 방 기본 OFF에서 한 번만 요청한다. 재개 상태에 재전송하지 않는다."""
    if stop.is_set() or not focused():
        raise RuntimeError("item label preparation interrupted or unfocused")
    acquired = clock()
    frame = capture.grab().frame
    blocked = resource_guard(hud.observe(frame), policy)
    if blocked or panels_open(frame) or clock() - acquired >= 0.25:
        raise RuntimeError("item label preparation: " + (blocked or "panel_or_stale_frame"))
    if stop.is_set() or not focused():
        raise RuntimeError("item label preparation interrupted or unfocused")
    backend.press("alt")
    return dict(status="on_requested", alt_sent=True, basis="fresh_room_default_off_toggle_mode", visibility="unverified_until_drop")


def ensure_weapon_set(capture, backend, focused, stop, observer, expected, *, timeout=2):
    """현재 I/II 표식을 읽고 필요한 경우 한 번만 전환한다. 미확인 전환을 반복하지 않는다."""
    if expected not in (1, 2):
        raise ValueError("explicit primary weapon set required")

    def guard():
        if stop.is_set() or not focused():
            raise RuntimeError("weapon check interrupted or unfocused")

    def read_until(predicate):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            guard()
            acquired = time.monotonic()
            number = observer.observe(capture.grab().frame)
            if time.monotonic() - acquired < 0.25 and predicate(number):
                return number
            stop.wait(0.02)
        raise RuntimeError("weapon set unverified")

    guard()
    backend.press("i")
    current = read_until(lambda value: value in (1, 2))
    if current != expected:
        guard()
        backend.press("w")
        read_until(lambda value: value == expected)
    guard()
    backend.press("i")
    return dict(before=current, after=expected, swapped=current != expected)


def validate_permit(permit, character, policy, now, *, buff=False):
    required = ("field", "monsters_clear", "belt_closed")
    if not buff:
        required += ("buff_effect_verified", "equipment_restored")
    if permit.get("character_id") != character or any(permit.get(key) is not True for key in required):
        raise ValueError("explicit supervised field/buff/equipment/belt confirmations required")
    at = permit.get("observed_at")
    if type(at) not in (int, float) or not math.isfinite(at) or at > now:
        raise ValueError("valid nonfuture supervised observation time required")
    count = permit.get("potions_remaining")
    if type(count) is not int or not 1 <= count <= sum(c.capacity for c in policy.belt_columns if c.kind == policy.potion_kind):
        raise ValueError("confirmed nonempty purple belt required")
    if not buff:
        casts = permit.get("buff_casts", {})
        if any(
            type(casts.get(rule.name)) not in (int, float)
            or not math.isfinite(casts[rule.name])
            or not 0 <= now - casts[rule.name] < rule.duration_seconds - rule.renew_before_seconds
            for rule in policy.buffs
        ):
            raise ValueError("confirmed buff already due or missing cast time")
    if now - at > 15:
        raise StaleSupervision("supervised permit is stale (age {:.3f}s; limit 15s)".format(now - at))


class RecordingInput:
    def __init__(self, backend, clock=time.monotonic):
        self.backend, self.clock, self.events = backend, clock, []

    def _send(self, action, *args):
        row = {"action": action, "args": args, "at": self.clock(), "wall_at": time.time(), "sent": False}
        self.events.append(row)
        getattr(self.backend, action)(*args)
        if action == "move" and getattr(self.backend, "pointer_evidence", None) is not None:
            row["pointer_evidence"] = self.backend.pointer_evidence
        row["sent"] = True

    def key_down(self, key):
        self._send("key_down", key)

    def key_up(self, key):
        self._send("key_up", key)

    def press(self, key):
        self._send("press", key)

    def move(self, x, y):
        self._send("move", x, y)

    def click(self):
        self._send("click")

    def pointer_matches(self, point):
        matches = getattr(self.backend, "pointer_matches", None)
        return callable(matches) and matches(point)


def resource_guard(reading, policy):
    if reading.belt_visibility != "closed" or reading.life.ratio is None or reading.mana.ratio is None:
        return "observation_unverified"
    if reading.life.ratio < policy.life_potion_below or reading.mana.ratio < policy.mana_potion_below:
        return "resource_low"
    return None


def wait_readable_resources(capture, hud, focused, policy, stop, deadline, *, clock=time.monotonic, observe_frame=None):
    while clock() < deadline and not stop.is_set():
        if not focused():
            return "focus_lost"
        acquired = clock()
        frame = capture.grab().frame
        reading = hud.observe(frame)
        if observe_frame is not None:
            observe_frame(frame, acquired)
        if clock() - acquired >= 0.25:
            return "screen_stale"
        blocked = resource_guard(reading, policy)
        if blocked != "observation_unverified":
            return blocked
        # 잠깐 가려진 글자는 입력 없이 새 화면을 기다린다. 이전 최대값을 재사용하지 않는다.
        stop.wait(0.02)
    return "observation_unverified" if not stop.is_set() else "interrupted"


def dispatch_buffs(
    capture, hud, backend, focused, profile, stop, *, trace_frames=None, observation_wait=0.75, sequence_timeout=8, before_step=None
):
    # 짧은 감독 준비용이다. 전송 기록은 효과나 안전의 자동 확인이 아니다.
    if trace_frames is not None and (trace_frames or len(profile.actions.pre_run_buff_order) > 16):
        raise ValueError("buff trace requires an empty buffer and at most sixteen steps")
    events, swapped, status = [], False, "dispatched_unconfirmed"
    began = time.monotonic()
    try:
        for token in profile.actions.pre_run_buff_order:
            if stop.is_set() or not focused() or time.monotonic() - began >= sequence_timeout:
                status = "interrupted"
                break
            blocked = wait_readable_resources(
                capture, hud, focused, profile.survival, stop, min(began + sequence_timeout, time.monotonic() + observation_wait)
            )
            if blocked:
                status = blocked
                break
            if before_step is not None:
                blocked = before_step()
                if blocked:
                    status = blocked
                    break
            if stop.is_set() or not focused() or time.monotonic() - began >= sequence_timeout:
                status = "interrupted_or_stale"
                break
            if token in ("right-click", "left-click"):
                raise ValueError("this probe supports configured keyboard buff sequences only")
            at, monotonic_at = time.time(), time.monotonic()
            backend.press(token)
            events.append({"key": token, "cast_request_at": at, "cast_request_monotonic": monotonic_at})
            if token == "w":
                swapped = not swapped
            stop.wait(profile.actions.buff_action_pause_seconds.get(token, 0.4))
            if trace_frames is not None and not stop.is_set() and focused():
                if len(trace_frames) >= 16:
                    raise ValueError("buff trace exceeds sixteen frames")
                acquired = time.monotonic()
                frame = capture.grab().frame
                reading = hud.observe(frame)
                processed = time.monotonic()
                trace_frames.append(
                    (
                        frame,
                        {
                            "key": token,
                            "index": len(events) - 1,
                            "captured_at": acquired,
                            "processed_at": processed,
                            "hud": asdict(reading),
                        },
                    )
                )
                if processed - acquired >= 0.25:
                    status = "trace_screen_stale"
                    break
    finally:
        # 부분 전환 뒤 복귀를 추측해 추가 입력하지 않는다. 결과는 사람 검토 대상으로 남긴다.
        if swapped and status == "dispatched_unconfirmed":
            status = "equipment_dispatch_unbalanced"
    return {
        "status": status,
        "buff_dispatches": events,
        "equipment_dispatch_balanced": not swapped,
        "effect_verified": False,
        "held_after": [],
        "errors": [],
    }


def read_probe_signals(max_signals=2):
    if type(max_signals) is not int or not 1 <= max_signals <= 32:
        raise ValueError("one to thirty-two supervised signals required")
    signals = queue.Queue(maxsize=max_signals + 1)

    def read():
        try:
            for _ in range(max_signals):
                raw = input()
                if len(raw) > 4096:
                    raise ValueError("probe signal exceeds 4096 characters")
                signal = json.loads(raw)
                if not isinstance(signal, dict):
                    raise ValueError("probe signal must be an object")
                signals.put_nowait(signal)
        except Exception as exc:
            signals.put_nowait(exc)

    threading.Thread(target=read, name="d2-buff-probe-signals", daemon=True).start()
    return signals


def wait_probe_signal(
    signals,
    *,
    capture,
    hud,
    focused,
    policy,
    stop,
    deadline,
    phase,
    accept,
    clock=time.monotonic,
    observe_frame=None,
    retry_unverified=False,
    retry_stale=False,
    emit=print,
):
    # 입력 대기는 별도 reader가 맡고, 같은 프로세스는 화면/포커스/중단을 계속 검사한다.
    while not stop.is_set() and clock() < deadline:
        blocked = wait_readable_resources(
            capture, hud, focused, policy, stop, min(deadline, clock() + 0.75), clock=clock, observe_frame=observe_frame
        )
        # 입력 없는 확인 대기 중에는 HUD 가림만 재관측한다. 저자원/포커스/오래된 화면은 무시하지 않는다.
        if blocked == "observation_unverified" and retry_unverified:
            continue
        if blocked:
            raise RuntimeError("{}: {}".format(phase, blocked))
        if stop.is_set() or clock() >= deadline:
            break
        try:
            signal = signals.get_nowait()
        except queue.Empty:
            stop.wait(0.02)
            continue
        if isinstance(signal, Exception):
            raise signal
        if signal.get("phase") != phase:
            raise ValueError("unexpected supervised probe phase")
        try:
            accept(signal)
        except StaleSupervision as exc:
            if not retry_stale:
                raise
            emit("AWAIT_FRESH_SUPERVISION: {}: {}; no input authorized".format(phase, exc))
            continue
        return signal
    raise RuntimeError("{}: interrupted or confirmation timeout".format(phase))


def confirm_initial_probe(signal, character, last_cast_wall, now):
    observed = signal.get("observed_at")
    if (
        signal.get("character_id") != character
        or signal.get("source") != "supervised_visual"
        or any(signal.get(name) is not True for name in ("effect_verified", "equipment_restored", "sequence_effects_verified"))
        or type(observed) not in (int, float)
        or not math.isfinite(observed)
        or not last_cast_wall <= observed <= now
    ):
        raise ValueError("fresh post-sequence supervised effect/equipment proof required")
    if now - observed > 15:
        raise StaleSupervision("post-sequence proof is stale (age {:.3f}s; limit 15s)".format(now - observed))
