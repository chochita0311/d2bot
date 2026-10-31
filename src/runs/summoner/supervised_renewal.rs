//! 실제 시전 시간을 보존하는 같은 필드 재개와 감독 버프 갱신.
use super::encounter_execution::EncounterEvidence;
use crate::{
    common::{
        buffing::PauseSchedule, config::ConfigError, error::ControlError, field_preparation::*,
        survival::CharacterSurvivalPolicy,
    },
    runs::RunError,
};
use serde_json::{Value, json};
pub struct ReusableBuffs {
    pub buff: Value,
    pub deadlines: Vec<f64>,
}
pub fn reusable_buffs(
    report: &Value,
    policy: &CharacterSurvivalPolicy,
    wall_now: f64,
    monotonic_now: f64,
) -> Option<ReusableBuffs> {
    let proof = &report["supervised_effect"];
    let buff = &report["buff"];
    if !report["safe_exit"].is_null()
        || proof["character_id"] != "flash"
        || proof["source"] != "supervised_visual"
        || [
            "effect_verified",
            "equipment_restored",
            "sequence_effects_verified",
        ]
        .iter()
        .any(|key| proof[*key] != true)
        || buff["status"] != "dispatched_unconfirmed"
    {
        return None;
    }
    let mut deadlines = vec![];
    for rule in policy.buffs() {
        let requested = buff["buff_dispatches"]
            .as_array()?
            .iter()
            .rev()
            .find(|v| v["key"] == rule.key())?;
        let wall = requested["cast_request_at"]
            .as_f64()
            .filter(|v| v.is_finite())?;
        let mono = requested["cast_request_monotonic"]
            .as_f64()
            .filter(|v| v.is_finite())?;
        let observed = proof["observed_at"].as_f64().filter(|v| v.is_finite())?;
        let elapsed = wall_now - wall;
        let monotonic_elapsed = monotonic_now - mono;
        let duration = rule.duration_seconds() - rule.renew_before_seconds();
        if !(0.0..duration - 5.0).contains(&elapsed)
            || !(0.0..duration - 5.0).contains(&monotonic_elapsed)
            || (elapsed - monotonic_elapsed).abs() > 2.0
            || observed < wall
        {
            return None;
        }
        deadlines.push(mono + duration);
    }
    (!deadlines.is_empty()).then(|| ReusableBuffs {
        buff: buff.clone(),
        deadlines,
    })
}
pub fn validate_buff_site_permit(
    signal: &Value,
    policy: &CharacterSurvivalPolicy,
    now: f64,
) -> Result<(), ConfigError> {
    if signal["buff_safety_scope"] == "nearby" {
        if signal["source"] != "supervised_visual" || signal["nearby_monsters_clear"] != true {
            return Err(ConfigError::value(
                "fresh supervised nearby-clear confirmation required",
            ));
        }
        let mut copy = signal.clone();
        copy["monsters_clear"] = json!(true);
        validate_permit(&copy, "flash", policy, now, true)
    } else {
        validate_permit(signal, "flash", policy, now, true)
    }
}
pub fn buff_renewal_deadlines(
    dispatched: &BuffDispatchResult,
    policy: &CharacterSurvivalPolicy,
) -> Result<Vec<f64>, RunError> {
    let mut deadlines = vec![];
    for buff in policy.buffs() {
        let requested = dispatched
            .buff_dispatches
            .iter()
            .rev()
            .find(|item| item.key == buff.key())
            .ok_or_else(|| {
                RunError::Runtime(format!("timed buff request missing: {}", buff.name()))
            })?;
        deadlines.push(
            requested.cast_request_monotonic + buff.duration_seconds()
                - buff.renew_before_seconds(),
        );
    }
    if deadlines.is_empty() {
        return Err(RunError::Runtime("timed buff policy required".into()));
    }
    Ok(deadlines)
}
pub struct SupervisedBuffRequest<'a> {
    pub policy: &'a CharacterSurvivalPolicy,
    pub order: &'a [String],
    pub pauses: &'a PauseSchedule,
    pub prefix: &'a str,
}
struct Checks<'a> {
    guard: &'a mut BuffSiteGuard,
    policy: &'a CharacterSurvivalPolicy,
}
impl<E: BuffSiteEnvironment> PreparationChecks<E> for Checks<'_> {
    fn check_frame(
        &mut self,
        env: &mut E,
        frame: &std::sync::Arc<crate::common::frame::BgrFrame>,
        acquired: f64,
        hud: &crate::common::survival_vision::HudReading,
    ) -> Result<Option<String>, E::Error> {
        Ok(self
            .guard
            .wait_check(
                env,
                self.policy,
                Some(BuffSafetyFrame {
                    frame,
                    acquired: Some(acquired),
                    reading: Some(hud),
                }),
            )?
            .map(str::to_owned))
    }
}
fn accept_safety<E: PreparationEnvironment>(
    env: &E,
    signal: &Value,
    policy: &CharacterSurvivalPolicy,
) -> Result<(), ControlError> {
    validate_buff_site_permit(signal, policy, env.wall_clock())?;
    if signal["area"] != "arcane_sanctuary" {
        return Err(ConfigError::value("current Arcane area confirmation required").into());
    }
    Ok(())
}
pub fn renew_supervised_field_buffs<E: BuffSiteEnvironment<Error = RunError>>(
    env: &mut E,
    signals: &mut impl ProbeSignals<RunError>,
    request: SupervisedBuffRequest<'_>,
    record: &mut Value,
    evidence: &mut Vec<EncounterEvidence>,
    mut emit: impl FnMut(&str),
) -> Result<(Value, Vec<f64>), RunError> {
    emit(
        "AWAIT_BUFF_RENEWAL_SAFETY: fresh field_safety JSON; current route retained; no movement or weapon-panel input",
    );
    let options = ProbeWait {
        deadline: env.clock() + 300.0,
        phase: "field_safety",
        retry_unverified: true,
        retry_stale: true,
    };
    let permit = wait_probe_signal(
        env,
        signals,
        request.policy,
        options,
        |env, signal| accept_safety(env, signal, request.policy),
        &mut NoPreparationChecks,
        &mut emit,
    )?;
    record["safety"] = permit.clone();
    let mut guard = BuffSiteGuard::new(Some(permit.clone()));
    record["stability"] = guard
        .prepare(env, request.policy)
        .map_err(|error| match error {
            GuardPreparationError::NearbyHostile => RunError::Runtime("buff_nearby_hostile".into()),
            GuardPreparationError::Other(error) => error,
        })?;
    accept_safety(env, &permit, request.policy)?;
    let mut trace = vec![];
    let dispatched = dispatch_buffs(
        env,
        request.policy,
        request.order,
        request.pauses,
        &mut Checks {
            guard: &mut guard,
            policy: request.policy,
        },
        BuffDispatchOptions {
            trace_frames: Some(&mut trace),
            observation_wait: 2.0,
            sequence_timeout: 12.0,
            ..Default::default()
        },
    )?;
    record["buff"] = json!(&dispatched);
    record["resource_observations"] = json!(guard.observations());
    evidence.extend(trace.into_iter().map(|t| EncounterEvidence {
        filename: format!(
            "{}-step-{:02}.png",
            request.prefix,
            t.metadata["index"].as_u64().unwrap_or(0)
        ),
        frame: t.frame,
    }));
    if dispatched.status != "dispatched_unconfirmed" {
        return Err(RunError::Runtime(format!(
            "buff renewal dispatch: {}",
            dispatched.status
        )));
    }
    let last = dispatched
        .buff_dispatches
        .last()
        .ok_or_else(|| RunError::Runtime("buff request missing".into()))?
        .cast_request_at;
    emit(
        "AWAIT_BUFF_RENEWAL_EFFECT: fresh initial_effect JSON; dispatch alone does not confirm renewal",
    );
    let options = ProbeWait {
        deadline: env.clock() + 300.0,
        phase: "initial_effect",
        retry_unverified: true,
        retry_stale: true,
    };
    let proof = wait_probe_signal(
        env,
        signals,
        request.policy,
        options,
        |env, signal| {
            confirm_initial_probe(signal, "flash", last, env.wall_clock())?;
            accept_safety(env, signal, request.policy)?;
            let deadlines = buff_renewal_deadlines(&dispatched, request.policy)
                .map_err(|error| ControlError::Runtime(error.to_string()))?;
            if deadlines.iter().any(|at| env.clock() >= *at) {
                return Err(ControlError::Runtime(
                    "renewed buff already due before effect confirmation".into(),
                ));
            }
            Ok(())
        },
        &mut NoPreparationChecks,
        &mut emit,
    )?;
    record["effect"] = proof.clone();
    let deadlines = buff_renewal_deadlines(&dispatched, request.policy)?;
    record["deadlines"] = json!(deadlines);
    Ok((proof, deadlines))
}
