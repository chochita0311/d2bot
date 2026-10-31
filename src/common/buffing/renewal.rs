use super::{
    BuffAction, BuffConfirmation, BuffDirective, BuffEnvironment, BuffObservation, BuffPhase,
    BuffSequence, PauseSchedule,
};
use crate::common::config::ConfigError;
use crate::common::survival::{
    CharacterSurvivalState, Location, ObservationLimits, ObservationStamp, SurvivalAction,
};
use crate::common::time;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct BuffRenewalLimits {
    sequence_seconds: f64,
    confirmation_seconds: f64,
    safety_wait_seconds: f64,
}

impl BuffRenewalLimits {
    pub fn new(
        sequence_seconds: f64,
        confirmation_seconds: f64,
        safety_wait_seconds: f64,
    ) -> Result<Self, ConfigError> {
        time::positive(sequence_seconds, "sequence_seconds")?;
        time::positive(confirmation_seconds, "confirmation_seconds")?;
        time::positive(safety_wait_seconds, "safety_wait_seconds")?;
        Ok(Self {
            sequence_seconds,
            confirmation_seconds,
            safety_wait_seconds,
        })
    }
}

#[derive(Debug)]
pub struct AutomaticBuffRenewal {
    owner: Arc<()>,
    order: Vec<String>,
    pauses: PauseSchedule,
    limits: BuffRenewalLimits,
    ages: ObservationLimits,
    sequence: Option<BuffSequence>,
    review_reason: Option<String>,
    requested_at: Option<f64>,
    started_at: Option<f64>,
    confirmation_at: Option<f64>,
    next_step_at: f64,
    last_stamp: Option<ObservationStamp>,
    last_now: Option<f64>,
}

