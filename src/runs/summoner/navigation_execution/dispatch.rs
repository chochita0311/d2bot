use crate::common::{
    error::ControlError,
    field_control::{DesktopPoint, FieldInputBackend},
    time::MonotonicClock,
    workers::Cancellation,
};
use crate::runs::RunError;
use serde::Serialize;
/// 같은 단조 시계·취소·포커스와 취소 가능한 대기를 실행 주체가 제공한다.
pub trait StepEnvironment {
    fn clock(&self) -> f64;
    fn stopped(&self) -> bool;
    fn focused(&self) -> Result<bool, ControlError>;
    fn wait(&self, seconds: f64) -> Result<(), ControlError>;
}
pub struct StepGuards<'a> {
    pub clock: &'a dyn MonotonicClock,
    pub stop: &'a Cancellation,
    pub focused: &'a dyn Fn() -> Result<bool, ControlError>,
}
impl StepEnvironment for StepGuards<'_> {
    fn clock(&self) -> f64 {
        self.clock.now()
    }
    fn stopped(&self) -> bool {
        self.stop.is_cancelled()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        (self.focused)()
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        let duration = std::time::Duration::try_from_secs_f64(seconds)
            .map_err(|_| ControlError::Runtime("step wait exceeds runtime range".into()))?;
        self.stop.wait(duration).map(|_| ())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PointerPreparation {
    pub aim: DesktopPoint,
    pub requested_at: f64,
}
pub struct StepRequest<'a> {
    pub aim: DesktopPoint,
    pub key: &'a str,
    pub captured_at: f64,
    pub expires_at: Option<f64>,
}
#[derive(Default)]
pub struct DispatchOptions<'a> {
    pub verify_aim: Option<&'a mut dyn FnMut() -> Result<bool, RunError>>,
    pub preparation: Option<&'a mut Option<PointerPreparation>>,
}
#[derive(Debug, Default, Clone, Serialize)]
pub struct DispatchDiagnostics {
    pub stage: &'static str,
    pub reason: &'static str,
    pub checked_at: Option<f64>,
    pub frame_age_seconds: Option<f64>,
}
impl DispatchDiagnostics {
    fn record(&mut self, stage: &'static str, reason: &'static str) {
        self.stage = stage;
        self.reason = reason;
    }
    fn refuse(&mut self, stage: &'static str, reason: &'static str) {
        self.record(stage, reason);
        self.checked_at = None;
        self.frame_age_seconds = None;
    }
    fn allowed(
        &mut self,
        env: &impl StepEnvironment,
        step: &StepRequest<'_>,
        stage: &'static str,
    ) -> Result<bool, ControlError> {
        if env.stopped() {
            self.refuse(stage, "stop_requested");
            return Ok(false);
        }
        if !env.focused()? {
            self.refuse(stage, "focus_lost");
            return Ok(false);
        }
        let now = env.clock();
        let age = now - step.captured_at;
        let reason = if !(0.0..0.25).contains(&age) {
            if age < 0.0 {
                "frame_time_reversed"
            } else {
                "frame_expired"
            }
        } else if step
            .expires_at
            .is_some_and(|expires| now.partial_cmp(&expires) != Some(std::cmp::Ordering::Less))
        {
            "deadline_expired"
        } else {
            "allowed"
        };
        self.record(stage, reason);
        self.checked_at = Some(now);
        self.frame_age_seconds = Some(age);
        Ok(reason == "allowed")
    }
}
pub fn dispatch_planned_step(
    backend: &(impl FieldInputBackend + ?Sized),
    env: &impl StepEnvironment,
    step: StepRequest<'_>,
    options: DispatchOptions<'_>,
    diagnostics: &mut DispatchDiagnostics,
) -> Result<bool, RunError> {
    if let Some(preparation) = options.preparation {
        if !diagnostics.allowed(env, &step, "before_pointer")? {
            *preparation = None;
            return Ok(false);
        }
        let prepared = preparation.is_some_and(|state| state.aim == step.aim);
        if prepared && backend.pointer_matches(step.aim)? {
            if preparation.is_some_and(|state| step.captured_at < state.requested_at + 0.05) {
                diagnostics.record("pointer_preparation", "pointer_settling");
                return Ok(false);
            }
        } else {
            if !diagnostics.allowed(env, &step, "before_pointer_move")? {
                *preparation = None;
                return Ok(false);
            }
            backend.move_pointer(step.aim)?;
            *preparation = Some(PointerPreparation {
                aim: step.aim,
                requested_at: env.clock(),
            });
            diagnostics.record("pointer_preparation", "pointer_settling");
            return Ok(false);
        }
        if let Some(verify) = options.verify_aim
            && !verify()?
        {
            *preparation = None;
            diagnostics.refuse("pointer_verification", "pointer_mismatch");
            return Ok(false);
        }
        if !diagnostics.allowed(env, &step, "before_cast")? {
            *preparation = None;
            return Ok(false);
        }
        backend.press(step.key)?;
        *preparation = None;
        diagnostics.record("cast", "sent");
        return Ok(true);
    }
    if !diagnostics.allowed(env, &step, "before_pointer")? {
        return Ok(false);
    }
    backend.move_pointer(step.aim)?;
    env.wait(0.05)?;
    if !diagnostics.allowed(env, &step, "before_cast")? {
        return Ok(false);
    }
    if let Some(verify) = options.verify_aim {
        if !verify()? {
            diagnostics.refuse("pointer_verification", "pointer_mismatch");
            return Ok(false);
        }
        if !diagnostics.allowed(env, &step, "after_pointer_verification")? {
            return Ok(false);
        }
    }
    backend.press(step.key)?;
    diagnostics.record("cast", "sent");
    Ok(true)
}
