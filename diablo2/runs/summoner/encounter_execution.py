from dataclasses import dataclass, asdict
from collections import deque
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
from pathlib import Path
import threading
import time

import cv2 as cv

from diablo2.common.field_control import HeldFieldInput, FieldDirective
from diablo2.common.frame_geometry import frame_to_desktop
from diablo2.common.supervised_field import resource_guard
from diablo2.common.hostile_probe import NearbyHostileProbe, nearby_actor_proposals
from diablo2.runs.summoner.continuous_navigation import near_ground
from diablo2.runs.summoner.navigation_execution import dispatch_planned_step


_AMBIGUOUS_APPEARANCE = object()


@dataclass(frozen=True)
class SummonerAppearance:
    box: tuple
    ground: tuple
    score: float
    appearance: str = "alive"
    asset_id: str = "summoner-robe"


class SummonerAppearanceObserver:
    """현재 룬 발판의 검토된 로브 외형만 찾는다. 신원/사망 확정과 구분한다."""

    def __init__(self):
        owner = Path(__file__).resolve().parents[3] / "assets/regions/arcane-sanctuary/remastered-ko-1922x1140"
        data = json.loads((owner / "summoner-encounter.json").read_text(encoding="utf-8"))
        self.parts = []
        if data.get("life_review", {}).get("reviewed_by") != "human-owner":
            raise ValueError("reviewed Summoner life calibration required")
        for rule in data["parts"]:
            path = owner / rule["path"]
            if hashlib.sha256(path.read_bytes()).hexdigest() != rule["sha256"]:
                raise ValueError("reviewed Summoner appearance fingerprint required")
            template = cv.imread(str(path), cv.IMREAD_COLOR)
            if template is None:
                raise ValueError("Summoner appearance template unavailable")
            proposal = cv.resize(cv.cvtColor(template, cv.COLOR_BGR2GRAY), None, fx=0.5, fy=0.5, interpolation=cv.INTER_AREA)
            self.parts.append((rule, template, proposal))

    def observe(self, frame, platform):
        return self._observe(frame, platform, "alive")

    def observe_corpse(self, frame, platform):
        return self._observe(frame, platform, "corpse")

    @staticmethod
    def _blue_gold_matches(patch, template, minimum):
        if minimum is None:
            return True

        def bands(frame):
            blue, green, red = (channel.astype("int16") for channel in cv.split(frame))
            return (blue - red >= 15) & (blue - green >= 5), (red - blue >= 20) & (green - blue >= 10)

        # 모양 기준을 보정한 외형은 원본의 파랑/금색 위치를 각각 현재 native 색상으로 확인한다.
        return all(
            reference.any() and float(current[reference].mean()) >= minimum for current, reference in zip(bands(patch), bands(template))
        )

    def _observe(self, frame, platform, state):
        if frame.shape != (1140, 1922, 3):
            raise ValueError("native Summoner appearance frame required")
        if platform is None:
            return None
        boxes = platform.rune_boxes
        if not boxes:
            return None
        candidates = []
        for rule, template, proposal in self.parts:
            if rule.get("appearance", "alive") != state:
                continue
            candidate = self._match(frame, boxes, rule, template, proposal, state)
            if candidate is _AMBIGUOUS_APPEARANCE:
                # 한 부위에서 복수 몸체를 확인했으면 다른 부위의 단일 양성으로 덮지 않는다.
                return None
            if candidate is not None:
                candidates.append(candidate)
        if not candidates:
            return None
        best = max(candidates, key=lambda candidate: candidate.score)
        if any(math.hypot(*(a - b for a, b in zip(best.ground, candidate.ground))) > 60 for candidate in candidates):
            return None
        return best

    @staticmethod
    def _match(frame, boxes, rule, template, proposal, state):
        h, w = template.shape[:2]
        dx, dy = rule["ground_offset"]
        # 기존 지상점 허용 범위에 들어올 수 있는 후보만 비교한다.
        # 같은 native 픽셀/임계값을 쓰며 전체 월드 색상 비교 지연을 줄인다.
        left = max(24, min(b[0] for b in boxes) - 100 - dx)
        right = min(1900, max(b[2] for b in boxes) + 100 - dx + w)
        top = max(55, min(b[1] for b in boxes) - 50 - dy)
        bottom = min(900, max(b[3] for b in boxes) + 500 - dy + h)
        patch = frame[top:bottom, left:right]
        if patch.shape[0] < h or patch.shape[1] < w:
            return None
        # 반해상도 명암은 후보만 제안한다. 모든 승인/복수 몸체 검사는 현재 원본 색상에서 한다.
        coarse = cv.resize(cv.cvtColor(patch, cv.COLOR_BGR2GRAY), None, fx=0.5, fy=0.5, interpolation=cv.INTER_AREA)
        scores = cv.matchTemplate(coarse, proposal, cv.TM_CCOEFF_NORMED)
        found = []
        ph, pw = proposal.shape[:2]
        for _ in range(16):
            _, coarse_score, _, (cx, cy) = cv.minMaxLoc(scores)
            if coarse_score < rule.get("proposal_threshold", 0.65):
                break
            x0, y0 = max(0, cx * 2 - 3), max(0, cy * 2 - 3)
            x1, y1 = min(patch.shape[1], cx * 2 + w + 4), min(patch.shape[0], cy * 2 + h + 4)
            native = patch[y0:y1, x0:x1]
            if native.shape[0] >= h and native.shape[1] >= w:
                _, score, _, (x, y) = cv.minMaxLoc(cv.matchTemplate(native, template, cv.TM_CCOEFF_NORMED))
                if score >= rule["threshold"] and SummonerAppearanceObserver._blue_gold_matches(
                    native[y : y + h, x : x + w], template, rule.get("minimum_blue_gold_agreement")
                ):
                    found.append((score, x0 + x, y0 + y))
            scores[max(0, cy - ph) : cy + ph + 1, max(0, cx - pw) : cx + pw + 1] = -1
        else:
            # 후보가 과도한 화면에서 일부만 골라 신원을 승인하지 않는다.
            return None
        if not found:
            return None
        score, x, y = max(found)
        if score < rule["threshold"]:
            return None
        if any(abs(x - other_x) > w or abs(y - other_y) > h for _, other_x, other_y in found):
            return _AMBIGUOUS_APPEARANCE
        # 축소 격자 위상/몸체 밖 배경 때문에 두 번째 몸체의 후보만 누락될 수 있다.
        # 승인된 부위에 한해서 원본 색상으로 떨어진 복수 몸체를 다시 거부한다.
        uniqueness = cv.matchTemplate(patch, template, cv.TM_CCOEFF_NORMED)
        uniqueness[max(0, y - h) : y + h + 1, max(0, x - w) : x + w + 1] = -1
        if cv.minMaxLoc(uniqueness)[1] >= rule["threshold"]:
            return _AMBIGUOUS_APPEARANCE
        x, y = x + left, y + top
        point = (x + dx, y + dy)
        if not boxes or not min(b[0] for b in boxes) - 100 <= point[0] <= max(b[2] for b in boxes) + 100:
            return None
        if not min(b[1] for b in boxes) - 50 <= point[1] <= max(b[3] for b in boxes) + 500 or point[1] >= 900:
            return None
        return SummonerAppearance((x, y, x + w, y + h), point, float(score), state, rule["id"])