impl AutomaticBuffRenewal {
    pub fn new(
        state: &CharacterSurvivalState,
        order: Vec<String>,
        pauses: PauseSchedule,
        limits: BuffRenewalLimits,
        ages: ObservationLimits,
    ) -> Result<Self, ConfigError> {
        BuffSequence::new(state, &order)?;
        Ok(Self {
            owner: state.identity().clone(),
            order,
            pauses,
            limits,
            ages,
            sequence: None,
            review_reason: None,
            requested_at: None,
            started_at: None,
            confirmation_at: None,
            next_step_at: 0.0,
            last_stamp: None,
            last_now: None,
        })
    }
    pub fn pending(&self) -> bool {
        self.requested_at.is_some()
    }
    pub(crate) fn owns_state(&self, state: &CharacterSurvivalState) -> bool {
        Arc::ptr_eq(state.identity(), &self.owner)
    }
    pub fn ages(&self) -> ObservationLimits {
        self.ages
    }
    pub fn pause_warnings(&self) -> &[super::PauseWarning] {
        self.pauses.warnings()
    }
    pub fn review_reason(&self) -> Option<&str> {
        self.review_reason.as_deref()
    }
    pub fn sequence(&self) -> Option<&BuffSequence> {
        self.sequence.as_ref()
    }
    pub fn next_step_at(&self) -> f64 {
        self.next_step_at
    }
    pub fn cancel(&mut self, reason: impl Into<String>) -> BuffDirective {
        if let Some(sequence) = &mut self.sequence {
            sequence.cancel();
        }
        let reason = self.review_reason.get_or_insert_with(|| reason.into());
        BuffDirective::hold(reason.clone())
    }
    pub fn interrupt(&mut self, reason: impl Into<String>) {
        if self.pending() {
            self.cancel(reason);
        }
    }
    pub fn tick(
        &mut self,
        state: &CharacterSurvivalState,
        now: f64,
    ) -> Result<Option<BuffDirective>, ConfigError> {
        if !self.owns_state(state) {
            return Err(ConfigError::value(
                "buff consumer must use its owning survival state",
            ));
        }
        time::nonnegative(now, "now")?;
        if self.last_now.is_some_and(|last| now < last) {
            return Ok(Some(self.cancel("buff_clock_regressed")));
        }
        self.last_now = Some(now);
        if let Some(reason) = &self.review_reason {
            return Ok(Some(BuffDirective::hold(reason.clone())));
        }
        if !self.pending()
            && let Some(due) = state.next_buff_renewal_at().filter(|&due| now >= due)
        {
            self.requested_at = Some(due);
        }
        let Some(requested) = self.requested_at else {
            return Ok(None);
        };
        if self.last_stamp.as_ref().is_none_or(|stamp| {
            !(0.0..=self.ages.screen_age()).contains(&(now - stamp.observed_at()))
        }) {
            return Ok(Some(self.cancel("buff_observation_stale")));
        }
        match (self.started_at, self.confirmation_at) {
            (None, _) if now >= requested + self.limits.safety_wait_seconds => {
                return Ok(Some(self.cancel("buff_safety_timeout")));
            }
            (Some(started), None) if now >= started + self.limits.sequence_seconds => {
                return Ok(Some(self.cancel("buff_sequence_timeout")));
            }
            (_, Some(confirmed)) if now >= confirmed + self.limits.confirmation_seconds => {
                return Ok(Some(self.cancel("buff_confirmation_timeout")));
            }
            _ => {}
        }
        Ok(Some(BuffDirective::hold("buff_observation_required")))
    }
    pub fn observe<E: BuffEnvironment>(
        &mut self,
        state: &mut CharacterSurvivalState,
        scene: &BuffObservation,
        now: f64,
        environment: &mut E,
        confirmation: Option<&BuffConfirmation>,
    ) -> Result<Option<BuffDirective>, E::Error> {
        // Check the previous frame's watchdog before admitting the arriving frame.
        self.tick(state, now)?;
        if let Some(reason) = &self.review_reason {
            return Ok(Some(BuffDirective::hold(reason.clone())));
        }
        let safety = &scene.safety;
        let stamp = safety.stamp();
        let valid = state.same_context(stamp)
            && (0.0..=self.ages.screen_age()).contains(&(now - stamp.observed_at()))
            && stamp.newer_than(self.last_stamp.as_ref());
        if !valid {
            return Ok(if self.pending() {
                Some(self.cancel("buff_screen_invalid"))
            } else {
                None
            });
        }
        if self
            .sequence
            .as_ref()
            .and_then(BuffSequence::last_stamp)
            .is_some_and(|last| {
                stamp.sequence() <= last.sequence() || stamp.observed_at() < last.observed_at()
            })
        {
            return Ok(Some(BuffDirective::hold("buff_frame_precedes_dispatch")));
        }
        self.last_stamp = Some(stamp.clone());
        let decision = state.decide(safety, now, self.ages.screen_age(), self.ages.belt_age())?;
        if !self.pending() {
            if decision.action != SurvivalAction::Buff
                && !matches!(decision.reason, "buff_unsafe" | "buff_requires_field")
            {
                return Ok(None);
            }
            self.requested_at = Some(now);
        }
        if environment.interrupted()? {
            return Ok(Some(self.cancel("buff_interrupted")));
        }
        let preempt = match decision.action {
            SurvivalAction::UsePotion => Some(BuffAction::UsePotion),
            SurvivalAction::Exit => Some(BuffAction::Exit),
            _ => None,
        };
        if let Some(action) = preempt {
            self.cancel(format!("buff_preempted_{}", decision.reason));
            return Ok(Some(BuffDirective::new(action, decision.reason)));
        }
        let safe = safety.location() == Location::Field
            && safety.monsters_clear() == Some(true)
            && scene.belt_closed == Some(true)
            && matches!(
                decision.action,
                SurvivalAction::Buff | SurvivalAction::Continue | SurvivalAction::Replenish
            );
        if !safe {
            return Ok(Some(if self.started_at.is_some() {
                self.cancel("buff_safety_lost")
            } else {
                BuffDirective::hold("buff_waiting_for_safe_field")
            }));
        }
        if self.sequence.is_none() {
            self.sequence = Some(BuffSequence::new(state, &self.order)?);
            self.started_at = Some(now);
        }
        let Some(sequence) = &mut self.sequence else {
            return Err(ConfigError::value("buff sequence missing after initialization").into());
        };
        if sequence
            .guard(
                state,
                safety,
                now,
                self.ages.screen_age(),
                self.ages.belt_age(),
            )?
            .is_some()
        {
            return Ok(Some(self.cancel("buff_screen_precedes_dispatch")));
        }
        if now < self.next_step_at {
            return Ok(Some(BuffDirective::hold("buff_gap")));
        }
        if sequence.phase() == BuffPhase::AwaitEffect {
            let Some(confirmation) = confirmation else {
                return Ok(Some(BuffDirective::hold("buff_effect_unconfirmed")));
            };
            let names: Vec<_> = state
                .policy()
                .into_iter()
                .flat_map(|policy| policy.buffs())
                .filter(|buff| !sequence.confirmed.contains(buff.name()))
                .map(|buff| buff.name().to_owned())
                .collect();
            for name in names {
                if !sequence.confirm_effect(state, &name, confirmation, safety, now, self.ages)? {
                    return Ok(Some(self.cancel("buff_confirmation_rejected")));
                }
            }
            self.sequence = None;
            self.requested_at = None;
            self.started_at = None;
            self.confirmation_at = None;
            return Ok(Some(BuffDirective::new(
                BuffAction::Complete,
                "buff_confirmed",
            )));
        }
        let directive = sequence.next_step(
            state,
            safety,
            now,
            self.ages.screen_age(),
            self.ages.belt_age(),
        )?;
        if directive.action != BuffAction::InputRequest || environment.interrupted()? {
            return Ok(Some(self.cancel("buff_dispatch_blocked")));
        }
        let (Some(token), Some(index)) = (directive.token.as_deref(), directive.index) else {
            return Ok(Some(self.cancel("buff_dispatch_blocked")));
        };
        let dispatched = (|| {
            let succeeded = environment.send_token(token)?;
            let acknowledged = environment.next_stamp()?;
            Ok((succeeded, acknowledged))
        })();
        let (succeeded, acknowledged): (bool, ObservationStamp) = match dispatched {
            Ok(result) => result,
            Err(error) => {
                self.cancel("buff_input_error");
                return Err(error);
            }
        };
        if !(now..=environment.clock()).contains(&acknowledged.observed_at()) {
            return Ok(Some(self.cancel("buff_dispatch_stamp_invalid")));
        }
        if !sequence.acknowledge_step(state, index, &acknowledged, succeeded) {
            return Ok(Some(self.cancel("buff_dispatch_unconfirmed")));
        }
        self.next_step_at = acknowledged.observed_at() + self.pauses.seconds(token);
        if sequence.phase() == BuffPhase::AwaitEffect {
            self.confirmation_at = Some(acknowledged.observed_at());
        }
        Ok(Some(BuffDirective::hold("buff_sequence_running")))
    }
}
