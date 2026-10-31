use super::{emit, hold, native, native::NativeProbe, options::Options, wait_signal};
use crate::tools::{ToolError, invalid};
use crate::{
    common::{
        buffing::PauseSchedule,
        field_preparation::{
            self, BuffDispatchOptions, BuffTrace, NoPreparationChecks, PreparationEnvironment,
        },
        survival::CharacterSurvivalPolicy,
        survival_vision::resource_guard,
    },
    platform::windows::StdinLines,
    runs::summoner::{
        camera_ground::GroundObserver,
        navigation::ArcanePathObserver,
        navigation_execution::{
            self, DispatchDiagnostics, DispatchOptions, LandingRequest, PlanOptions, StepRequest,
        },
    },
};
use serde_json::{Value, json};
use std::io::Write;
pub(super) struct BuffRun<'a> {
    pub options: &'a Options,
    pub policy: &'a CharacterSurvivalPolicy,
    pub order: &'a [String],
    pub pauses: &'a PauseSchedule,
    pub movement: &'a str,
    pub planner: &'a mut Option<(Box<dyn GroundObserver>, ArcanePathObserver)>,
    pub reference: Option<f64>,
    pub trace: &'a mut Option<Vec<BuffTrace>>,
    pub renewal_trace: &'a mut Option<Vec<BuffTrace>>,
    pub signals: &'a mut Option<StdinLines>,
}
pub(super) fn run_buffs(
    env: &mut NativeProbe,
    request: BuffRun<'_>,
    output: &mut impl Write,
    report: &mut Value,
) -> Result<(), ToolError> {
    let BuffRun {
        options,
        policy,
        order,
        pauses,
        movement,
        planner,
        reference,
        trace,
        renewal_trace,
        signals,
    } = request;
    let clock = env.clock.clone();
    let dispatched = field_preparation::dispatch_buffs(
        env,
        policy,
        order,
        pauses,
        &mut NoPreparationChecks,
        BuffDispatchOptions {
            trace_frames: trace.as_mut(),
            ..Default::default()
        },
    )?;
    *report = serde_json::to_value(&dispatched)?;
    if options.step_after_buff.is_some() && dispatched.status == "dispatched_unconfirmed" {
        *signals = Some(StdinLines::start(2)?);
        emit(
            output,
            "AWAIT_STEP_EFFECT: fresh supervised_visual effect/equipment and field safety required",
        )?;
        let last = dispatched
            .buff_dispatches
            .last()
            .ok_or_else(|| invalid("buff request missing"))?
            .cast_request_at;
        let deadline = clock.now() + 60.0;
        let proof = wait_signal(
            env,
            signals
                .as_ref()
                .ok_or_else(|| invalid("signals required"))?,
            policy,
            deadline,
            "initial_effect",
            |e, s| {
                field_preparation::confirm_initial_probe(
                    s,
                    &options.character,
                    last,
                    PreparationEnvironment::wall_clock(e),
                )
                .map_err(Into::into)
            },
        )?;
        field_preparation::validate_permit(
            &proof,
            &options.character,
            policy,
            native::wall_now(),
            true,
        )?;
        report["initial_supervised_confirmation"] = proof.clone();
        let acquired = clock.now();
        let frame = env.grab()?;
        let (ground, path) = planner
            .as_mut()
            .ok_or_else(|| invalid("step planner required"))?;
        let wing = serde_json::from_value(json!(options.step_after_buff))?;
        let plan = navigation_execution::plan_arcane_step(
            &frame,
            wing,
            ground.as_mut(),
            path,
            acquired,
            PlanOptions::default(),
            clock.as_ref(),
        )?;
        if let Some(blocked) = resource_guard(&env.hud.observe(frame.view())?, policy) {
            return Err(invalid(blocked));
        }
        field_preparation::validate_permit(
            &proof,
            &options.character,
            policy,
            native::wall_now(),
            true,
        )?;
        let aim = env.to_desktop(
            plan.choice
                .aim
                .ok_or_else(|| invalid("planned aim required"))?
                .map(f64::from),
        )?;
        report["path_choice"] = json!(plan.choice);
        report["player_ground"] = json!(plan.ground);
        report["planned_frame_at"] = json!(acquired);
        let input = env.input.clone();
        report["navigation"] = if navigation_execution::dispatch_planned_step(
            input.as_ref(),
            env,
            StepRequest {
                aim,
                key: movement,
                captured_at: acquired,
                expires_at: None,
            },
            DispatchOptions::default(),
            &mut DispatchDiagnostics::default(),
        )? {
            json!(navigation_execution::observe_step_landing(
                env,
                ground.as_mut(),
                path,
                LandingRequest {
                    frame: &frame,
                    view: &plan.view,
                    choice: &plan.choice,
                    policy,
                    reference_seconds: reference
                        .ok_or_else(|| invalid("cast reference required"))?,
                    expires_at: None
                }
            )?)
        } else {
            json!({"status":"dispatch_blocked"})
        };
        report["status"] = json!(if report["navigation"]["status"] == "arrival_observed" {
            "step_arrival_observed"
        } else {
            "step_failed"
        });
    }
    if options.renew_once && report["status"] == "dispatched_unconfirmed" {
        if policy.buffs().len() != 1 {
            return Err(invalid("timer probe requires one explicit timed buff rule"));
        }
        let rule = &policy.buffs()[0];
        let cast = dispatched
            .buff_dispatches
            .iter()
            .rev()
            .find(|r| r.key == rule.key())
            .ok_or_else(|| invalid("timed buff cast missing"))?;
        let due =
            cast.cast_request_monotonic + rule.duration_seconds() - rule.renew_before_seconds();
        *signals = Some(StdinLines::start(2)?);
        emit(
            output,
            "AWAIT_INITIAL_EFFECT: fresh supervised_visual confirmation required; no renewal armed",
        )?;
        let last = dispatched
            .buff_dispatches
            .last()
            .ok_or_else(|| invalid("buff request missing"))?
            .cast_request_at;
        let deadline = due.min(clock.now() + 60.0);
        let proof = wait_signal(
            env,
            signals
                .as_ref()
                .ok_or_else(|| invalid("signals required"))?,
            policy,
            deadline,
            "initial_effect",
            |e, s| {
                field_preparation::confirm_initial_probe(
                    s,
                    &options.character,
                    last,
                    PreparationEnvironment::wall_clock(e),
                )
                .map_err(Into::into)
            },
        )?;
        report["initial_supervised_confirmation"] = proof;
        report["renewal_due_monotonic"] = json!(due);
        hold::wait_renewal_deadline(env, policy, due, |remaining| {
            writeln!(output, "RENEWAL_TIMER: {remaining:.3} seconds remaining")?;
            output.flush()?;
            Ok(())
        })?;
        emit(
            output,
            "AWAIT_RENEWAL_SAFETY: prepared executor requires one fresh permit within 10 seconds",
        )?;
        let renewal = wait_signal(
            env,
            signals
                .as_ref()
                .ok_or_else(|| invalid("signals required"))?,
            policy,
            due + 10.0,
            "renewal_safety",
            |e, s| {
                field_preparation::validate_permit(
                    s,
                    &options.character,
                    policy,
                    PreparationEnvironment::wall_clock(e),
                    true,
                )
                .map_err(Into::into)
            },
        )?;
        field_preparation::validate_permit(
            &renewal,
            &options.character,
            policy,
            native::wall_now(),
            true,
        )?;
        let renewed = field_preparation::dispatch_buffs(
            env,
            policy,
            order,
            pauses,
            &mut NoPreparationChecks,
            BuffDispatchOptions {
                trace_frames: renewal_trace.as_mut(),
                ..Default::default()
            },
        )?;
        report["status"] = json!(if renewed.status == "dispatched_unconfirmed" {
            "renewed_unconfirmed"
        } else {
            "renewal_failed"
        });
        report["renewal"] = json!(renewed);
    }

    Ok(())
}
