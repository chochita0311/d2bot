use super::{BuffAction, BuffDirective, BuffSequence};
use crate::common::config::ConfigError;
use crate::common::survival::{
    CharacterSurvivalState, ObservationLimits, ObservationStamp, SafetyObservation,
};
use crate::common::time;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct BuffObservation {
    pub safety: SafetyObservation,
    pub belt_closed: Option<bool>,
}

/// The owning runtime supplies one monotonic origin and cancellable input dispatch.
/// Implementations must not hold an actor lock while blocking on the backend.
pub trait BuffEnvironment {
    type Error: From<ConfigError>;
    fn clock(&self) -> f64;
    fn interrupted(&self) -> Result<bool, Self::Error>;
    fn send_token(&mut self, token: &str) -> Result<bool, Self::Error>;
    fn next_stamp(&mut self) -> Result<ObservationStamp, Self::Error>;
}

pub trait BuffExecutionEnvironment: BuffEnvironment {
    fn observe(&mut self) -> Result<BuffObservation, Self::Error>;
    /// Returns true if cancelled during the wait.
    fn wait(&mut self, seconds: f64) -> Result<bool, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseWarning {
    pub token: String,
    pub original: String,
    pub message: &'static str,
}

#[derive(Debug, Clone)]
pub struct PauseSchedule {
    pauses: BTreeMap<String, f64>,
    default_pause: f64,
    warnings: Vec<PauseWarning>,
}

impl PauseSchedule {
    pub fn new(pauses: BTreeMap<String, f64>, default_pause: f64) -> Result<Self, ConfigError> {
        time::nonnegative(default_pause, "default buff pause")?;
        Duration::try_from_secs_f64(default_pause).map_err(|_| {
            ConfigError::value("default buff pause cannot be represented as a runtime duration")
        })?;
        let mut accepted = BTreeMap::new();
        let mut warnings = Vec::new();
        for (token, seconds) in pauses {
            if Duration::try_from_secs_f64(seconds).is_ok() {
                accepted.insert(token, seconds);
            } else {
                warnings.push(PauseWarning { token, original: seconds.to_string(), message: "Invalid or unrepresentable optional buff pause; using this consumer's existing default" });
            }
        }
        Ok(Self {
            pauses: accepted,
            default_pause,
            warnings,
        })
    }
    pub fn seconds(&self, token: &str) -> f64 {
        self.pauses
            .get(token)
            .copied()
            .unwrap_or(self.default_pause)
    }
    pub fn warnings(&self) -> &[PauseWarning] {
        &self.warnings
    }
}

#[derive(Debug)]
pub struct BuffSequenceExecutor {
    sequence: BuffSequence,
    pauses: PauseSchedule,
    limits: ObservationLimits,
    timeout: f64,
}

impl BuffSequenceExecutor {
    pub fn new(
        sequence: BuffSequence,
        pauses: PauseSchedule,
        limits: ObservationLimits,
        timeout: f64,
    ) -> Result<Self, ConfigError> {
        time::positive(timeout, "buff timeout")?;
        Ok(Self {
            sequence,
            pauses,
            limits,
            timeout,
        })
    }
    pub fn sequence(&self) -> &BuffSequence {
        &self.sequence
    }
    pub fn pause_warnings(&self) -> &[PauseWarning] {
        self.pauses.warnings()
    }
    pub fn run<E: BuffExecutionEnvironment>(
        &mut self,
        state: &CharacterSurvivalState,
        environment: &mut E,
    ) -> Result<BuffDirective, E::Error> {
        let began = environment.clock();
        for _ in 0..self.sequence.order().len() {
            if environment.interrupted()? || environment.clock() - began >= self.timeout {
                self.sequence.cancel();
                return Ok(BuffDirective::hold("sequence_interrupted"));
            }
            let scene = environment.observe()?;
            let now = environment.clock();
            if scene.belt_closed != Some(true)
                || environment.interrupted()?
                || now - began >= self.timeout
            {
                self.sequence.cancel();
                return Ok(BuffDirective::hold("belt_or_execution_unverified"));
            }
            let directive = self.sequence.next_step(
                state,
                &scene.safety,
                now,
                self.limits.screen_age(),
                self.limits.belt_age(),
            )?;
            if directive.action != BuffAction::InputRequest {
                return Ok(directive);
            }
            let (Some(token), Some(index)) = (directive.token.as_deref(), directive.index) else {
                self.sequence.cancel();
                return Err(
                    ConfigError::value("input request is missing its token or index").into(),
                );
            };
            let dispatched = (|| {
                let succeeded = environment.send_token(token)?;
                let stamp = environment.next_stamp()?;
                Ok(self
                    .sequence
                    .acknowledge_step(state, index, &stamp, succeeded))
            })();
            match dispatched {
                Err(error) => {
                    self.sequence.cancel();
                    return Err(error);
                }
                Ok(false) => {
                    self.sequence.cancel();
                    return Ok(BuffDirective::hold("dispatch_unconfirmed"));
                }
                Ok(true) => {}
            }
            let remaining = self.timeout - (environment.clock() - began);
            if remaining <= 0.0
                || environment.wait(self.pauses.seconds(token).min(remaining))?
                || environment.interrupted()?
                || environment.clock() - began >= self.timeout
            {
                self.sequence.cancel();
                return Ok(BuffDirective::hold("sequence_interrupted"));
            }
        }
        Ok(BuffDirective::new(
            BuffAction::AwaitConfirmation,
            "sequence_dispatched",
        ))
    }
}
