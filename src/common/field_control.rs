//! Input-free field decisions and an explicitly owned backend lease.

mod dry_input;
mod held_input;
mod session;
pub use dry_input::{DryFieldInput, DryInputEvent};
pub use held_input::{FieldInputBackend, HeldFieldInput, HeldInputSnapshot, InputObservation};
pub use session::{FieldInputSession, InputSessionOptions};

use super::casting::CastingEstimate;
use super::config::ConfigError;
use super::error::ControlError;
use super::survival::{
    BeltContents, CharacterSurvivalState, Location, ObservationLimits, ObservationStamp,
    SafetyObservation, SurvivalAction,
};
use super::time;
use serde::{Deserialize, Serialize};

/// Physical desktop coordinates may be negative on monitors left/above the primary.
/// FieldObservation separately preserves its source contract of nonnegative aims.
pub type DesktopPoint = [i64; 2];

#[derive(Debug, Clone, Copy)]
pub struct FieldLimits {
    ages: ObservationLimits,
    combat_seconds: f64,
    arrival_seconds: f64,
    potion_confirmation_seconds: f64,
}
impl FieldLimits {
    pub fn new(
        screen_age: f64,
        belt_age: f64,
        combat_seconds: f64,
        arrival_seconds: f64,
        potion_confirmation_seconds: f64,
    ) -> Result<Self, ConfigError> {
        for (name, value) in [
            ("screen_age", screen_age),
            ("belt_age", belt_age),
            ("combat_seconds", combat_seconds),
            ("arrival_seconds", arrival_seconds),
            ("potion_confirmation_seconds", potion_confirmation_seconds),
        ] {
            time::positive(value, name)?;
        }
        Ok(Self {
            ages: ObservationLimits::new(screen_age, belt_age)?,
            combat_seconds,
            arrival_seconds,
            potion_confirmation_seconds,
        })
    }
    pub fn ages(self) -> ObservationLimits {
        self.ages
    }
    pub fn arrival_seconds(self) -> f64 {
        self.arrival_seconds
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Enemy {
    Near,
    Far,
    Clear,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct FieldObservation {
    safety: SafetyObservation,
    belt_closed: Option<bool>,
    enemy: Enemy,
    travel_aim: Option<DesktopPoint>,
    reachable: Option<bool>,
    arrived: Option<bool>,
    loot_checked: bool,
    hunt_aim: Option<DesktopPoint>,
}
/// Related navigation evidence from the same admitted screen.
#[derive(Debug, Clone, Default)]
pub struct NavigationEvidence {
    pub travel_aim: Option<DesktopPoint>,
    pub reachable: Option<bool>,
    pub arrived: Option<bool>,
    pub loot_checked: bool,
    pub hunt_aim: Option<DesktopPoint>,
}
impl FieldObservation {
    pub fn new(
        safety: SafetyObservation,
        belt_closed: Option<bool>,
        enemy: Enemy,
        navigation: NavigationEvidence,
    ) -> Result<Self, ConfigError> {
        if [navigation.travel_aim, navigation.hunt_aim]
            .into_iter()
            .flatten()
            .any(|point| point.into_iter().any(|coordinate| coordinate < 0))
        {
            return Err(ConfigError::value(
                "aim must be a nonnegative absolute integer point",
            ));
        }
        if enemy == Enemy::Clear && safety.monsters_clear() != Some(true) {
            return Err(ConfigError::value(
                "clear requires positive current safety evidence",
            ));
        }
        if enemy != Enemy::Clear && safety.monsters_clear() == Some(true) {
            return Err(ConfigError::value(
                "unresolved or visible enemies cannot be a clear buff site",
            ));
        }
        Ok(Self {
            safety,
            belt_closed,
            enemy,
            travel_aim: navigation.travel_aim,
            reachable: navigation.reachable,
            arrived: navigation.arrived,
            loot_checked: navigation.loot_checked,
            hunt_aim: navigation.hunt_aim,
        })
    }
    pub fn safety(&self) -> &SafetyObservation {
        &self.safety
    }
    pub fn belt_closed(&self) -> Option<bool> {
        self.belt_closed
    }
    pub fn has_navigation_aims(&self) -> bool {
        self.travel_aim.is_some() || self.hunt_aim.is_some()
    }
    pub fn with_safety(
        &self,
        safety: SafetyObservation,
        belt_closed: Option<bool>,
    ) -> Result<Self, ConfigError> {
        Self::new(
            safety,
            belt_closed,
            self.enemy,
            NavigationEvidence {
                travel_aim: self.travel_aim,
                reachable: self.reachable,
                arrived: self.arrived,
                loot_checked: self.loot_checked,
                hunt_aim: self.hunt_aim,
            },
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldAction {
    Travel,
    Combat,
    Hold,
    UsePotion,
    Buff,
    Exit,
    InspectLoot,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldDirective {
    pub action: FieldAction,
    pub reason: String,
    pub key: Option<String>,
    pub aim: Option<DesktopPoint>,
    pub deadline: Option<f64>,
    pub hunt_visible: bool,
}
impl FieldDirective {
    pub fn new(
        action: FieldAction,
        reason: impl Into<String>,
        key: Option<String>,
        aim: Option<DesktopPoint>,
        deadline: Option<f64>,
        hunt_visible: bool,
    ) -> Result<Self, ConfigError> {
        if let Some(deadline) = deadline {
            time::nonnegative(deadline, "input deadline")?;
        }
        if action == FieldAction::UsePotion && key.as_ref().is_none_or(|key| key.trim().is_empty())
        {
            return Err(ConfigError::value("explicit potion key required"));
        }
        Ok(Self {
            action,
            reason: reason.into(),
            key,
            aim,
            deadline,
            hunt_visible,
        })
    }
    pub fn hold(reason: impl Into<String>) -> Self {
        Self {
            action: FieldAction::Hold,
            reason: reason.into(),
            key: None,
            aim: None,
            deadline: None,
            hunt_visible: false,
        }
    }
    fn simple(action: FieldAction, reason: impl Into<String>) -> Self {
        Self {
            action,
            ..Self::hold(reason)
        }
    }
}

#[derive(Debug)]
pub struct AdaptiveTeleportCadence {
    reference_seconds: f64,
    timeout: f64,
    interval: f64,
    sent_at: Option<f64>,
    aim: Option<DesktopPoint>,
    next_aim_at: f64,
}
impl AdaptiveTeleportCadence {
    pub fn new(estimate: &CastingEstimate, arrival_timeout: f64) -> Result<Self, ConfigError> {
        if estimate.action != "estimate" || estimate.frames.is_none() {
            return Err(ConfigError::value(
                "configured class/FCR reference required",
            ));
        }
        let reference = estimate
            .frames
            .as_ref()
            .and_then(|frames| frames.as_f64())
            .filter(|frames| frames.is_finite() && *frames > 0.0)
            .ok_or_else(|| {
                ConfigError::value("reference animation cannot be represented as runtime seconds")
            })?
            / 25.0;
        time::nonnegative(arrival_timeout, "arrival_timeout")?;
        if arrival_timeout <= reference {
            return Err(ConfigError::value(
                "arrival timeout must exceed reference animation",
            ));
        }
        Ok(Self {
            reference_seconds: reference,
            timeout: arrival_timeout,
            interval: reference,
            sent_at: None,
            aim: None,
            next_aim_at: 0.0,
        })
    }
    pub fn deadline(&self) -> Option<f64> {
        self.sent_at.map(|sent| sent + self.timeout)
    }
    pub fn interval(&self) -> f64 {
        self.interval
    }
    pub fn confirm_arrival(&mut self, now: f64) {
        if let Some(sent) = self.sent_at
            && sent < now
            && now < sent + self.timeout
        {
            self.interval = self.timeout.min(
                self.reference_seconds
                    .max(0.75 * self.interval + 0.25 * (now - sent)),
            );
            self.sent_at = None;
        }
    }
    pub fn reset(&mut self) {
        self.sent_at = None;
        self.aim = None;
        self.next_aim_at = 0.0;
    }
    pub fn aim(&mut self, point: DesktopPoint, now: f64) -> Option<DesktopPoint> {
        if self.sent_at.is_some() || now < self.next_aim_at {
            return self.aim;
        }
        self.aim = Some(point);
        self.sent_at = Some(now);
        self.next_aim_at = now + self.interval;
        self.aim
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldPhase {
    Travel,
    Combat,
    Loot,
}

pub struct FieldControlLoop<B: FieldInputBackend> {
    state: CharacterSurvivalState,
    inputs: HeldFieldInput<B>,
    limits: FieldLimits,
    cadence: AdaptiveTeleportCadence,
    phase: FieldPhase,
    last_stamp: Option<ObservationStamp>,
    combat_started: Option<f64>,
    potion_pending: Option<(ObservationStamp, String, f64)>,
    replenishing: bool,
    paused: bool,
    stopped: bool,
    last: FieldDirective,
    last_now: Option<f64>,
}

impl<B: FieldInputBackend> Drop for FieldControlLoop<B> {
    fn drop(&mut self) {
        self.stopped = true;
        super::error::best_effort_cleanup(|| self.inputs.close());
    }
}
impl<B: FieldInputBackend> FieldControlLoop<B> {
    pub fn new(
        state: CharacterSurvivalState,
        inputs: HeldFieldInput<B>,
        limits: FieldLimits,
        cadence: AdaptiveTeleportCadence,
    ) -> Result<Self, ConfigError> {
        if state.policy().is_none() {
            return Err(ConfigError::value("character survival policy required"));
        }
        if limits.arrival_seconds != cadence.timeout {
            return Err(ConfigError::value(
                "cadence and control arrival limits must agree",
            ));
        }
        Ok(Self {
            state,
            inputs,
            limits,
            cadence,
            phase: FieldPhase::Travel,
            last_stamp: None,
            combat_started: None,
            potion_pending: None,
            replenishing: false,
            paused: false,
            stopped: false,
            last: FieldDirective::hold("not_observed"),
            last_now: None,
        })
    }
    pub fn state(&self) -> &CharacterSurvivalState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut CharacterSurvivalState {
        &mut self.state
    }
    pub fn inputs(&self) -> &HeldFieldInput<B> {
        &self.inputs
    }
    pub fn limits(&self) -> FieldLimits {
        self.limits
    }
    pub fn phase(&self) -> FieldPhase {
        self.phase
    }
    pub fn last(&self) -> &FieldDirective {
        &self.last
    }
    pub fn last_stamp(&self) -> Option<&ObservationStamp> {
        self.last_stamp.as_ref()
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn stopped(&self) -> bool {
        self.stopped
    }
    pub fn potion_pending(&self) -> bool {
        self.potion_pending.is_some()
    }
    fn clock_valid(&mut self, now: f64) -> Result<bool, ControlError> {
        time::nonnegative(now, "now")?;
        if self.last_now.is_some_and(|last| now < last) {
            self.stop()?;
            return Ok(false);
        }
        self.last_now = Some(now);
        Ok(true)
    }
    pub(crate) fn emit(
        &mut self,
        result: FieldDirective,
        now: f64,
    ) -> Result<FieldDirective, ControlError> {
        self.last = result.clone();
        self.inputs.apply(&result, now)?;
        if matches!(
            result.action,
            FieldAction::Combat
                | FieldAction::InspectLoot
                | FieldAction::Buff
                | FieldAction::UsePotion
                | FieldAction::Exit
        ) {
            self.cadence.reset();
        }
        Ok(result)
    }
    pub fn pause(&mut self, paused: bool) -> Result<(), ControlError> {
        self.paused = paused;
        if paused {
            self.inputs.suspend()?;
        } else {
            self.inputs.resume()?;
        }
        self.cadence.reset();
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), ControlError> {
        self.stopped = true;
        self.inputs.close()
    }
    pub fn acknowledge_potion(&mut self, stamp: &ObservationStamp, column: u8) -> bool {
        let Some((sent, key, _)) = &self.potion_pending else {
            return false;
        };
        let rule = self
            .state
            .policy()
            .and_then(|policy| policy.potion_column(column, key))
            .map(str::to_owned);
        let Some(kind) = rule else {
            return false;
        };
        if stamp.sequence() <= sent.sequence() || stamp.observed_at() < sent.observed_at() {
            return false;
        }
        if !self.state.confirm_resource_change(stamp, column, &kind, -1) {
            return false;
        }
        self.potion_pending = None;
        true
    }
    pub fn reconcile_belt(
        &mut self,
        stamp: &ObservationStamp,
        contents: Option<&BeltContents>,
    ) -> bool {
        if self.potion_pending.as_ref().is_some_and(|(sent, _, _)| {
            stamp.sequence() <= sent.sequence() || stamp.observed_at() < sent.observed_at()
        }) {
            return false;
        }
        if !self.state.observe_belt(stamp, contents) {
            return false;
        }
        self.potion_pending = None;
        true
    }
    fn empty_requires_exit(&self) -> bool {
        self.state.potions_remaining() == Some(0)
            && self
                .state
                .policy()
                .is_some_and(|policy| policy.exit_on_empty())
    }
    pub fn tick(&mut self, now: f64) -> Result<FieldDirective, ControlError> {
        if !self.clock_valid(now)? {
            return self.emit(FieldDirective::hold("clock_regressed"), now);
        }
        if self.stopped || self.paused || self.inputs.closed() {
            return self.emit(FieldDirective::hold("stopped_or_paused"), now);
        }
        if self.empty_requires_exit() {
            return self.emit(
                FieldDirective::simple(FieldAction::Exit, "potions_empty"),
                now,
            );
        }
        if self
            .state
            .next_buff_renewal_at()
            .is_some_and(|due| now >= due)
        {
            return self.emit(FieldDirective::hold("buff_renewal_required"), now);
        }
        if self.inputs.expire(now)? {
            return self.emit(FieldDirective::hold("input_lease_expired"), now);
        }
        Ok(self.last.clone())
    }
    pub fn observe(
        &mut self,
        observation: &FieldObservation,
        now: f64,
        focused: bool,
    ) -> Result<FieldDirective, ControlError> {
        if !self.clock_valid(now)? {
            return self.emit(FieldDirective::hold("clock_regressed"), now);
        }
        if self.stopped || self.paused || self.inputs.closed() || !focused {
            return self.emit(FieldDirective::hold("stopped_paused_or_unfocused"), now);
        }
        if self.empty_requires_exit() {
            return self.emit(
                FieldDirective::simple(FieldAction::Exit, "potions_empty"),
                now,
            );
        }
        let safety = &observation.safety;
        let stamp = safety.stamp();
        if !self.state.same_context(stamp) {
            return self.emit(FieldDirective::hold("wrong_context"), now);
        }
        if !(0.0..=self.limits.ages.screen_age()).contains(&(now - stamp.observed_at())) {
            return self.emit(FieldDirective::hold("screen_stale"), now);
        }
        if !stamp.newer_than(self.last_stamp.as_ref()) {
            return self.emit(FieldDirective::hold("screen_not_newer"), now);
        }
        self.last_stamp = Some(stamp.clone());
        let decision = self.state.decide(
            safety,
            now,
            self.limits.ages.screen_age(),
            self.limits.ages.belt_age(),
        )?;
        if let Some((_, _, sent_at)) = &self.potion_pending {
            return self.emit(
                FieldDirective::hold(
                    if now >= sent_at + self.limits.potion_confirmation_seconds {
                        "potion_confirmation_timeout"
                    } else {
                        "potion_outcome_pending"
                    },
                ),
                now,
            );
        }
        if decision.action == SurvivalAction::UsePotion {
            let key = decision
                .potion_key
                .ok_or_else(|| ControlError::Runtime("potion request is missing its key".into()))?;
            self.potion_pending = Some((stamp.clone(), key.clone(), now));
            return self.emit(
                FieldDirective::new(
                    FieldAction::UsePotion,
                    decision.reason,
                    Some(key),
                    None,
                    None,
                    false,
                )?,
                now,
            );
        }
        if !matches!(
            decision.action,
            SurvivalAction::Continue | SurvivalAction::Replenish
        ) {
            let action = match decision.action {
                SurvivalAction::Buff => FieldAction::Buff,
                SurvivalAction::Exit => FieldAction::Exit,
                _ => FieldAction::Hold,
            };
            return self.emit(FieldDirective::simple(action, decision.reason), now);
        }
        if safety.location() != Location::Field {
            return self.emit(FieldDirective::hold("field_required"), now);
        }
        if observation.belt_closed != Some(true) {
            return self.emit(FieldDirective::hold("belt_not_confirmed_closed"), now);
        }
        if let Some((policy, count)) = self.state.policy().zip(self.state.potions_remaining()) {
            if count <= policy.replenish_at_or_below() {
                self.replenishing = true;
            } else if count >= policy.potion_target() {
                self.replenishing = false;
            }
        }
        let mut deadline = stamp.observed_at() + self.limits.ages.screen_age();
        if let Some(due) = self.state.next_buff_renewal_at() {
            deadline = deadline.min(due);
        }
        if observation.enemy == Enemy::Unknown {
            return self.emit(FieldDirective::hold("enemy_unknown"), now);
        }
        if observation.enemy == Enemy::Near {
            let started = *self.combat_started.get_or_insert(now);
            deadline = deadline.min(started + self.limits.combat_seconds);
            if now >= deadline {
                return self.emit(FieldDirective::hold("combat_timeout"), now);
            }
            self.phase = FieldPhase::Combat;
            return self.emit(
                FieldDirective::new(
                    FieldAction::Combat,
                    "near_enemy",
                    None,
                    None,
                    Some(deadline),
                    self.replenishing,
                )?,
                now,
            );
        }
        if self.phase == FieldPhase::Combat {
            if observation.enemy != Enemy::Clear {
                return self.emit(FieldDirective::hold("combat_clear_unconfirmed"), now);
            }
            self.phase = FieldPhase::Loot;
            self.combat_started = None;
            return self.emit(
                FieldDirective::simple(FieldAction::InspectLoot, "combat_clear"),
                now,
            );
        }
        if self.phase == FieldPhase::Loot {
            if observation.enemy != Enemy::Clear || !observation.loot_checked {
                return self.emit(
                    FieldDirective::simple(FieldAction::InspectLoot, "loot_unconfirmed"),
                    now,
                );
            }
            self.phase = FieldPhase::Travel;
        }
        let point = if self.replenishing && observation.enemy == Enemy::Far {
            observation.hunt_aim
        } else {
            observation.travel_aim
        };
        let Some(point) = point.filter(|_| observation.reachable == Some(true)) else {
            let mut result = FieldDirective::hold("destination_unconfirmed");
            result.hunt_visible = self.replenishing;
            return self.emit(result, now);
        };
        if observation.arrived == Some(true) {
            self.cadence.confirm_arrival(stamp.observed_at());
        }
        if self
            .cadence
            .deadline()
            .is_some_and(|deadline| now >= deadline)
        {
            return self.emit(FieldDirective::hold("arrival_timeout"), now);
        }
        let aim = self.cadence.aim(point, now);
        if let Some(arrival) = self.cadence.deadline() {
            deadline = deadline.min(arrival);
        }
        self.emit(
            FieldDirective::new(
                FieldAction::Travel,
                if self.replenishing {
                    "hunt_visible"
                } else {
                    "route"
                },
                None,
                aim,
                Some(deadline),
                self.replenishing,
            )?,
            now,
        )
    }
}
