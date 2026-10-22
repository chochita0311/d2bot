"""전투 뒤에만 라벨을 켜고 같은 field backend로 제한된 줍기 입력을 요청한다."""

from __future__ import annotations

import math
import time

from diablo2.actions.loot_pickup import LootPickupSession
from diablo2.common.supervised_field import resource_guard
from diablo2.common.frame_geometry import frame_to_desktop


class _LootAbort(RuntimeError):
    pass


class ConditionalLootController:
    """새 방의 기본 OFF와 자신이 보낸 토글만 추적한다. 영상으로 ON/OFF를 확정하지 않는다.

    potion_count_supplier는 count, observed_at, confirmed=True를 가진 현재 관측을 반환한다.
    입력 요청이나 라벨 소실만으로 처치/획득 성공을 기록하지 않는다.
    """

    POTION_LABELS = {
        "full rejuvenation potion": "full_rejuvenation_potion",
        "full_rejuvenation_potion": "full_rejuvenation_potion",
        "rejuvenation potion": "rejuvenation_potion",
        "rejuvenation_potion": "rejuvenation_potion",
    }

    def __init__(
        self,
        config,
        capture,
        hud,
        backend,
        focused,
        profile,
        stop,
        panels_open,
        *,
        clock=time.monotonic,
        scanner=None,
        deadline_supplier=None,
        potion_count_supplier=None,
        ground_supplier=None,
        recover_resources=None,
        cast_reference_seconds=0.32,
        max_scan_seconds=3,
        max_scan_frames=12,
        max_clicks=2,
    ):
        if type(max_scan_seconds) not in (int, float) or not math.isfinite(max_scan_seconds) or not 0 < max_scan_seconds <= 3:
            raise ValueError("loot scan duration must be within three seconds")
        if type(max_scan_frames) is not int or not 1 <= max_scan_frames <= 30:
            raise ValueError("bounded loot frame count required")
        if type(max_clicks) is not int or not 0 <= max_clicks <= 4:
            raise ValueError("bounded loot click count required")
        if not 0 < cast_reference_seconds < 1.5:
            raise ValueError("configured teleport cast reference required")
        self.config, self.capture, self.hud, self.backend = config, capture, hud, backend
        self.focused, self.profile, self.stop, self.panels_open = focused, profile, stop, panels_open
        self.clock, self.scanner = clock, scanner if scanner is not None else LootPickupSession(config)
        self.deadline_supplier, self.potion_count_supplier = deadline_supplier, potion_count_supplier
        self.ground_supplier, self.cast_reference_seconds = ground_supplier, cast_reference_seconds
        self.recover_resources = recover_resources
        self.resource_exit_required = False
        self._recovery_attempted = False
        self.max_scan_seconds, self.max_scan_frames, self.max_clicks = max_scan_seconds, max_scan_frames, max_clicks
        self.labels_state, self._prepared = "unknown", False
        self.records = []
        self._combat_ids = []
        self.last_hud_failure = None
        self.last_landing_observation = None

    @property
    def movement_allowed(self):
        return self._prepared and self.labels_state == "off"

    def _active(self):
        if self.stop.is_set():
            raise _LootAbort("stopped")
        if not self.focused():
            raise _LootAbort("focus_lost")

    def _buff_deadline(self, explicit):
        values = [
            value for value in (explicit, self.deadline_supplier() if self.deadline_supplier is not None else None) if value is not None
        ]
        if any(type(value) not in (int, float) or not math.isfinite(value) for value in values):
            raise _LootAbort("buff_deadline_unverified")
        return min(values) if values else None

    def _guard(self, acquired=None, *, buff_expires_at=None, scan_deadline=None):
        self._active()
        now = self.clock()
        if acquired is not None and not 0 <= now - acquired < 0.25:
            raise _LootAbort("screen_stale")
        due = self._buff_deadline(buff_expires_at)
        if due is not None and now >= due:
            raise _LootAbort("buff_renewal_due")
        if scan_deadline is not None and now >= scan_deadline:
            raise _LootAbort("scan_deadline")

    def _read(self, *, buff_expires_at=None, scan_deadline=None, restoring=False):
        read_deadline, pending = self.clock() + 0.75, None
        while True:
            self._active()
            if pending is not None:
                if not restoring:
                    self._guard(buff_expires_at=buff_expires_at, scan_deadline=scan_deadline)
                if self.clock() >= read_deadline:
                    raise _LootAbort("observation_unverified")
            acquired = self.clock()
            frame = self.capture.grab().frame
            reading = self.hud.observe(frame)
            blocked = resource_guard(reading, self.profile.survival)
            if blocked == "resource_low" and self.recover_resources is not None:
                self._active()
                if not 0 <= self.clock() - acquired < 0.25:
                    raise _LootAbort("screen_stale")
                if self.panels_open(frame):
                    raise _LootAbort("panel_open")
                # 생존 입력은 검사/버프 시한보다 우선한다. 회복 뒤에도 이번 줍기는 중단한다.
                self.resource_exit_required = True
                if self._recovery_attempted:
                    raise _LootAbort("resource_low")
                self._recovery_attempted = True
                recovered = self.recover_resources(frame, reading, acquired)
                self.resource_exit_required = not recovered
                if restoring and recovered:
                    continue
                raise _LootAbort("resource_recovered" if recovered else "resource_low")
            if self.panels_open(frame) or blocked not in (None, "observation_unverified"):
                raise _LootAbort(blocked or "panel_open")
            if restoring:
                self._active()
                if not 0 <= self.clock() - acquired < 0.25:
                    raise _LootAbort("screen_stale")
            else:
                self._guard(acquired, buff_expires_at=buff_expires_at, scan_deadline=scan_deadline)
            if pending is not None and self.clock() >= read_deadline:
                raise _LootAbort("observation_unverified")
            if blocked is None:
                if pending is not None:
                    pending["recovered_at"] = acquired
                return frame, acquired
            # 숫자 가림만 입력 없이 새로 관측한다. 저자원/패널/오래된 화면은 재시도하지 않는다.
            pending = dict(
                acquired_at=acquired,
                life_ratio=reading.life.ratio,
                mana_ratio=reading.mana.ratio,
                belt_visibility=reading.belt_visibility,
                restoring=restoring,
                deadline=read_deadline,
                recovered_at=None,
            )
            self.last_hud_failure = (frame, pending)
            self.stop.wait(min(0.02, max(0, read_deadline - self.clock())))

    def _toggle(self, target, *, buff_expires_at=None, scan_deadline=None, restoring=False, verified_acquired_at=None):
        if self.labels_state == target:
            return False
        if self.labels_state not in ("on", "off"):
            raise _LootAbort("item_labels_state_unknown")
        if verified_acquired_at is None:
            _, acquired = self._read(buff_expires_at=buff_expires_at, scan_deadline=scan_deadline, restoring=restoring)
        else:
            # 같은 _read에서 자원/패널 확인을 끝낸 화면만 재사용한다. 관찰 시각은 갱신하지 않는다.
            acquired = verified_acquired_at
            self._guard(acquired, buff_expires_at=buff_expires_at, scan_deadline=scan_deadline)
        if restoring:
            self._active()
        else:
            self._guard(acquired, buff_expires_at=buff_expires_at, scan_deadline=scan_deadline)
        # 전송 실패나 포커스 상실 때 토글을 재시도하면 반대 상태가 될 수 있다.
        self.labels_state = "unknown"
        self.backend.press("alt")
        self._active()
        self.labels_state = target
        return True

    def prepare(self, *, fresh_room, labels_on=None):
        if type(fresh_room) is not bool or (labels_on is not None and type(labels_on) is not bool):
            raise ValueError("explicit item label starting state required")
        if fresh_room and labels_on is True:
            raise ValueError("fresh room defaults to labels OFF")
        self._read(restoring=True)
        if self._prepared:
            if self.labels_state != "off":
                raise RuntimeError("item_labels_state_unknown")
            return dict(status="off_preserved", alt_sent=False, labels_state="off", basis="program_owned_toggle_state")
        if not fresh_room and labels_on is None:
            raise RuntimeError("item_labels_state_unknown")
        self.labels_state = "on" if labels_on is True else "off"
        toggled = self._toggle("off", restoring=True) if labels_on is True else False
        self._prepared = True
        return dict(
            status="off_requested" if toggled else "off_known",
            alt_sent=toggled,
            labels_state="off",
            basis="fresh_room_default_off_toggle_mode" if fresh_room else "supervised_starting_toggle_state",
        )

    def _current_potion_count(self):
        if self.potion_count_supplier is None:
            return None
        observed = self.potion_count_supplier()
        if not isinstance(observed, dict):
            return None
        count = observed.get("count", observed.get("potions_remaining"))
        acquired = observed.get("observed_at", observed.get("belt_observed_at"))
        if (
            observed.get("confirmed") is not True
            or type(count) is not int
            or count < 0
            or type(acquired) not in (int, float)
            or not math.isfinite(acquired)
            or not 0 <= self.clock() - acquired < 0.25
        ):
            return None
        return count

    def _eligible(self, candidates, attempted):
        ignored = {value.casefold() for value in self.config.shared_loot.ignore_labels}
        pickup = getattr(self.profile.survival, "potion_pickup", None)
        count = self._current_potion_count()
        allowed = []
        for candidate in candidates:
            label = candidate.label.casefold()
            if label in attempted or label in ignored or label == "gold":
                continue
            potion = self.POTION_LABELS.get(label)
            if potion is not None:
                if pickup is None:
                    continue
                if potion != pickup.preferred_kind and not (
                    potion == pickup.fallback_kind and count is not None and count <= pickup.fallback_at_or_below
                ):
                    continue
            allowed.append(candidate)
        return allowed

    @staticmethod
    def _center(hit):
        return (hit.match.top_left[0] + hit.match.width // 2, hit.match.top_left[1] + hit.match.height // 2)

    def after_combat(self, *, buff_expires_at=None, reason="combat_ended_scan", combat_id=None):
        self._recovery_attempted = False
        result = dict(
            status="inspection_pending", reason=reason, kill_confirmed=False, pickups_confirmed=0, clicks=[], teleports=[], scan_frames=0
        )
        result["scan_observations"] = []
        if combat_id is not None and combat_id in self._combat_ids:
            result.update(status="combat_scan_already_handled", labels_state=self.labels_state, movement_allowed=self.movement_allowed)
            return result
        if combat_id is not None:
            self._combat_ids.append(combat_id)
            del self._combat_ids[:-32]
        self.records.append(result)
        started, attempted, quiet = self.clock(), set(), 0
        deadline = started + self.max_scan_seconds
        try:
            if not self.movement_allowed:
                raise _LootAbort("item_labels_state_unknown")
            self._guard(buff_expires_at=buff_expires_at, scan_deadline=deadline)
            result["labels_on_sent"] = self._toggle("on", buff_expires_at=buff_expires_at, scan_deadline=deadline)
            self.stop.wait(0.05)
            candidates = self.scanner._build_candidates()
            result["configured_labels"] = [candidate.label for candidate in candidates]
            pickup = getattr(self.profile.survival, "potion_pickup", None)
            if pickup is not None:
                available = {self.POTION_LABELS.get(candidate.label.casefold()) for candidate in candidates}
                result["unavailable_potion_kinds"] = [
                    kind for kind in (pickup.preferred_kind, pickup.fallback_kind) if kind is not None and kind not in available
                ]
            while result["scan_frames"] < self.max_scan_frames:
                frame, acquired = self._read(buff_expires_at=buff_expires_at, scan_deadline=deadline)
                allowed = self._eligible(candidates, attempted)
                result["scan_frames"] += 1
                hit = self.scanner.scan_frame(frame, allowed) if allowed else None
                result["scan_observations"].append(
                    dict(acquired_at=acquired, processed_at=self.clock(), label=None if hit is None else hit.label)
                )
                self._guard(acquired, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                if hit is None:
                    quiet += 1
                    if quiet >= 2:
                        result.update(status="inspection_complete", bounded_reason="two_frames_without_eligible_candidate")
                        break
                    self.stop.wait(0.05)
                    continue
                quiet = 0
                if len(result["clicks"]) >= self.max_clicks:
                    result.update(status="inspection_complete", bounded_reason="click_limit")
                    break
                if not callable(getattr(self.backend, "click", None)) or not callable(getattr(self.backend, "pointer_matches", None)):
                    raise _LootAbort("loot_click_backend_unavailable")
                center = self._center(hit)
                height, width = frame.shape[:2]
                if not (0 <= center[0] < width and 0 <= center[1] < height):
                    raise _LootAbort("loot_aim_outside_capture")
                aim = frame_to_desktop(self.capture, center)
                self.backend.move(*aim)
                prepared_at = self.clock()
                fresh, fresh_at = self._read(buff_expires_at=buff_expires_at, scan_deadline=deadline)
                reconfirm = [candidate for candidate in self._eligible(candidates, attempted) if candidate.label == hit.label]
                current = self.scanner.scan_frame(fresh, reconfirm) if reconfirm else None
                self._guard(fresh_at, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                if (
                    current is None
                    or current.label != hit.label
                    or math.hypot(*(a - b for a, b in zip(center, self._center(current)))) > 18
                    or not current.match.top_left[0] <= center[0] < current.match.top_left[0] + current.match.width
                    or not current.match.top_left[1] <= center[1] < current.match.top_left[1] + current.match.height
                ):
                    attempted.add(hit.label.casefold())
                    continue
                if not self.backend.pointer_matches(aim):
                    raise _LootAbort("loot_aim_unverified")
                self._guard(fresh_at, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                # 필드에서는 아이템에 단 한 번 텔레포트한 뒤 새 위치에서만 클릭한다.
                # 이동 중 라벨은 OFF이며, 전송/시전 대기를 착지 성공으로 기록하지 않는다.
                approach = dict(label=hit.label, aim=aim, requested_at=None, dispatch_returned=False)
                result["teleports"].append(approach)
                self._toggle("off", buff_expires_at=buff_expires_at, scan_deadline=deadline, verified_acquired_at=fresh_at)
                # 새 라벨 판독/ALT 전송 동안 이미 지난 커서 정착 시간을 다시 기다리지 않는다.
                remaining_settle = max(0, prepared_at + 0.05 - self.clock())
                if remaining_settle:
                    self.stop.wait(remaining_settle)
                self._guard(fresh_at, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                if not self.backend.pointer_matches(aim):
                    raise _LootAbort("loot_aim_unverified")
                approach["requested_at"] = self.clock()
                self.backend.press(self.profile.actions.movement_skill_key)
                approach["dispatch_returned"] = True
                self.stop.wait(self.cast_reference_seconds)
                self._toggle("on", buff_expires_at=buff_expires_at, scan_deadline=deadline)
                landed, landed_at = self._read(buff_expires_at=buff_expires_at, scan_deadline=deadline)
                current = self.scanner.scan_frame(landed, reconfirm) if reconfirm else None
                ground = self.ground_supplier() if self.ground_supplier is not None else None
                # 착지 판독의 누락 원인을 현재 한 장으로 남긴다. 추가 촬영/입력이나 승인 근거를 만들지 않는다.
                landing = dict(
                    acquired_at=landed_at,
                    processed_at=None,
                    label=None if current is None else current.label,
                    item_center=None if current is None else self._center(current),
                    player=None,
                    ground_reason=None,
                    missing=["ground_observer"],
                    status="unverified",
                )
                approach["landing_observation"] = landing
                self.last_landing_observation = (landed, landing)
                if ground is None:
                    raise _LootAbort("loot_landing_unverified")
                prepare = getattr(ground, "prepare_frame", None)
                if prepare is not None:
                    prepare(landed, landed_at, self.hud.observe(landed), self.focused())
                reading = ground.observe(landed, captured_at=landed_at)
                player = reading.point
                landing.update(
                    processed_at=self.clock(),
                    player=player,
                    ground_reason=getattr(reading, "reason", None),
                    missing=(["label"] if current is None else []) + (["player"] if player is None else []),
                    status="unverified" if current is None or player is None else "proximity_pending",
                )
                self._guard(landed_at, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                if current is None or player is None:
                    raise _LootAbort("loot_landing_unverified")
                center = self._center(current)
                if math.hypot((center[0] - player[0]) / 300, (center[1] - player[1]) / 180) > 1:
                    landing["status"] = "not_near_item"
                    raise _LootAbort("loot_landing_not_near_item")
                landing["status"] = "current_item_near_player"
                approach.update(observed_after_at=landed_at, player=player, item_center=center, status="current_item_near_player")
                aim = frame_to_desktop(self.capture, center)
                self.backend.move(*aim)
                fresh, fresh_at = self._read(buff_expires_at=buff_expires_at, scan_deadline=deadline)
                current = self.scanner.scan_frame(fresh, reconfirm) if reconfirm else None
                self._guard(fresh_at, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                if (
                    current is None
                    or current.label != hit.label
                    or math.hypot(*(a - b for a, b in zip(center, self._center(current)))) > 18
                    or not current.match.top_left[0] <= center[0] < current.match.top_left[0] + current.match.width
                    or not current.match.top_left[1] <= center[1] < current.match.top_left[1] + current.match.height
                ):
                    raise _LootAbort("loot_label_changed_after_teleport")
                if not self.backend.pointer_matches(aim):
                    raise _LootAbort("loot_aim_unverified")
                self._guard(fresh_at, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                row = dict(
                    label=hit.label,
                    aim=aim,
                    requested_at=self.clock(),
                    status="click_unconfirmed",
                    pickup_confirmed=False,
                    dispatch_returned=False,
                )
                result["clicks"].append(row)
                self.backend.click()
                row["dispatch_returned"] = True
                attempted.add(hit.label.casefold())
                # 사라짐도 보관함/벨트의 실제 증가 근거가 아니므로 획득 확정으로 바꾸지 않는다.
                after, after_at = self._read(buff_expires_at=buff_expires_at, scan_deadline=deadline)
                visible = self.scanner.scan_frame(after, reconfirm) if reconfirm else None
                self._guard(after_at, buff_expires_at=buff_expires_at, scan_deadline=deadline)
                row.update(observed_after_at=after_at, label_visible_after=visible is not None)
            else:
                result.update(status="inspection_complete", bounded_reason="frame_limit")
        except _LootAbort as error:
            result["status"] = str(error)
            if str(error) in ("stopped", "focus_lost"):
                self.labels_state = "unknown"
        except Exception as error:
            result.update(status="scan_error", error=str(error))
        finally:
            if self.labels_state == "on":
                try:
                    # 버프 만료는 탐색을 선점하지만 안전한 OFF 복원 한 번은 허용한다.
                    result["labels_off_sent"] = self._toggle("off", restoring=True)
                except Exception as error:
                    if isinstance(error, _LootAbort) and str(error) in ("stopped", "focus_lost"):
                        self.labels_state = "unknown"
                    result["restore_error"] = str(error)
            if result["clicks"] and result["status"] == "inspection_complete":
                result["status"] = "click_unconfirmed"
            result.update(labels_state=self.labels_state, movement_allowed=self.movement_allowed, ended_at=self.clock())
        if self.resource_exit_required:
            # 라벨 복원 실패가 원래 저자원 오류를 일반 경로 보류로 바꾸지 않게 한다.
            raise RuntimeError("resource_low")
        return result
