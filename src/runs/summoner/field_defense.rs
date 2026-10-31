//! 버프 준비 구역의 한정 공격과 일시정지 중 현재 HUD·근접 방어.
use super::{
    continuation::{NEAR_GROUND_RADIUS, near_ground},
    continuous_navigation::ContinuousVision,
    encounter_execution::{EncounterEnvironment, EncounterEvidence},
    scene_vision::GroundReading,
};
use crate::{
    common::{
        encounters::VisualCandidate,
        error::ControlError,
        field_control::{FieldAction, FieldDirective, FieldInputBackend, FieldInputSession},
        frame::BgrFrame,
        survival::CharacterSurvivalPolicy,
        survival_vision::{HudReading, resource_guard},
    },
    runs::RunError,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
#[cfg(test)]
mod tests;

pub trait DefenseVision {
    fn prepares_ground(&self) -> bool;
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        hud: &HudReading,
    ) -> Result<(), RunError>;
    fn ground(&mut self, frame: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError>;
    fn hazards(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
    ) -> Result<Vec<VisualCandidate>, RunError>;
}
impl<T: ContinuousVision> DefenseVision for T {
    fn prepares_ground(&self) -> bool {
        ContinuousVision::prepares_ground(self)
    }
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        hud: &HudReading,
    ) -> Result<(), RunError> {
        ContinuousVision::prepare(self, frame, acquired, hud, true)
    }
    fn ground(&mut self, frame: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError> {
        ContinuousVision::ground(self, frame, acquired)
    }
    fn hazards(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        ContinuousVision::hazards(self, frame, player, acquired, None)
    }
}
pub trait PausedEnvironment: EncounterEnvironment {
    fn defense_probe_points(
        &mut self,
        _frame: &Arc<BgrFrame>,
        _player: [i32; 2],
    ) -> Result<Option<Vec<[i32; 2]>>, RunError> {
        Ok(None)
    }
    fn encounter_recheck(
        &mut self,
        _frame: &Arc<BgrFrame>,
        _acquired: f64,
    ) -> Result<bool, RunError> {
        Ok(false)
    }
    fn has_renewal(&self) -> bool {
        false
    }
    fn renew_buffs(
        &mut self,
        _report: &mut Value,
        _evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<f64, RunError> {
        Err(RunError::Runtime(
            "buff renewal callback unavailable".into(),
        ))
    }
}
pub struct PausedRequest<'a> {
    pub policy: &'a CharacterSurvivalPolicy,
    pub buff_expires_at: Option<f64>,
    pub engage_monsters: bool,
}
#[derive(Serialize)]
pub struct BuffSiteCombatReport {
    pub status: &'static str,
    pub frames: u64,
    pub attack_dispatches: u64,
}
fn merge(primary: Result<(), RunError>, cleanup: Result<(), RunError>) -> Result<(), RunError> {
    match (primary, cleanup) {
        (Err(p), Err(c)) => Err(p.with_cleanup(c)),
        (Err(e), _) | (_, Err(e)) => Err(e),
        _ => Ok(()),
    }
}
fn combat<B: FieldInputBackend>(
    inputs: &FieldInputSession<B>,
    reason: &str,
    deadline: f64,
    now: f64,
) -> Result<(), RunError> {
    Ok(inputs.inputs().apply(
        &FieldDirective::new(
            FieldAction::Combat,
            reason,
            None,
            None,
            Some(deadline),
            false,
        )?,
        now,
    )?)
}
fn faults<B: FieldInputBackend>(inputs: &FieldInputSession<B>, name: &str) -> Result<(), RunError> {
    if let Some(fault) = inputs.faults()?.first() {
        return Err(RunError::Runtime(format!("{name}: {}", fault.message)));
    }
    Ok(())
}
pub fn clear_buff_site<B: FieldInputBackend, E: EncounterEnvironment>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    evidence: &mut Vec<EncounterEvidence>,
) -> Result<BuffSiteCombatReport, RunError> {
    clear(inputs, env, policy, evidence, |inputs, deadline| {
        inputs.start_expiry_only("summoner-buff-site-watchdog", deadline)
    })
}
fn clear<B: FieldInputBackend, E: EncounterEnvironment>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    policy: &CharacterSurvivalPolicy,
    evidence: &mut Vec<EncounterEvidence>,
    start: impl FnOnce(&mut FieldInputSession<B>, f64) -> Result<(), ControlError>,
) -> Result<BuffSiteCombatReport, RunError> {
    let started = inputs.clock().now();
    let mut frames = 0;
    let mut last = None;
    let mut unavailable_since = None;
    let result = (|| {
        start(inputs, started + 6.0)?;
        while inputs.clock().now() - started < 6.0 {
            if inputs.stop().is_set() || !inputs.focused()? {
                return Err(RunError::Runtime(
                    "buff-site combat interrupted or unfocused".into(),
                ));
            }
            let acquired = inputs.clock().now();
            let frame = env.capture()?;
            last = Some(frame.clone());
            let reading = env.hud(&frame)?;
            if let Some(blocked) = resource_guard(&reading, policy) {
                inputs.inputs().release(false)?;
                if matches!(blocked, "resource_low" | "mercenary_unverified") {
                    return Err(RunError::Runtime(blocked.into()));
                }
                let since = *unavailable_since.get_or_insert(acquired);
                if inputs.clock().now() - since >= 1.5 {
                    return Err(RunError::Runtime(blocked.into()));
                }
                env.wait(0.02)?;
                continue;
            }
            unavailable_since = None;
            if inputs.clock().now() - acquired >= 0.25 {
                inputs.inputs().release(false)?;
                continue;
            }
            if frames == 0 {
                evidence.push(EncounterEvidence {
                    filename: "buff-site-combat-before.png".into(),
                    frame: frame.clone(),
                });
            }
            combat(
                inputs,
                "supervised_hostile_before_buff",
                acquired + 0.25,
                inputs.clock().now(),
            )?;
            frames += 1;
            env.wait(0.02)?;
        }
        Ok(())
    })();
    let cleanup = inputs
        .finish(Duration::from_secs(1))
        .map_err(RunError::from);
    if let Some(frame) = last {
        evidence.push(EncounterEvidence {
            filename: "buff-site-combat-after.png".into(),
            frame,
        });
    }
    merge(result, cleanup)?;
    faults(inputs, "buff-site watchdog")?;
    let snapshot = inputs.inputs().snapshot()?;
    Ok(BuffSiteCombatReport {
        status: "attack_finished_clear_unconfirmed",
        frames,
        attack_dispatches: snapshot
            .dispatches
            .get(&inputs.inputs().keys()[1])
            .copied()
            .unwrap_or(0),
    })
}
pub fn maintain_paused_field<B: FieldInputBackend, E: PausedEnvironment>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    vision: Option<&mut dyn DefenseVision>,
    request: PausedRequest<'_>,
    report: &mut Value,
    evidence: &mut Vec<EncounterEvidence>,
) -> Result<(), RunError> {
    paused(
        inputs,
        env,
        vision,
        request,
        report,
        evidence,
        |inputs, deadline| inputs.start("summoner-paused-defense", deadline),
    )
}
fn paused<B: FieldInputBackend, E: PausedEnvironment>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    mut vision: Option<&mut dyn DefenseVision>,
    request: PausedRequest<'_>,
    report: &mut Value,
    evidence: &mut Vec<EncounterEvidence>,
    start: impl FnOnce(&mut FieldInputSession<B>, f64) -> Result<(), ControlError>,
) -> Result<(), RunError> {
    let enabled = vision.is_some() && request.buff_expires_at.is_some() && request.engage_monsters;
    let mut guard = json!({"status":"watching","observations":0,"started_at":inputs.clock().now(),"nearby_defense_enabled":enabled});
    let mut missing_since = None;
    let mut last = None;
    let mut attack_started = None;
    let mut attack_baseline = 0;
    let mut buff = request.buff_expires_at;
    let mut focus_handoff = false;
    let result = (|| {
        if enabled {
            start(
                inputs,
                buff.ok_or_else(|| RunError::Runtime("paused defense deadline missing".into()))?,
            )?;
        }
        while !inputs.stop().is_set() {
            if !inputs.focused()? {
                guard["status"] = json!("focus_handoff");
                focus_handoff = true;
                return Ok(());
            }
            if env.has_renewal() && buff.is_some_and(|at| inputs.clock().now() >= at) {
                if enabled {
                    inputs.inputs().release(false)?;
                }
                let renewed = env.renew_buffs(report, evidence)?;
                buff = Some(renewed);
                if enabled {
                    inputs.set_deadline(renewed)?;
                }
                attack_started = None;
                continue;
            }
            let acquired = inputs.clock().now();
            let frame = env.capture()?;
            last = Some(frame.clone());
            let reading = env.hud(&frame)?;
            let mut blocked = resource_guard(&reading, request.policy);
            if inputs.clock().now() - acquired >= 0.25 {
                blocked = Some("observation_unverified");
            }
            guard["observations"] = json!(guard["observations"].as_u64().unwrap_or(0) + 1);
            guard["last_observed_at"] = json!(acquired);
            guard["life_ratio"] = json!(reading.life.ratio());
            guard["mana_ratio"] = json!(reading.mana.ratio());
            guard["blocked"] = json!(blocked);
            if blocked == Some("mercenary_unverified") {
                if enabled {
                    inputs.inputs().release(false)?;
                }
                guard["status"] = json!("mercenary_unverified");
                return Err(RunError::Runtime("mercenary_unverified".into()));
            }
            if blocked == Some("resource_low") {
                if enabled {
                    inputs.inputs().release(false)?;
                    if env.has_probe() {
                        env.reset_probe();
                    }
                }
                if env.recover_resources(&frame, &reading, acquired)? {
                    attack_started = None;
                    missing_since = None;
                    continue;
                }
                guard["status"] = json!("resource_low");
                return Err(RunError::Runtime("resource_low".into()));
            }
            if blocked.is_some() {
                if enabled {
                    if env.has_probe() {
                        env.reset_probe();
                    }
                    inputs.inputs().release(false)?;
                }
                let since = *missing_since.get_or_insert(acquired);
                if inputs.clock().now() - since >= 2.0 {
                    guard["status"] = json!("hud_unverified");
                    return Err(RunError::Runtime("paused_hud_unverified".into()));
                }
            } else {
                missing_since = None;
                if env.encounter_recheck(&frame, acquired)?
                    && (0.0..0.25).contains(&(inputs.clock().now() - acquired))
                    && !inputs.stop().is_set()
                    && inputs.focused()?
                {
                    if enabled {
                        inputs.inputs().release(false)?;
                    }
                    env.reset_probe();
                    report["encounter_recheck_requested"] = json!(true);
                    guard["status"] = json!("encounter_recheck");
                    return Ok(());
                }
                if enabled {
                    let observer = vision
                        .as_deref_mut()
                        .ok_or_else(|| RunError::Runtime("paused defense vision missing".into()))?;
                    if observer.prepares_ground() {
                        observer.prepare(&frame, acquired, &reading)?;
                    }
                    let player = observer.ground(&frame, acquired)?.point;
                    let candidates = player
                        .map(|player| observer.hazards(&frame, player, acquired))
                        .transpose()?
                        .unwrap_or_default();
                    let mut close = false;
                    for candidate in &candidates {
                        if let Some(player) = player
                            && near_ground(
                                candidate.ground(),
                                player.map(f64::from),
                                NEAR_GROUND_RADIUS,
                            )?
                        {
                            close = true;
                            break;
                        }
                    }
                    let expires = buff.ok_or_else(|| {
                        RunError::Runtime("paused defense deadline missing".into())
                    })?;
                    if !close && env.has_probe() && inputs.clock().now() < expires {
                        let target = if let Some(player) = player {
                            let points = env.defense_probe_points(&frame, player)?;
                            env.probe(&frame, player, acquired, points.as_deref())?
                        } else {
                            env.reset_probe();
                            None
                        };
                        close = target.is_some();
                        if close {
                            guard["hover_hostile_confirmations"] = json!(
                                guard["hover_hostile_confirmations"].as_u64().unwrap_or(0) + 1
                            );
                        }
                    }
                    let now = inputs.clock().now();
                    if close
                        && (0.0..0.25).contains(&(now - acquired))
                        && now < expires
                        && !inputs.stop().is_set()
                        && inputs.focused()?
                    {
                        if attack_started.is_none() {
                            attack_baseline = inputs
                                .inputs()
                                .snapshot()?
                                .dispatches
                                .get(&inputs.inputs().keys()[1])
                                .copied()
                                .unwrap_or(0);
                        }
                        let started = *attack_started.get_or_insert(now);
                        if now - started < 6.0 {
                            combat(
                                inputs,
                                "paused_current_near_hostile",
                                (acquired + 0.25).min(expires).min(started + 6.0),
                                now,
                            )?;
                            guard["nova_observations"] =
                                json!(guard["nova_observations"].as_u64().unwrap_or(0) + 1);
                        } else {
                            inputs.inputs().release(false)?;
                            guard["defense_status"] = json!("near_hostile_requires_review");
                            return Err(RunError::Runtime("near_hostile_requires_review".into()));
                        }
                    } else {
                        inputs.inputs().release(false)?;
                        let dispatches = inputs
                            .inputs()
                            .snapshot()?
                            .dispatches
                            .get(&inputs.inputs().keys()[1])
                            .copied()
                            .unwrap_or(0);
                        if let Some(started) =
                            attack_started.filter(|_| dispatches > attack_baseline)
                        {
                            env.inspect_loot(started, dispatches, buff)?;
                        }
                        attack_started = None;
                    }
                }
            }
            env.wait(0.02)?;
        }
        guard["status"] = json!("operator_stop");
        Ok(())
    })();
    let cleanup = inputs
        .finish(Duration::from_secs(1))
        .map_err(RunError::from);
    guard["ended_at"] = json!(inputs.clock().now());
    report["paused_survival_guard"] = guard;
    if let Some(frame) = last {
        evidence.push(EncounterEvidence {
            filename: "paused-survival-last.png".into(),
            frame,
        });
    }
    merge(result, cleanup)?;
    if !focus_handoff {
        faults(inputs, "paused defense watchdog")?;
    }
    Ok(())
}
