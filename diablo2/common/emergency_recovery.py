"""현재 비율·보이는 포션과 새 HUD/전체 벨트로 제한된 비상 회복을 연결한다."""

import time
from dataclasses import asdict

from diablo2.common.supervised_field import resource_guard


class EmergencyResourceRecovery:
    def __init__(self, capture, hud, backend, focused, policy, stop, *, records, evidence, clock=time.monotonic):
        self.capture, self.hud, self.backend = capture, hud, backend
        self.focused, self.policy, self.stop, self.clock = focused, policy, stop, clock
        self.records, self.evidence = records, evidence

    def active(self):
        return not self.stop.is_set() and self.focused()

    def _wait(self, deadline, accept, observe=None, protect_resources=False):
        while self.clock() < deadline and self.active():
            acquired = self.clock()
            frame = self.capture.grab().frame
            reading = self.hud.observe(frame)
            if observe is not None:
                observe(frame, reading, acquired)
            now = self.clock()
            if now >= deadline:
                return None
            if 0 <= now - acquired < 0.25 and self.active():
                if protect_resources and any(
                    ratio is not None and ratio < limit
                    for ratio, limit in (
                        (reading.life.ratio, self.policy.life_potion_below),
                        (reading.mana.ratio, self.policy.mana_potion_below),
                    )
                ):
                    self._belt_resource_low = True
                    return None
                if accept(reading):
                    return frame, reading, acquired
            self.stop.wait(0.02)
        return None

    def __call__(self, frame, reading, acquired):
        if len(self.records) >= 12:
            return False
        record = dict(started_at=self.clock(), observed_at=acquired, status="unconfirmed", potion_requests=0)
        self.records.append(record)
        if (
            not self.active()
            or not 0 <= self.clock() - acquired < 0.25
            or reading.belt_visibility != "closed"
            or resource_guard(reading, self.policy) != "resource_low"
        ):
            record["status"] = "precondition_unverified"
            return False
        before = (reading.life.ratio, reading.mana.ratio)
        record["before_ratios"] = before
        slots = {slot.column: slot for slot in self.hud.closed_belt_slots(frame, reading)}
        column = next(
            (
                rule
                for rule in self.policy.belt_columns
                if rule.kind == self.policy.potion_kind and rule.column in slots and slots[rule.column].kind == rule.kind
            ),
            None,
        )
        if column is None or not self.active() or self.clock() - acquired >= 0.25:
            record["status"] = "visible_potion_unverified"
            return False
        prefix = "resource-recovery-{:02d}".format(len(self.records))
        self.evidence.append((prefix + "-before.png", frame))
        deadline, opened, closed, last_belt = self.clock() + 3, False, False, []
        self._belt_resource_low = False
        try:
            self.backend.press(column.key)
            record.update(potion_requests=1, requested_at=self.clock(), column=column.column, key=column.key)

            def recovered(value):
                ratios = (value.life.ratio, value.mana.ratio)
                limits = (self.policy.life_potion_below, self.policy.mana_potion_below)
                return value.belt_visibility == "closed" and all(
                    after is not None and after >= limit and (prior is None or prior >= limit or after > prior)
                    for after, prior, limit in zip(ratios, before, limits)
                )

            current = self._wait(min(deadline, self.clock() + 1), recovered)
            if current is None:
                record["status"] = "resource_recovery_unconfirmed"
                return False
            self.evidence.append((prefix + "-recovered.png", current[0]))
            record.update(recovered_at=current[2], after_ratios=(current[1].life.ratio, current[1].mana.ratio))
            if not self.active():
                return False
            self.backend.press(self.policy.belt_toggle_key)
            opened = True

            def remember_belt(frame, reading, acquired):
                last_belt[:] = [(frame, reading, acquired)]

            current = self._wait(
                min(deadline, self.clock() + 0.75),
                lambda value: value.belt_visibility == "expanded" and value.belt_contents(self.policy) is not None,
                observe=remember_belt,
                protect_resources=True,
            )
            if current is None:
                record["status"] = "resource_low_during_belt" if self._belt_resource_low else "belt_reconciliation_unconfirmed"
                if last_belt:
                    failed_frame, failed_reading, failed_at = last_belt[0]
                    self.evidence.append((prefix + "-belt-unverified.png", failed_frame))
                    record["belt_unverified"] = dict(
                        observed_at=failed_at,
                        visibility=failed_reading.belt_visibility,
                        status=failed_reading.belt_status,
                        life_ratio=failed_reading.life.ratio,
                        mana_ratio=failed_reading.mana.ratio,
                        slots=[asdict(slot) for slot in failed_reading.slots],
                    )
                return False
            self.evidence.append((prefix + "-belt.png", current[0]))
            contents = current[1].belt_contents(self.policy)
            record["belt_counts"] = {number: count for number, (_, count) in contents.items()}
            record["potions_remaining"] = sum(count for number, (kind, count) in contents.items() if kind == self.policy.potion_kind)
            record["belt_observed_at"] = current[2]
        except KeyboardInterrupt:
            self.stop.set()
            raise
        except Exception as error:
            record.update(status="recovery_error", error=str(error))
            return False
        finally:
            if opened and self.active() and self.clock() < deadline:
                # 같은 현재 펼침 관측으로 닫아 추가 촬영 동안 위험에 노출되지 않게 한다.
                current = last_belt[0] if last_belt and 0 <= self.clock() - last_belt[0][2] < 0.25 else None
                if current is None or current[1].belt_visibility not in ("closed", "expanded"):
                    current = self._wait(min(deadline, self.clock() + 0.5), lambda value: value.belt_visibility in ("closed", "expanded"))
                if (
                    current is not None
                    and current[1].belt_visibility == "expanded"
                    and self.active()
                    and self.clock() < deadline
                    and 0 <= self.clock() - current[2] < 0.25
                ):
                    self.backend.press(self.policy.belt_toggle_key)
                    current = self._wait(min(deadline, self.clock() + 0.5), lambda value: value.belt_visibility == "closed")
                closed = current is not None and current[1].belt_visibility == "closed" and resource_guard(current[1], self.policy) is None
                record["belt_visibility_closed"] = current is not None and current[1].belt_visibility == "closed"
            record["belt_closed_verified"] = closed
            record["ended_at"] = self.clock()
        if not closed or record.get("potions_remaining", 0) == 0:
            record["status"] = "potions_empty" if closed else "belt_closure_unconfirmed"
            return False
        record["status"] = "resources_recovered_and_belt_reconciled"
        record["replenishment_required"] = record["potions_remaining"] <= self.policy.replenish_at_or_below
        return True
