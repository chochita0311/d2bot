use crate::{
    common::{
        error::ControlError,
        field_control::{
            DesktopPoint, FieldAction, FieldDirective, FieldInputBackend, HeldFieldInput,
        },
        field_preparation::{NoPreparationChecks, PreparationEnvironment, wait_readable_resources},
        survival::CharacterSurvivalPolicy,
        survival_vision::resource_guard,
        time::MonotonicClock,
        workers::{Cancellation, WorkerGroup},
    },
    runs::RunError,
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
pub struct HoldRequest {
    pub action: FieldAction,
    pub seconds: f64,
    pub aim: DesktopPoint,
    pub buff_deadline: f64,
}
pub struct HoldControl<B: FieldInputBackend> {
    pub inputs: HeldFieldInput<B>,
    pub clock: Arc<dyn MonotonicClock>,
    pub stop: Cancellation,
    pub focused: Arc<dyn Fn() -> Result<bool, ControlError> + Send + Sync>,
}
fn finish(stop: &Cancellation, reason: &Mutex<Option<String>>, value: &str) {
    let mut reason = match reason.lock() {
        Ok(v) => v,
        Err(p) => p.into_inner(),
    };
    if reason.is_none() {
        *reason = Some(value.into());
    }
    stop.cancel();
}
struct WatchLease<B: FieldInputBackend> {
    inputs: HeldFieldInput<B>,
    stop: Cancellation,
}
impl<B: FieldInputBackend> Drop for WatchLease<B> {
    fn drop(&mut self) {
        self.stop.cancel();
        crate::common::error::best_effort_cleanup(|| self.inputs.close());
    }
}
pub fn hold_probe<E: PreparationEnvironment<Error = RunError>, B: FieldInputBackend + 'static>(
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    control: HoldControl<B>,
    request: HoldRequest,
) -> Result<Value, RunError> {
    let started = env.clock();
    let deadline = (started + request.seconds).min(request.buff_deadline);
    let reason = Arc::new(Mutex::new(None));
    let worker_reason = reason.clone();
    let mut worker = WorkerGroup::new(Cancellation::default(), 1);
    let inputs = control.inputs.clone();
    let clock = control.clock.clone();
    let stop = control.stop.clone();
    let focused = control.focused.clone();
    worker.spawn("d2-probe-watchdog", move |local| {
        let _lease = WatchLease {
            inputs: inputs.clone(),
            stop: stop.clone(),
        };
        let result = (|| {
            while !local.wait(Duration::from_millis(5))? && !stop.is_cancelled() {
                let now = clock.now();
                if !focused()? {
                    finish(&stop, &worker_reason, "focus_lost");
                } else if now >= deadline {
                    finish(
                        &stop,
                        &worker_reason,
                        if request.buff_deadline <= started + request.seconds {
                            "buff_due"
                        } else {
                            "duration_complete"
                        },
                    );
                } else {
                    inputs.expire(now)?;
                }
            }
            inputs.close()
        })();
        if result.is_err() {
            finish(&stop, &worker_reason, "input_error");
        }
        result
    })?;
    let mut observations = vec![];
    let mut errors = vec![];
    let observed = (|| -> Result<(), RunError> {
        while !control.stop.is_cancelled() {
            let acquired = env.clock();
            let frame = env.capture()?;
            let reading = env.hud(&frame)?;
            let now = env.clock();
            observations.push(json!({"elapsed":now-started,"life":[reading.life.current(),reading.life.maximum()],"mana":[reading.mana.current(),reading.mana.maximum()],"belt":reading.belt_visibility}));
            if let Some(blocked) = resource_guard(&reading, policy) {
                finish(&control.stop, &reason, blocked);
            } else if !(control.focused)()? {
                finish(&control.stop, &reason, "focus_lost");
            } else if now - acquired >= 0.25 {
                finish(&control.stop, &reason, "screen_stale");
            } else if now < deadline && !control.stop.is_cancelled() {
                control.inputs.apply(
                    &FieldDirective::new(
                        request.action,
                        "supervised_probe",
                        None,
                        Some(request.aim),
                        Some(deadline.min(acquired + 0.25)),
                        false,
                    )?,
                    now,
                )?;
            }
            control.stop.wait(Duration::from_millis(20))?;
        }
        Ok(())
    })();
    if let Err(error) = observed {
        errors.push(error.to_string());
        finish(&control.stop, &reason, "input_error");
    }
    finish(&control.stop, &reason, "interrupted");
    if let Err(error) = control.inputs.close() {
        errors.push(error.to_string());
    }
    // timeout은 성공이 아니다. 실제 종료 전에는 소유자를 버리거나 다음 probe를 시작하지 않는다.
    while !worker.is_finished() {
        if let Err(error) = worker.finish(Duration::from_secs(1))
            && !errors.contains(&error.to_string())
        {
            errors.push(error.to_string());
        }
    }
    if let Err(error) = worker.finish(Duration::ZERO) {
        errors.push(error.to_string());
    }
    errors.extend(worker.faults()?.into_iter().map(|f| f.message));
    let status = reason
        .lock()
        .map_err(|_| RunError::Runtime("probe result poisoned".into()))?
        .clone()
        .unwrap_or_else(|| "interrupted".into());
    let held = control.inputs.snapshot()?.held;
    Ok(json!({"status":status,"held_after":held,"errors":errors,"observations":observations}))
}
pub fn wait_renewal_deadline<E: PreparationEnvironment<Error = RunError>>(
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    due: f64,
    mut progress: impl FnMut(f64) -> Result<(), RunError>,
) -> Result<(), RunError> {
    let mut last_progress = None;
    while !env.stopped() && env.clock() < due {
        let deadline = due.min(env.clock() + 0.75);
        if let Some(blocked) =
            wait_readable_resources(env, policy, deadline, &mut NoPreparationChecks, &[])?
        {
            if blocked == "observation_unverified" && env.clock() >= due {
                break;
            }
            return Err(RunError::Runtime(format!("renewal monitor: {blocked}")));
        }
        let remaining = (due - env.clock()).max(0.0);
        let bucket = (remaining / 10.0).floor() as i64;
        if last_progress != Some(bucket) {
            progress(remaining)?;
            last_progress = Some(bucket);
        }
        env.wait(remaining.min(0.02))?;
    }
    if env.stopped() {
        return Err(RunError::Runtime("renewal monitor interrupted".into()));
    }
    Ok(())
}
