//! Buff dispatch/effect transactions; the actor supplies its one mutable survival state.

mod execution;
mod renewal;
pub use execution::{
    BuffEnvironment, BuffExecutionEnvironment, BuffObservation, BuffSequenceExecutor,
    PauseSchedule, PauseWarning,
};
pub use renewal::{AutomaticBuffRenewal, BuffRenewalLimits};

use super::config::ConfigError;
use super::survival::{
    BeltContents, CharacterSurvivalPolicy, CharacterSurvivalState, Location, ObservationLimits,
    ObservationStamp, SafetyObservation, SurvivalAction,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuffAction {
    Hold,
    UsePotion,
    Exit,
    InputRequest,
    Complete,
    AwaitConfirmation,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BuffDirective {
    pub action: BuffAction,
    pub reason: String,
    pub index: Option<usize>,
    pub token: Option<String>,
}

impl BuffDirective {
    fn new(action: BuffAction, reason: impl Into<String>) -> Self {
        Self {
            action,
            reason: reason.into(),
            index: None,
            token: None,
        }
    }
    pub(crate) fn hold(reason: impl Into<String>) -> Self {
        Self::new(BuffAction::Hold, reason)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationSource {
    SupervisedVisual,
    ValidatedVision,
}

#[derive(Debug, Clone)]
pub struct BuffConfirmation {
    pub stamp: ObservationStamp,
    pub source: ConfirmationSource,
    pub effect_verified: bool,
    pub equipment_restored: bool,
    pub sequence_effects_verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuffPhase {
    Ready,
    Running,
    AwaitEffect,
    Complete,
    Cancelled,
}

#[derive(Debug)]
pub struct BuffSequence {
    owner: Arc<()>,
    policy: Arc<CharacterSurvivalPolicy>,
    character_id: String,
    room_id: String,
    order: Vec<String>,
    timed_indices: BTreeMap<usize, String>,
    phase: BuffPhase,
    index: usize,
    pending: Option<ObservationStamp>,
    last: Option<ObservationStamp>,
    casts: BTreeMap<String, ObservationStamp>,
    confirmed: BTreeSet<String>,
}

impl BuffSequence {
    pub fn new(state: &CharacterSurvivalState, order: &[String]) -> Result<Self, ConfigError> {
        let policy = state
            .shared_policy()
            .ok_or_else(|| ConfigError::value("Configured survival policy required"))?;
        if policy.buffs().is_empty() {
            return Err(ConfigError::value(
                "Timed buff rules required for this confirmation sequence",
            ));
        }
        if order.is_empty() || order.iter().any(|token| token.trim().is_empty()) {
            return Err(ConfigError::value("Explicit non-empty buff order required"));
        }
        let order: Vec<_> = order
            .iter()
            .map(|token| token.trim().to_lowercase())
            .collect();
        let mut timed_indices = BTreeMap::new();
        for buff in policy.buffs() {
            let index = order
                .iter()
                .rposition(|token| *token == buff.key().trim().to_lowercase())
                .ok_or_else(|| {
                    ConfigError::value("Timed buff key must occur in the configured sequence")
                })?;
            timed_indices.insert(index, buff.name().into());
        }
        if timed_indices.len() != policy.buffs().len() {
            return Err(ConfigError::value(
                "Timed effects require distinct identifiable sequence steps",
            ));
        }
        Ok(Self {
            owner: state.identity().clone(),
            policy,
            character_id: state.character_id().into(),
            room_id: state.room_id().into(),
            order,
            timed_indices,
            phase: BuffPhase::Ready,
            index: 0,
            pending: None,
            last: None,
            casts: BTreeMap::new(),
            confirmed: BTreeSet::new(),
        })
    }
    pub fn phase(&self) -> BuffPhase {
        self.phase
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn order(&self) -> &[String] {
        &self.order
    }
    pub fn last_stamp(&self) -> Option<&ObservationStamp> {
        self.last.as_ref()
    }
    fn accepts(&self, state: &CharacterSurvivalState, stamp: &ObservationStamp) -> bool {
        Arc::ptr_eq(state.identity(), &self.owner)
            && state.character_id() == self.character_id
            && state.room_id() == self.room_id
            && state.same_context(stamp)
            && stamp.newer_than(self.last.as_ref())
    }
    pub fn cancel(&mut self) {
        self.phase = BuffPhase::Cancelled;
        self.pending = None;
    }
    fn guard(
        &self,
        state: &CharacterSurvivalState,
        observation: &SafetyObservation,
        now: f64,
        screen_age: f64,
        belt_age: f64,
    ) -> Result<Option<BuffDirective>, ConfigError> {
        if !self.accepts(state, observation.stamp()) {
            return Ok(Some(BuffDirective::hold("wrong_or_old_context")));
        }
        let decision = state.decide(observation, now, screen_age, belt_age)?;
        let preempt = match decision.action {
            SurvivalAction::UsePotion => Some(BuffAction::UsePotion),
            SurvivalAction::Exit => Some(BuffAction::Exit),
            _ => None,
        };
        if let Some(action) = preempt {
            return Ok(Some(BuffDirective::new(action, decision.reason)));
        }
        if observation.location() != Location::Field || observation.monsters_clear() != Some(true) {
            return Ok(Some(BuffDirective::hold("field_safety_unverified")));
        }
        if self.phase == BuffPhase::Complete && decision.action == SurvivalAction::Buff {
            return Ok(Some(BuffDirective::hold("buff_renewal_required")));
        }
        if !matches!(
            decision.action,
            SurvivalAction::Buff | SurvivalAction::Continue | SurvivalAction::Replenish
        ) {
            return Ok(Some(BuffDirective::hold(decision.reason)));
        }
        Ok(None)
    }
    pub fn next_step(
        &mut self,
        state: &CharacterSurvivalState,
        observation: &SafetyObservation,
        now: f64,
        screen_age: f64,
        belt_age: f64,
    ) -> Result<BuffDirective, ConfigError> {
        if self.phase == BuffPhase::Cancelled {
            return Ok(BuffDirective::hold("sequence_cancelled"));
        }
        if let Some(blocked) = self.guard(state, observation, now, screen_age, belt_age)? {
            if self.phase != BuffPhase::Ready {
                self.cancel();
            }
            return Ok(blocked);
        }
        if self.pending.is_some() {
            return Ok(BuffDirective::hold("input_outcome_pending"));
        }
        if self.index == self.order.len() {
            return Ok(if self.phase == BuffPhase::Complete {
                BuffDirective::new(BuffAction::Complete, "confirmed")
            } else {
                BuffDirective::hold("effect_unconfirmed")
            });
        }
        self.phase = BuffPhase::Running;
        self.pending = Some(observation.stamp().clone());
        self.last = self.pending.clone();
        Ok(BuffDirective {
            action: BuffAction::InputRequest,
            reason: "next_step".into(),
            index: Some(self.index),
            token: Some(self.order[self.index].clone()),
        })
    }
    pub fn acknowledge_step(
        &mut self,
        state: &CharacterSurvivalState,
        index: usize,
        stamp: &ObservationStamp,
        succeeded: bool,
    ) -> bool {
        if self.phase != BuffPhase::Running
            || self.pending.is_none()
            || index != self.index
            || !self.accepts(state, stamp)
        {
            return false;
        }
        if !succeeded {
            self.cancel();
            return false;
        }
        if let Some(name) = self.timed_indices.get(&index)
            && let Some(pending) = &self.pending
        {
            // Start the effect's lifetime at the request, never the delayed acknowledgement.
            self.casts.insert(name.clone(), pending.clone());
        }
        self.last = Some(stamp.clone());
        self.pending = None;
        self.index += 1;
        if self.index == self.order.len() {
            self.phase = BuffPhase::AwaitEffect;
        }
        true
    }
    pub fn confirm_effect(
        &mut self,
        state: &mut CharacterSurvivalState,
        name: &str,
        evidence: &BuffConfirmation,
        observation: &SafetyObservation,
        now: f64,
        limits: ObservationLimits,
    ) -> Result<bool, ConfigError> {
        if self.phase != BuffPhase::AwaitEffect
            || self.confirmed.contains(name)
            || !self.casts.contains_key(name)
        {
            return Ok(false);
        }
        if evidence.stamp != *observation.stamp()
            || !evidence.effect_verified
            || !evidence.equipment_restored
            || !evidence.sequence_effects_verified
        {
            return Ok(false);
        }
        if self
            .guard(
                state,
                observation,
                now,
                limits.screen_age(),
                limits.belt_age(),
            )?
            .is_some()
        {
            self.cancel();
            return Ok(false);
        }
        let buff = self
            .policy
            .buffs()
            .iter()
            .find(|buff| buff.name() == name)
            .ok_or_else(|| ConfigError::value("Timed buff rule missing"))?;
        let cast = &self.casts[name];
        if now >= cast.observed_at() + buff.duration_seconds() - buff.renew_before_seconds() {
            self.cancel();
            return Ok(false);
        }
        if !state.confirm_buff(cast, name, Location::Field, Some(true)) {
            return Ok(false);
        }
        self.confirmed.insert(name.into());
        self.last = Some(observation.stamp().clone());
        if self.confirmed.len() == self.casts.len() {
            self.phase = BuffPhase::Complete;
        }
        Ok(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BeltVisibility {
    Unknown,
    Expanded,
    Closed,
}

#[derive(Debug)]
pub struct BeltInspection {
    owner: Arc<()>,
    visibility: BeltVisibility,
    last: Option<ObservationStamp>,
}

impl BeltInspection {
    pub fn new(state: &CharacterSurvivalState) -> Result<Self, ConfigError> {
        if state.policy().is_none() {
            return Err(ConfigError::value("Configured belt policy required"));
        }
        Ok(Self {
            owner: state.identity().clone(),
            visibility: BeltVisibility::Unknown,
            last: None,
        })
    }
    pub fn visibility(&self) -> BeltVisibility {
        self.visibility
    }
    pub fn request_toggle<'a>(
        &mut self,
        state: &'a CharacterSurvivalState,
    ) -> Result<&'a str, ConfigError> {
        if !Arc::ptr_eq(state.identity(), &self.owner) {
            return Err(ConfigError::value(
                "belt inspection must use its owning survival state",
            ));
        }
        self.visibility = BeltVisibility::Unknown;
        state
            .policy()
            .map(CharacterSurvivalPolicy::belt_toggle_key)
            .ok_or_else(|| ConfigError::value("Configured belt policy required"))
    }
    pub fn observe(
        &mut self,
        state: &mut CharacterSurvivalState,
        stamp: &ObservationStamp,
        visibility: BeltVisibility,
        contents: Option<&BeltContents>,
    ) -> bool {
        if !Arc::ptr_eq(state.identity(), &self.owner)
            || !state.same_context(stamp)
            || !stamp.newer_than(self.last.as_ref())
        {
            return false;
        }
        self.last = Some(stamp.clone());
        self.visibility = visibility;
        match visibility {
            BeltVisibility::Expanded => match contents {
                None => {
                    state.invalidate_belt(stamp);
                    false
                }
                Some(contents) => state.observe_belt(stamp, Some(contents)),
            },
            BeltVisibility::Closed => true,
            BeltVisibility::Unknown => false,
        }
    }
    pub fn permits_movement(
        &self,
        state: &CharacterSurvivalState,
        observation: &SafetyObservation,
        now: f64,
        screen_age: f64,
        belt_age: f64,
    ) -> Result<bool, ConfigError> {
        if !Arc::ptr_eq(state.identity(), &self.owner) {
            return Ok(false);
        }
        let Some(last) = self
            .last
            .as_ref()
            .filter(|_| self.visibility == BeltVisibility::Closed)
        else {
            return Ok(false);
        };
        if observation.stamp().sequence() < last.sequence()
            || observation.stamp().observed_at() < last.observed_at()
        {
            return Ok(false);
        }
        let age = now - last.observed_at();
        if !(0.0..=screen_age).contains(&age) {
            return Ok(false);
        }
        Ok(matches!(
            state.decide(observation, now, screen_age, belt_age)?.action,
            SurvivalAction::Continue | SurvivalAction::Replenish
        ))
    }
}