def run_platform_encounter(
    capture,
    hud,
    backend,
    focused,
    profile,
    stop,
    planner,
    objective,
    appearance,
    *,
    buff_expires_at,
    reference_seconds,
    evidence,
    emit=print,
    clock=time.monotonic,
    target_observer=None,
    recover_resources=None,
    loot_controller=None,
    life_tracker=None,
):
    """현재 외형으로 제한된 접근/노바를 실행하고 처치 확인 전에는 복귀를 승인하지 않는다."""
    started = clock()
    deadline = min(started + 12, buff_expires_at)
    result = dict(status="kill_unconfirmed", observations=[], approach_requests=0, nova_dispatches=0, evidence=[])
    held = HeldFieldInput(
        backend, profile.actions.movement_skill_key, profile.actions.primary_attack_skill_key, clock=clock, repeat_seconds=reference_seconds
    )
    watchdog_stop = threading.Event()
    errors = []

    def watchdog():
        while not watchdog_stop.wait(0.02):
            try:
                if stop.is_set() or not focused() or clock() >= deadline:
                    held.release()
                else:
                    held.service(clock())
            except Exception as exc:
                errors.append(str(exc))
                stop.set()
                return

    worker = threading.Thread(target=watchdog, name="summoner-encounter-watchdog", daemon=True)
    worker.start()
    first_nova, missing_since, last, last_sequence = None, None, None, None
    platform_seen, platform_missing_since = False, None
    prelude, saved, last_saved = deque(maxlen=4), 0, float("-inf")
    alive_frame, first_corpse_frame = None, None
    tentative_corpse_frame, tentative_followup_frame = None, None
    approach_frame_pending = False
    observations = ThreadPoolExecutor(max_workers=3, thread_name_prefix="summoner-observation")
    probe = (
        NearbyHostileProbe(
            target_observer,
            backend,
            (capture.target["left"], capture.target["top"]),
            clock=clock,
            to_desktop=lambda point: frame_to_desktop(capture, point),
        )
        if target_observer is not None
        else None
    )

    def retain(name, frame, sequence, acquired, phase):
        evidence.append((name, frame))
        result["evidence"].append(dict(file=name, sequence=sequence, acquired_at=acquired, phase=phase))

    try:
        for sequence in range(120):
            if stop.is_set() or not focused() or clock() >= deadline:
                break
            if loot_controller is not None and not loot_controller.movement_allowed:
                result["blocked"] = "item_labels_not_off"
                break
            acquired = clock()
            last = capture.grab().frame
            last_sequence = sequence
            if tentative_corpse_frame is not None and tentative_followup_frame is None:
                tentative_followup_frame = (sequence, acquired, last)
            prelude.append((sequence, acquired, last))
            if sequence == 0:
                retain("encounter-before.png", last, sequence, acquired, "initial")
                saved += 1
            elif approach_frame_pending and saved < 21:
                # 이동 직후 생존 포즈가 미검출돼도 첫 새 화면은 보정 자료로 남긴다.
                retain("encounter-after-approach-{:03d}.png".format(sequence), last, sequence, acquired, "prefight")
                saved += 1
                approach_frame_pending = False
            row = dict(sequence=sequence, acquired_at=acquired, target=None, player=None, platform_ground=None)
            result["observations"].append(row)
            reading = hud.observe(last)
            blocked = resource_guard(reading, profile.survival)
            if blocked:
                row["blocked"] = blocked
                held.release()
                if blocked == "mercenary_unverified":
                    raise RuntimeError(blocked)
                if blocked == "resource_low":
                    if recover_resources is not None and recover_resources(last, reading, acquired):
                        if probe is not None:
                            probe.reset()
                        continue
                    raise RuntimeError(blocked)
                continue
            prepare = getattr(planner[0], "prepare_frame", None)
            if prepare is not None:
                prepare(last, acquired, reading, focused())
            player_work = observations.submit(planner[0].observe, last, captured_at=acquired)
            platform = objective.observe(last)
            target_work = observations.submit(appearance.observe, last, platform)
            corpse_work = observations.submit(appearance.observe_corpse, last, platform) if life_tracker is not None else None
            player = player_work.result()
            # 발 위치 작업이 끝난 슬롯에서 같은 현재 화면의 바닥을 미리 판독한다.
            # 외형/생존 판독이 끝난 뒤 실제 목표로 선택하며 입력 시한은 그대로 검사한다.
            path_work = None
            if player.point is not None and platform is not None and result["approach_requests"] < 2:
                path_work = observations.submit(
                    planner[1].observe,
                    last,
                    sequence=sequence,
                    generation=0,
                    captured_at=acquired,
                    viewport=(120, 180, 1720, 890),
                    anchor=player.point,
                )
            target = target_work.result()
            corpse = corpse_work.result() if corpse_work is not None else None
            goal = target.ground if target is not None else getattr(platform, "ground_point", None)
            row.update(
                target=None if target is None else asdict(target),
                corpse=None if corpse is None else asdict(corpse),
                player=player.point,
                platform_ground=goal,
                platform_anchor=None
                if platform is None
                else dict(
                    ground_point=getattr(platform, "ground_point", None),
                    evidence_kind=getattr(platform, "evidence_kind", None),
                    rune_boxes=getattr(platform, "rune_boxes", ()),
                ),
            )
            now = clock()
            row["processed_at"] = now
            fresh_player = player.point is not None and 0 <= now - acquired < 0.25
            if platform is not None and fresh_player:
                platform_seen, platform_missing_since = True, None
            elif platform_seen and platform is None and fresh_player:
                platform_missing_since = now if platform_missing_since is None else platform_missing_since
                row["platform_reacquisition_seconds"] = now - platform_missing_since
                if now - platform_missing_since >= 6:
                    held.release()
                    row["action"] = "platform_reacquisition_timeout"
                    break
            if life_tracker is not None:
                row["life"] = life_tracker.observe(target, corpse, platform, sequence=sequence, acquired_at=acquired, now=now)
                if row["life"]["state"] == "alive":
                    alive_frame = (sequence, acquired, last)
                    first_corpse_frame = None
                if row["life"].get("first_dead_sequence") == sequence:
                    first_corpse_frame = (sequence, acquired, last)
                    if tentative_corpse_frame is None:
                        tentative_corpse_frame = first_corpse_frame
                if row["life"]["state"] == "dead":
                    held.release()
                    corpse_sequence, corpse_at, corpse_image = first_corpse_frame
                    if alive_frame is not None and row["life"].get("linked_alive_sequence") == alive_frame[0]:
                        alive_sequence, alive_at, alive_image = alive_frame
                        retain("encounter-alive-witness.png", alive_image, alive_sequence, alive_at, "prefight")
                        saved += 1
                    retain("encounter-first-corpse.png", corpse_image, corpse_sequence, corpse_at, "combat")
                    saved += 1
                    result.update(status="kill_confirmed", kill_confirmed=True, life_confirmation=row["life"])
                    break
                if corpse is not None:
                    held.release()
                    row["action"] = "verify_current_platform_corpse"
                    continue
            if first_nova is not None and saved < 21 and acquired - last_saved >= 0.12:
                retain("encounter-combat-{:03d}.png".format(sequence), last, sequence, acquired, "combat")
                saved += 1
                last_saved = acquired
            hostile = None
            if fresh_player and platform_seen and platform is None and probe is not None:
                # 녹색 효과가 없어도 기존 근접 격자를 호버할 수 있다. 공격은 현재 커서·새 적 패널만 승인한다.
                if probe.pending is None:
                    held.release()
                proposals = nearby_actor_proposals(last, player.point)
                hostile = probe.observe(last, player.point, acquired, points=proposals or None)
                row.update(
                    current_defense_proposals=proposals,
                    current_defense_source="current_actor_effects" if proposals else "current_near_hover_grid",
                    hostile_panel=None if hostile is None else asdict(hostile),
                )
            if not fresh_player or (goal is None and hostile is None):
                held.release()
                missing_since = now if missing_since is None else missing_since
                missing_limit = 6 if fresh_player and platform_seen and platform is None else 1
                row["missing_evidence_limit"] = missing_limit
                if now - missing_since >= missing_limit:
                    break
                continue
            missing_since = None
            if hostile is not None or near_ground(goal, player.point):
                if target is None and hostile is None and probe is not None:
                    if probe.pending is None:
                        held.release()
                    hostile = probe.observe(last, player.point, acquired)
                    row["hostile_panel"] = None if hostile is None else asdict(hostile)
                if target is None and hostile is None:
                    held.release()
                    row["action"] = "platform_hostile_probe" if probe is not None else "platform_hostile_unverified"
                    continue
                if first_nova is None:
                    for before_sequence, before_at, before_frame in prelude:
                        if saved >= 21:
                            break
                        retain(
                            "encounter-prefight-{:03d}.png".format(before_sequence), before_frame, before_sequence, before_at, "prefight"
                        )
                        saved += 1
                    last_saved = acquired
                first_nova = now if first_nova is None else first_nova
                if now - first_nova >= 6:
                    row.update(action="nova_window_expired", nova_window_seconds=now - first_nova)
                    break
                held.apply(
                    FieldDirective(
                        "combat",
                        "current_platform_robe_candidate" if target is not None else "current_platform_hostile_panel",
                        deadline=min(acquired + 0.25, deadline),
                    ),
                    clock(),
                )
                row["action"] = "nova_on_current_appearance" if target is not None else "nova_on_current_hostile_panel"
            else:
                held.release()
                if result["approach_requests"] >= 2:
                    break
                ground, path = planner
                view = path_work.result()
                destination = tuple(a - b for a, b in zip(goal, player.point))
                choice = path.choose(view, "north", destination=destination)
                if choice.aim is None or math.hypot(*(a - b for a, b in zip(choice.aim, goal))) > 100:
                    row["action"] = "target_floor_unverified"
                    break
                aim = frame_to_desktop(capture, choice.aim)
                dispatch = dict(aim=aim, captured_at=acquired, expires_at=deadline)
                row["approach_dispatch"] = dispatch
                sent = dispatch_planned_step(
                    backend,
                    focused,
                    stop,
                    aim,
                    profile.actions.movement_skill_key,
                    acquired,
                    clock=clock,
                    verify_aim=lambda: backend.pointer_matches(aim),
                    expires_at=deadline,
                    diagnostics=dispatch,
                )
                row["action"] = "approach_requested" if sent else "approach_not_sent"
                result["approach_requests"] += int(sent)
                approach_frame_pending = sent
                if sent and target is not None:
                    dispatch["target"] = "current_summoner_appearance"
                if sent and life_tracker is not None:
                    life_tracker.moved()
                    alive_frame = None
                    first_corpse_frame = None
                if not sent:
                    if dispatch.get("reason") == "frame_expired":
                        # 이전 조준을 재전송하지 않고 다음 현재 화면에서 다시 계획한다.
                        continue
                    break
                stop.wait(reference_seconds)
        emit(
            "SUMMONER_ENCOUNTER: "
            + ("current platform corpse confirmed" if result.get("kill_confirmed") else "bounded approach/Nova finished; kill unconfirmed")
        )
    finally:
        observations.shutdown(wait=True)
        watchdog_stop.set()
        worker.join(timeout=1)
        held.close()
        result["nova_dispatches"] = held.snapshot()["dispatches"][profile.actions.primary_attack_skill_key]
        if not result.get("kill_confirmed"):
            # 단발 시체 양성의 직후 미검출을 진단하되 사망 증거와 구분하고 마지막 화면 자리를 남긴다.
            for name, witness, phase in (
                ("encounter-tentative-corpse.png", tentative_corpse_frame, "tentative"),
                ("encounter-tentative-followup.png", tentative_followup_frame, "unconfirmed_followup"),
            ):
                if witness is not None and saved < 23:
                    witness_sequence, witness_at, witness_frame = witness
                    retain(name, witness_frame, witness_sequence, witness_at, phase)
                    saved += 1
        result["evidence_frames"] = saved + int(last is not None)
        if last is not None:
            retain("encounter-after.png", last, last_sequence, acquired, "final")
    if errors:
        raise RuntimeError("encounter watchdog: " + errors[0])
    if loot_controller is not None and (result["nova_dispatches"] or result.get("kill_confirmed")):
        result["loot_inspection"] = loot_controller.after_combat(
            buff_expires_at=buff_expires_at,
            reason="combat_ended_scan",
            combat_id=("encounter", started, result["nova_dispatches"]),
        )
        inspection = result["loot_inspection"]
        result["loot_complete"] = (
            result.get("kill_confirmed") is True
            and inspection.get("status") == "inspection_complete"
            and inspection.get("bounded_reason") == "two_frames_without_eligible_candidate"
            and inspection.get("movement_allowed") is True
            and not inspection.get("clicks")
        )
    return result
