//! 한 번의 포션 요청과 새 HUD/벨트 확인으로 한정한 비상 회복.
use super::{
    buffing::BeltVisibility,
    field_preparation::PreparationEnvironment,
    frame::BgrFrame,
    survival::CharacterSurvivalPolicy,
    survival_vision::{HudReading, SlotReading, resource_guard},
};
use serde::Serialize;
use std::{collections::BTreeMap, fmt::Display, sync::Arc};

pub trait RecoveryEnvironment: PreparationEnvironment {
    fn closed_belt_slots(
        &mut self,
        frame: &Arc<BgrFrame>,
        reading: &HudReading,
    ) -> Result<Vec<SlotReading>, Self::Error>;
}
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStatus {
    Unconfirmed,
    PreconditionUnverified,
    VisiblePotionUnverified,
    ResourceRecoveryUnconfirmed,
    ResourceLowDuringBelt,
    BeltReconciliationUnconfirmed,
    RecoveryError,
    PotionsEmpty,
    BeltClosureUnconfirmed,
    ResourcesRecoveredAndBeltReconciled,
}
#[derive(Debug, Serialize)]
pub struct BeltFailure {
    observed_at: f64,
    visibility: BeltVisibility,
    status: String,
    life_ratio: Option<f64>,
    mana_ratio: Option<f64>,
    slots: Vec<SlotReading>,
}
#[derive(Debug, Serialize)]
pub struct RecoveryRecord {
    started_at: f64,
    observed_at: f64,
    pub status: RecoveryStatus,
    potion_requests: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    before_ratios: Option<[Option<f64>; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovered_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    after_ratios: Option<[Option<f64>; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    belt_unverified: Option<BeltFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    belt_counts: Option<BTreeMap<u8, i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    potions_remaining: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    belt_observed_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    belt_visibility_closed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    belt_closed_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replenishment_required: Option<bool>,
}
impl RecoveryRecord {
    fn new(started_at: f64, observed_at: f64) -> Self {
        Self {
            started_at,
            observed_at,
            status: RecoveryStatus::Unconfirmed,
            potion_requests: 0,
            before_ratios: None,
            requested_at: None,
            column: None,
            key: None,
            recovered_at: None,
            after_ratios: None,
            belt_unverified: None,
            belt_counts: None,
            potions_remaining: None,
            belt_observed_at: None,
            error: None,
            belt_visibility_closed: None,
            belt_closed_verified: None,
            ended_at: None,
            replenishment_required: None,
        }
    }
}
pub struct RecoveryEvidence {
    pub filename: String,
    pub frame: Arc<BgrFrame>,
}
#[derive(Default)]
pub struct EmergencyResourceRecovery {
    records: Vec<RecoveryRecord>,
    evidence: Vec<RecoveryEvidence>,
}
impl EmergencyResourceRecovery {
    pub fn records(&self) -> &[RecoveryRecord] {
        &self.records
    }
    pub fn evidence(&self) -> &[RecoveryEvidence] {
        &self.evidence
    }
    pub fn take_evidence(&mut self) -> Vec<RecoveryEvidence> {
        std::mem::take(&mut self.evidence)
    }
    pub fn recover<E: RecoveryEnvironment>(
        &mut self,
        env: &mut E,
        policy: &CharacterSurvivalPolicy,
        frame: &Arc<BgrFrame>,
        reading: &HudReading,
        acquired: f64,
    ) -> Result<bool, E::Error>
    where
        E::Error: Display,
    {
        if self.records.len() >= 12 {
            return Ok(false);
        }
        self.records
            .push(RecoveryRecord::new(env.clock(), acquired));
        let number = self.records.len();
        let record = &mut self.records[number - 1];
        if !active(env)?
            || !(0.0..0.25).contains(&(env.clock() - acquired))
            || reading.belt_visibility != BeltVisibility::Closed
            || resource_guard(reading, policy) != Some("resource_low")
        {
            record.status = RecoveryStatus::PreconditionUnverified;
            return Ok(false);
        }
        let before = ratios(reading);
        record.before_ratios = Some(before);
        let slots: BTreeMap<_, _> = env
            .closed_belt_slots(frame, reading)?
            .into_iter()
            .map(|slot| (slot.column, slot))
            .collect();
        let column = policy.belt_columns().iter().find(|column| {
            column.kind() == policy.potion_kind()
                && slots
                    .get(&column.column())
                    .is_some_and(|slot| slot.kind.as_deref() == Some(column.kind()))
        });
        let Some(column) = column else {
            record.status = RecoveryStatus::VisiblePotionUnverified;
            return Ok(false);
        };
        if !active(env)? || env.clock() - acquired >= 0.25 {
            record.status = RecoveryStatus::VisiblePotionUnverified;
            return Ok(false);
        }
        let prefix = format!("resource-recovery-{number:02}");
        self.evidence.push(RecoveryEvidence {
            filename: format!("{prefix}-before.png"),
            frame: frame.clone(),
        });
        let mut attempt = Attempt {
            policy,
            before,
            deadline: env.clock() + 3.0,
            opened: false,
            last_belt: None,
            belt_resource_low: false,
        };
        let continued = match attempt.run(env, record, &mut self.evidence, &prefix, column) {
            Ok(continued) => continued,
            Err(error) => {
                record.status = RecoveryStatus::RecoveryError;
                record.error = Some(error.to_string());
                false
            }
        };
        let closed = attempt.close(env, record)?;
        record.belt_closed_verified = Some(closed);
        record.ended_at = Some(env.clock());
        if !continued {
            return Ok(false);
        }
        if !closed || record.potions_remaining.unwrap_or(0) == 0 {
            record.status = if closed {
                RecoveryStatus::PotionsEmpty
            } else {
                RecoveryStatus::BeltClosureUnconfirmed
            };
            return Ok(false);
        }
        record.status = RecoveryStatus::ResourcesRecoveredAndBeltReconciled;
        record.replenishment_required = Some(
            record
                .potions_remaining
                .is_some_and(|count| count <= i64::from(policy.replenish_at_or_below())),
        );
        Ok(true)
    }
}
fn active<E: PreparationEnvironment>(env: &E) -> Result<bool, E::Error> {
    Ok(!env.stopped() && env.focused()?)
}
fn ratios(reading: &HudReading) -> [Option<f64>; 2] {
    [reading.life.ratio(), reading.mana.ratio()]
}
struct Current {
    frame: Arc<BgrFrame>,
    reading: HudReading,
    acquired: f64,
}
struct Attempt<'a> {
    policy: &'a CharacterSurvivalPolicy,
    before: [Option<f64>; 2],
    deadline: f64,
    opened: bool,
    last_belt: Option<Current>,
    belt_resource_low: bool,
}
impl Attempt<'_> {
    fn wait<E: RecoveryEnvironment>(
        &mut self,
        env: &mut E,
        deadline: f64,
        accept: impl Fn(&HudReading) -> bool,
        remember: bool,
        protect: bool,
    ) -> Result<Option<Current>, E::Error> {
        while env.clock() < deadline && active(env)? {
            let acquired = env.clock();
            let frame = env.capture()?;
            let reading = env.hud(&frame)?;
            if remember {
                self.last_belt = Some(Current {
                    frame: frame.clone(),
                    reading: reading.clone(),
                    acquired,
                });
            }
            let now = env.clock();
            if now >= deadline {
                return Ok(None);
            }
            if (0.0..0.25).contains(&(now - acquired)) && active(env)? {
                if protect
                    && (reading
                        .life
                        .ratio()
                        .is_some_and(|ratio| ratio < self.policy.life_potion_below())
                        || reading
                            .mana
                            .ratio()
                            .is_some_and(|ratio| ratio < self.policy.mana_potion_below()))
                {
                    self.belt_resource_low = true;
                    return Ok(None);
                }
                if accept(&reading) {
                    return Ok(Some(Current {
                        frame,
                        reading,
                        acquired,
                    }));
                }
            }
            env.wait(0.02)?;
        }
        Ok(None)
    }
    fn run<E: RecoveryEnvironment>(
        &mut self,
        env: &mut E,
        record: &mut RecoveryRecord,
        evidence: &mut Vec<RecoveryEvidence>,
        prefix: &str,
        column: &super::survival::BeltColumnPolicy,
    ) -> Result<bool, E::Error> {
        let key = column.key();
        env.press(key)?;
        record.potion_requests = 1;
        record.requested_at = Some(env.clock());
        record.column = Some(column.column());
        record.key = Some(key.into());
        let before = self.before;
        let limits = [
            self.policy.life_potion_below(),
            self.policy.mana_potion_below(),
        ];
        let recovered = move |reading: &HudReading| {
            reading.belt_visibility == BeltVisibility::Closed
                && ratios(reading).into_iter().zip(before).zip(limits).all(
                    |((after, prior), limit)| {
                        after.is_some_and(|after| {
                            after >= limit
                                && prior.is_none_or(|prior| prior >= limit || after > prior)
                        })
                    },
                )
        };
        let Some(current) = self.wait(
            env,
            self.deadline.min(env.clock() + 1.0),
            recovered,
            false,
            false,
        )?
        else {
            record.status = RecoveryStatus::ResourceRecoveryUnconfirmed;
            return Ok(false);
        };
        evidence.push(RecoveryEvidence {
            filename: format!("{prefix}-recovered.png"),
            frame: current.frame,
        });
        record.recovered_at = Some(current.acquired);
        record.after_ratios = Some(ratios(&current.reading));
        if !active(env)? {
            return Ok(false);
        }
        env.press(self.policy.belt_toggle_key())?;
        self.opened = true;
        let policy = self.policy;
        let current = self.wait(
            env,
            self.deadline.min(env.clock() + 0.75),
            |reading| {
                reading.belt_visibility == BeltVisibility::Expanded
                    && reading.belt_contents(policy).is_some()
            },
            true,
            true,
        )?;
        let Some(current) = current else {
            record.status = if self.belt_resource_low {
                RecoveryStatus::ResourceLowDuringBelt
            } else {
                RecoveryStatus::BeltReconciliationUnconfirmed
            };
            if let Some(last) = &self.last_belt {
                evidence.push(RecoveryEvidence {
                    filename: format!("{prefix}-belt-unverified.png"),
                    frame: last.frame.clone(),
                });
                record.belt_unverified = Some(BeltFailure {
                    observed_at: last.acquired,
                    visibility: last.reading.belt_visibility,
                    status: last.reading.belt_status.clone(),
                    life_ratio: last.reading.life.ratio(),
                    mana_ratio: last.reading.mana.ratio(),
                    slots: last.reading.slots.clone(),
                });
            }
            return Ok(false);
        };
        evidence.push(RecoveryEvidence {
            filename: format!("{prefix}-belt.png"),
            frame: current.frame,
        });
        // wait의 수용 조건이 같은 불변 reading의 전체 슬롯을 이미 확인했다.
        if let Some(contents) = current.reading.belt_contents(policy) {
            record.belt_counts = Some(
                contents
                    .iter()
                    .map(|(number, reading)| (*number, reading.count))
                    .collect(),
            );
            record.potions_remaining = Some(
                contents
                    .values()
                    .filter(|reading| reading.kind.as_deref() == Some(policy.potion_kind()))
                    .map(|reading| reading.count)
                    .sum(),
            );
        }
        record.belt_observed_at = Some(current.acquired);
        Ok(true)
    }
    fn close<E: RecoveryEnvironment>(
        &mut self,
        env: &mut E,
        record: &mut RecoveryRecord,
    ) -> Result<bool, E::Error> {
        if !self.opened || !active(env)? || env.clock() >= self.deadline {
            return Ok(false);
        }
        let mut current = self
            .last_belt
            .take()
            .filter(|current| (0.0..0.25).contains(&(env.clock() - current.acquired)));
        if current.as_ref().is_none_or(|current| {
            !matches!(
                current.reading.belt_visibility,
                BeltVisibility::Closed | BeltVisibility::Expanded
            )
        }) {
            current = self.wait(
                env,
                self.deadline.min(env.clock() + 0.5),
                |reading| {
                    matches!(
                        reading.belt_visibility,
                        BeltVisibility::Closed | BeltVisibility::Expanded
                    )
                },
                false,
                false,
            )?;
        }
        if current
            .as_ref()
            .is_some_and(|current| current.reading.belt_visibility == BeltVisibility::Expanded)
            && active(env)?
            && env.clock() < self.deadline
            && current
                .as_ref()
                .is_some_and(|current| (0.0..0.25).contains(&(env.clock() - current.acquired)))
        {
            env.press(self.policy.belt_toggle_key())?;
            current = self.wait(
                env,
                self.deadline.min(env.clock() + 0.5),
                |reading| reading.belt_visibility == BeltVisibility::Closed,
                false,
                false,
            )?;
        }
        let visibility = current
            .as_ref()
            .is_some_and(|current| current.reading.belt_visibility == BeltVisibility::Closed);
        let closed = visibility
            && current
                .as_ref()
                .is_some_and(|current| resource_guard(&current.reading, self.policy).is_none());
        record.belt_visibility_closed = Some(visibility);
        Ok(closed)
    }
}
