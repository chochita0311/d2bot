use super::*;
use crate::common::{
    encounters::LifeState,
    error::ControlError,
    field_control::{FieldAction, FieldDirective, FieldInputBackend, FieldInputSession},
    survival::CharacterSurvivalPolicy,
    survival_vision::resource_guard,
};
use crate::runs::summoner::{
    continuation::{NEAR_GROUND_RADIUS, near_ground},
    life_vision::SummonerLifeTracker,
    navigation_execution::{
        DispatchDiagnostics, DispatchOptions, StepEnvironment, StepRequest, dispatch_planned_step,
    },
    scene_vision::HostileTarget,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub trait EncounterEnvironment {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError>;
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError>;
    fn wait(&self, seconds: f64) -> Result<(), ControlError>;
    fn frame_to_desktop(&self, point: [i32; 2]) -> Result<[i64; 2], ControlError>;
    fn emit(&mut self, message: &str);
    fn loot_movement_allowed(&self) -> Option<bool> {
        None
    }
    fn recover_resources(
        &mut self,
        _frame: &Arc<BgrFrame>,
        _reading: &HudReading,
        _acquired: f64,
    ) -> Result<bool, RunError> {
        Ok(false)
    }
    fn has_probe(&self) -> bool {
        false
    }
    fn probe_pending(&self) -> bool {
        false
    }
    fn encounter_map_visible(&mut self, _frame: &Arc<BgrFrame>) -> Result<Option<bool>, RunError> {
        Ok(None)
    }
    fn restore_map(&mut self) -> Result<Value, RunError> {
        Err(RunError::Runtime(
            "encounter map restoration unavailable".into(),
        ))
    }
    fn encounter_panels_open(&mut self, _frame: &Arc<BgrFrame>) -> Result<bool, RunError> {
        Ok(true)
    }
    fn reset_probe(&mut self) {}
    fn actor_proposals(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
    ) -> Result<Vec<[i32; 2]>, RunError>;
    fn probe(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        points: Option<&[[i32; 2]]>,
    ) -> Result<Option<HostileTarget>, RunError>;
    fn inspect_loot(
        &mut self,
        started: f64,
        dispatches: u64,
        buff_expires_at: Option<f64>,
    ) -> Result<Option<Value>, RunError>;
}
pub struct EncounterRequest<'a> {
    pub policy: &'a CharacterSurvivalPolicy,
    pub buff_expires_at: f64,
    pub reference_seconds: f64,
}
pub struct EncounterEvidence {
    pub filename: String,
    pub frame: Arc<BgrFrame>,
}
#[derive(Serialize)]
struct EvidenceRecord {
    file: String,
    sequence: u64,
    acquired_at: f64,
    phase: &'static str,
}
#[derive(Serialize)]
pub struct EncounterReport {
    pub status: &'static str,
    pub observations: Vec<Value>,
    pub approach_requests: u64,
    pub nova_dispatches: u64,
    evidence: Vec<EvidenceRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kill_confirmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    life_confirmation: Option<Value>,
    evidence_frames: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    loot_inspection: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loot_complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    map_inspection: Option<Value>,
}
impl Default for EncounterReport {
    fn default() -> Self {
        Self {
            status: "kill_unconfirmed",
            observations: vec![],
            approach_requests: 0,
            nova_dispatches: 0,
            evidence: vec![],
            blocked: None,
            kill_confirmed: None,
            life_confirmation: None,
            evidence_frames: 0,
            loot_inspection: None,
            loot_complete: None,
            map_inspection: None,
        }
    }
}
#[derive(Clone)]
struct Witness {
    sequence: u64,
    acquired: f64,
    frame: Arc<BgrFrame>,
}
#[derive(Default)]
struct State {
    report: EncounterReport,
    first_nova: Option<f64>,
    missing_since: Option<f64>,
    last: Option<Witness>,
    platform_seen: bool,
    platform_missing_since: Option<f64>,
    prelude: VecDeque<Witness>,
    saved: usize,
    last_saved: Option<f64>,
    alive: Option<Witness>,
    first_corpse: Option<Witness>,
    tentative_corpse: Option<Witness>,
    tentative_followup: Option<Witness>,
    approach_pending: bool,
    map_hidden: bool,
    map_off_observed: bool,
}
impl State {
    fn retain(
        &mut self,
        evidence: &mut Vec<EncounterEvidence>,
        name: String,
        witness: &Witness,
        phase: &'static str,
    ) {
        evidence.push(EncounterEvidence {
            filename: name.clone(),
            frame: witness.frame.clone(),
        });
        self.report.evidence.push(EvidenceRecord {
            file: name,
            sequence: witness.sequence,
            acquired_at: witness.acquired,
            phase,
        });
    }
    fn acquired(&mut self, evidence: &mut Vec<EncounterEvidence>, witness: Witness) {
        if self.tentative_corpse.is_some() && self.tentative_followup.is_none() {
            self.tentative_followup = Some(witness.clone());
        }
        self.prelude.push_back(witness.clone());
        while self.prelude.len() > 4 {
            self.prelude.pop_front();
        }
        if witness.sequence == 0 {
            self.retain(evidence, "encounter-before.png".into(), &witness, "initial");
            self.saved += 1;
        } else if self.approach_pending && self.saved < 21 {
            self.retain(
                evidence,
                format!("encounter-after-approach-{:03}.png", witness.sequence),
                &witness,
                "prefight",
            );
            self.saved += 1;
            self.approach_pending = false;
        }
        self.last = Some(witness);
    }
    fn finish_evidence(&mut self, evidence: &mut Vec<EncounterEvidence>) {
        if self.first_nova.is_none() && self.report.kill_confirmed != Some(true) {
            let witness = self
                .prelude
                .iter()
                .find(|w| {
                    self.last
                        .as_ref()
                        .is_some_and(|last| w.sequence < last.sequence)
                        && !self
                            .report
                            .evidence
                            .iter()
                            .any(|e| e.sequence == w.sequence)
                })
                .cloned();
            if let Some(witness) = witness.filter(|_| self.saved < 23) {
                self.retain(
                    evidence,
                    format!("encounter-unconfirmed-{:03}.png", witness.sequence),
                    &witness,
                    "unconfirmed",
                );
                self.saved += 1;
            }
        }
        if self.report.kill_confirmed != Some(true) {
            for (name, witness, phase) in [
                (
                    "encounter-tentative-corpse.png",
                    self.tentative_corpse.clone(),
                    "tentative",
                ),
                (
                    "encounter-tentative-followup.png",
                    self.tentative_followup.clone(),
                    "unconfirmed_followup",
                ),
            ] {
                if let Some(witness) = witness.filter(|_| self.saved < 23) {
                    self.retain(evidence, name.into(), &witness, phase);
                    self.saved += 1;
                }
            }
        }
        self.report.evidence_frames = self.saved + usize::from(self.last.is_some());
        if let Some(last) = self.last.clone() {
            self.retain(evidence, "encounter-after.png".into(), &last, "final");
        }
    }
}
struct Step<'a, B: FieldInputBackend, E> {
    inputs: &'a FieldInputSession<B>,
    env: &'a E,
}
impl<B: FieldInputBackend, E: EncounterEnvironment> StepEnvironment for Step<'_, B, E> {
    fn clock(&self) -> f64 {
        self.inputs.clock().now()
    }
    fn stopped(&self) -> bool {
        self.inputs.stop().is_set()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        self.inputs.focused()
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        self.env.wait(seconds)
    }
}
fn merge(primary: Result<(), RunError>, cleanup: Result<(), RunError>) -> Result<(), RunError> {
    match (primary, cleanup) {
        (Err(primary), Err(cleanup)) => Err(primary.with_cleanup(cleanup)),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}
fn platform_probe_points(goal: [f64; 2], player: [i32; 2]) -> Result<Vec<[i32; 2]>, RunError> {
    let mut points = Vec::with_capacity(5);
    for [dx, dy] in [
        [0.0, 176.0],
        [-24.0, 176.0],
        [24.0, 176.0],
        [0.0, 88.0],
        [-24.0, 88.0],
        [24.0, 88.0],
        [0.0, 64.0],
        [0.0, 104.0],
    ] {
        let [x, y] = [goal[0] + dx, goal[1] + dy];
        if !(24.0..1900.0).contains(&x) || !(55.0..900.0).contains(&y) {
            continue;
        }
        let point = [x.round_ties_even() as i32, y.round_ties_even() as i32];
        if near_ground(
            Some([f64::from(point[0]), f64::from(point[1]) + 80.0]),
            player.map(f64::from),
            NEAR_GROUND_RADIUS,
        )? {
            points.push(point);
        }
    }
    Ok(points)
}
pub fn run_platform_encounter<B: FieldInputBackend, E: EncounterEnvironment, V: EncounterVision>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    vision: &mut V,
    request: EncounterRequest<'_>,
    life: Option<&mut SummonerLifeTracker>,
    evidence: &mut Vec<EncounterEvidence>,
) -> Result<EncounterReport, RunError> {
    execute(
        inputs,
        env,
        vision,
        request,
        life,
        evidence,
        |inputs, deadline| inputs.start("summoner-encounter-watchdog", deadline),
    )
}
fn execute<B: FieldInputBackend, E: EncounterEnvironment, V: EncounterVision>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    vision: &mut V,
    request: EncounterRequest<'_>,
    mut life: Option<&mut SummonerLifeTracker>,
    evidence: &mut Vec<EncounterEvidence>,
    start: impl FnOnce(&mut FieldInputSession<B>, f64) -> Result<(), ControlError>,
) -> Result<EncounterReport, RunError> {
    let started = inputs.clock().now();
    let deadline = (started + 12.0).min(request.buff_expires_at);
    let mut state = State::default();
    let result = (|| {
        start(inputs, deadline)?;
        for sequence in 0..120 {
            if inputs.stop().is_set() || !inputs.focused()? || inputs.clock().now() >= deadline {
                break;
            }
            if env.loot_movement_allowed() == Some(false) {
                state.report.blocked = Some("item_labels_not_off");
                break;
            }
            let acquired = inputs.clock().now();
            let frame = env.capture()?;
            let witness = Witness {
                sequence,
                acquired,
                frame,
            };
            state.acquired(evidence, witness.clone());
            let mut row = json!({"sequence":sequence,"acquired_at":acquired,"target":null,"player":null,"platform_ground":null});
            let iteration = state.step(FrameStep {
                inputs,
                env,
                vision,
                request: &request,
                life: life.as_deref_mut(),
                evidence,
                witness: &witness,
                deadline,
                row: &mut row,
            });
            state.report.observations.push(row);
            if iteration? {
                break;
            }
        }
        env.emit(if state.report.kill_confirmed == Some(true) {
            "SUMMONER_ENCOUNTER: current platform corpse confirmed"
        } else {
            "SUMMONER_ENCOUNTER: bounded approach/Nova finished; kill unconfirmed"
        });
        Ok(())
    })();
    let vision_cleanup = vision.finish(Duration::from_secs(1));
    let input_cleanup = inputs
        .finish(Duration::from_secs(1))
        .map_err(RunError::from);
    let mut cleanup = merge(vision_cleanup, input_cleanup);
    if state.map_hidden {
        let restored = (|| {
            if inputs.stop().is_set() || !inputs.focused()? {
                return Err(RunError::Runtime(
                    "encounter map restoration interrupted".into(),
                ));
            }
            inputs.inputs().backend().press("tab")?;
            let restored = env.restore_map()?;
            if let Some(record) = state.report.map_inspection.as_mut() {
                record["restoration"] = restored;
                record["status"] = json!("restored");
            }
            Ok(())
        })();
        if let Err(error) = &restored
            && let Some(record) = state.report.map_inspection.as_mut()
        {
            record["status"] = json!("restore_unverified");
            record["error"] = json!(error.to_string());
        }
        cleanup = merge(cleanup, restored);
    }
    match inputs.inputs().snapshot() {
        Ok(snapshot) => {
            state.report.nova_dispatches = snapshot
                .dispatches
                .get(&inputs.inputs().keys()[1])
                .copied()
                .unwrap_or(0)
        }
        Err(error) => cleanup = merge(cleanup, Err(error.into())),
    }
    state.finish_evidence(evidence);
    merge(result, cleanup)?;
    if let Some(fault) = inputs.faults()?.first() {
        return Err(RunError::Runtime(format!(
            "encounter watchdog: {}",
            fault.message
        )));
    }
    if (state.report.nova_dispatches > 0 || state.report.kill_confirmed == Some(true))
        && let Some(inspection) = env.inspect_loot(
            started,
            state.report.nova_dispatches,
            Some(request.buff_expires_at),
        )?
    {
        state.report.loot_complete = Some(
            state.report.kill_confirmed == Some(true)
                && inspection["status"] == "inspection_complete"
                && inspection["bounded_reason"] == "two_frames_without_eligible_candidate"
                && inspection["movement_allowed"] == true
                && python_falsy(&inspection["clicks"]),
        );
        state.report.loot_inspection = Some(inspection);
    }
    Ok(state.report)
}
fn python_falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(value) => !*value,
        Value::Number(value) => value.as_f64() == Some(0.0),
        Value::String(value) => value.is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
    }
}
struct FrameStep<'a, B: FieldInputBackend, E, V> {
    inputs: &'a FieldInputSession<B>,
    env: &'a mut E,
    vision: &'a mut V,
    request: &'a EncounterRequest<'a>,
    life: Option<&'a mut SummonerLifeTracker>,
    evidence: &'a mut Vec<EncounterEvidence>,
    witness: &'a Witness,
    deadline: f64,
    row: &'a mut Value,
}
impl State {
    fn step<B: FieldInputBackend, E: EncounterEnvironment, V: EncounterVision>(
        &mut self,
        step: FrameStep<'_, B, E, V>,
    ) -> Result<bool, RunError> {
        let FrameStep {
            inputs,
            env,
            vision,
            request,
            mut life,
            evidence,
            witness,
            deadline,
            row,
        } = step;
        let Witness {
            sequence,
            acquired,
            frame,
        } = witness;
        let (sequence, acquired) = (*sequence, *acquired);
        let reading = env.hud(frame)?;
        if let Some(blocked) = resource_guard(&reading, request.policy) {
            row["blocked"] = json!(blocked);
            inputs.inputs().release(false)?;
            if blocked == "mercenary_unverified" {
                return Err(RunError::Runtime(blocked.into()));
            }
            if blocked == "resource_low" {
                if env.recover_resources(frame, &reading, acquired)? {
                    env.reset_probe();
                    return Ok(false);
                }
                return Err(RunError::Runtime(blocked.into()));
            }
            return Ok(false);
        }
        if vision.prepares_ground() {
            vision.prepare(frame, acquired, &reading, inputs.focused()?)?;
        }
        let observed = vision.observe(
            frame,
            sequence,
            acquired,
            self.report.approach_requests < 2,
            life.is_some(),
        )?;
        let EncounterFrame {
            player,
            platform,
            target,
            corpse,
            path,
        } = observed;
        let goal = target
            .as_ref()
            .map(|target| target.ground)
            .or_else(|| platform.as_ref().and_then(|platform| platform.ground_point));
        row["target"] = json!(target);
        row["corpse"] = json!(corpse);
        row["player"] = json!(player.point);
        row["platform_ground"] = json!(goal);
        row["platform_anchor"]=platform.as_ref().map(|platform|json!({"ground_point":platform.ground_point,"evidence_kind":platform.evidence_kind,"rune_boxes":platform.rune_boxes})).unwrap_or(Value::Null);
        let now = inputs.clock().now();
        row["processed_at"] = json!(now);
        let fresh = player.point.is_some() && (0.0..0.25).contains(&(now - acquired));
        if platform.is_some() && fresh {
            self.platform_seen = true;
            self.platform_missing_since = None;
        } else if self.platform_seen && platform.is_none() && fresh {
            let since = *self.platform_missing_since.get_or_insert(now);
            row["platform_reacquisition_seconds"] = json!(now - since);
            if now - since >= 6.0 {
                inputs.inputs().release(false)?;
                row["action"] = json!("platform_reacquisition_timeout");
                return Ok(true);
            }
        }
        if let Some(tracker) = life.as_deref_mut() {
            let reading = tracker.observe(
                target.as_ref(),
                corpse.as_ref(),
                platform.as_ref(),
                sequence,
                acquired,
                now,
            )?;
            row["life"] = json!(reading);
            if reading.state == LifeState::Alive {
                self.alive = Some(witness.clone());
                self.first_corpse = None;
            }
            if reading
                .corpse
                .as_ref()
                .is_some_and(|link| link.first_dead_sequence == sequence)
            {
                self.first_corpse = Some(witness.clone());
                if self.tentative_corpse.is_none() {
                    self.tentative_corpse = self.first_corpse.clone();
                }
            }
            if reading.state == LifeState::Dead {
                inputs.inputs().release(false)?;
                let first = self
                    .first_corpse
                    .clone()
                    .ok_or_else(|| RunError::Runtime("confirmed corpse witness missing".into()))?;
                if let Some(alive) = self.alive.clone().filter(|alive| {
                    reading
                        .corpse
                        .as_ref()
                        .is_some_and(|link| link.linked_alive_sequence == Some(alive.sequence))
                }) {
                    self.retain(
                        evidence,
                        "encounter-alive-witness.png".into(),
                        &alive,
                        "prefight",
                    );
                    self.saved += 1;
                }
                self.retain(
                    evidence,
                    "encounter-first-corpse.png".into(),
                    &first,
                    "combat",
                );
                self.saved += 1;
                self.report.status = "kill_confirmed";
                self.report.kill_confirmed = Some(true);
                self.report.life_confirmation = Some(json!(reading));
                return Ok(true);
            }
            if corpse.is_some() {
                inputs.inputs().release(false)?;
                row["action"] = json!("verify_current_platform_corpse");
                return Ok(false);
            }
        }
        if self.first_nova.is_some()
            && self.saved < 21
            && self.last_saved.is_none_or(|saved| acquired - saved >= 0.12)
        {
            self.retain(
                evidence,
                format!("encounter-combat-{sequence:03}.png"),
                witness,
                "combat",
            );
            self.saved += 1;
            self.last_saved = Some(acquired);
        }
        let mut hostile = None;
        if fresh && self.platform_seen && platform.is_none() && env.has_probe() {
            if !env.probe_pending() {
                inputs.inputs().release(false)?;
            }
            let point = player
                .point
                .ok_or_else(|| RunError::Runtime("fresh player missing".into()))?;
            let proposals = env.actor_proposals(frame, point)?;
            hostile = env.probe(
                frame,
                point,
                acquired,
                if proposals.is_empty() {
                    None
                } else {
                    Some(&proposals)
                },
            )?;
            row["current_defense_proposals"] = json!(proposals);
            row["current_defense_source"] = json!(if proposals.is_empty() {
                "current_near_hover_grid"
            } else {
                "current_actor_effects"
            });
            row["hostile_panel"] = json!(hostile);
        }
        if !fresh || (goal.is_none() && hostile.is_none()) {
            inputs.inputs().release(false)?;
            let since = *self.missing_since.get_or_insert(now);
            let limit = if fresh && self.platform_seen && platform.is_none() {
                6.0
            } else {
                1.0
            };
            row["missing_evidence_limit"] = json!(limit);
            return Ok(now - since >= limit);
        }
        self.missing_since = None;
        let point = player
            .point
            .ok_or_else(|| RunError::Runtime("fresh player missing".into()))?;
        let body_points = if target.is_none() && env.has_probe() {
            goal.map(|goal| platform_probe_points(goal, point))
                .transpose()?
                .unwrap_or_default()
        } else {
            vec![]
        };
        if fresh && target.is_none() && corpse.is_none() && !body_points.is_empty() {
            let visible = env.encounter_map_visible(frame)?;
            if self.report.map_inspection.is_none()
                && visible == Some(true)
                && !env.encounter_panels_open(frame)?
                && !inputs.stop().is_set()
                && inputs.focused()?
                && (0.0..0.25).contains(&(inputs.clock().now() - acquired))
            {
                inputs.inputs().release(false)?;
                env.reset_probe();
                inputs.inputs().backend().press("tab")?;
                self.map_hidden = true;
                self.report.map_inspection =
                    Some(json!({"status":"hide_requested","requested_at":inputs.clock().now()}));
                row["action"] = json!("summoner_map_hide");
                return Ok(false);
            }
            if visible == Some(false) {
                self.map_off_observed = true;
                if let Some(record) = self.report.map_inspection.as_mut() {
                    record["off_observed_at"] = json!(acquired);
                }
            } else if self.map_hidden {
                row["action"] = json!("summoner_map_hide_pending");
                return Ok(false);
            }
        }
        // 발판 중심은 적의 발 위치가 아니므로 외형 미확인 때는 가까이 접근한다.
        let radius = if target.is_some() {
            NEAR_GROUND_RADIUS
        } else {
            [100.0, 100.0]
        };
        if hostile.is_some()
            || !body_points.is_empty()
            || near_ground(goal, point.map(f64::from), radius)?
        {
            if target.is_none() && hostile.is_none() && env.has_probe() {
                if !env.probe_pending() {
                    inputs.inputs().release(false)?;
                }
                let mut points = body_points;
                if self.map_off_observed {
                    for actor in env.actor_proposals(frame, point)? {
                        if points.len() < 16 && !points.contains(&actor) {
                            points.push(actor);
                        }
                    }
                }
                row["platform_hostile_proposals"] = json!(points);
                hostile = env.probe(frame, point, acquired, Some(&points))?;
                row["hostile_panel"] = json!(hostile);
            }
            if target.is_none() && hostile.is_none() {
                inputs.inputs().release(false)?;
                row["action"] = json!(if env.has_probe() {
                    "platform_hostile_probe"
                } else {
                    "platform_hostile_unverified"
                });
                return Ok(false);
            }
            if self.first_nova.is_none() {
                for before in self.prelude.clone() {
                    if self.saved >= 21 {
                        break;
                    }
                    self.retain(
                        evidence,
                        format!("encounter-prefight-{:03}.png", before.sequence),
                        &before,
                        "prefight",
                    );
                    self.saved += 1;
                }
                self.last_saved = Some(acquired);
            }
            let first = *self.first_nova.get_or_insert(now);
            if now - first >= 6.0 {
                row["action"] = json!("nova_window_expired");
                row["nova_window_seconds"] = json!(now - first);
                return Ok(true);
            }
            inputs.inputs().apply(
                &FieldDirective::new(
                    FieldAction::Combat,
                    if target.is_some() {
                        "current_platform_robe_candidate"
                    } else {
                        "current_platform_hostile_panel"
                    },
                    None,
                    None,
                    Some((acquired + 0.25).min(deadline)),
                    false,
                )?,
                inputs.clock().now(),
            )?;
            row["action"] = json!(if target.is_some() {
                "nova_on_current_appearance"
            } else {
                "nova_on_current_hostile_panel"
            });
        } else {
            inputs.inputs().release(false)?;
            if self.report.approach_requests >= 2 {
                return Ok(true);
            }
            let goal = goal.ok_or_else(|| RunError::Runtime("approach target missing".into()))?;
            let path =
                path.ok_or_else(|| RunError::Runtime("approach floor task missing".into()))?;
            let choice = vision.choose(
                path,
                [goal[0] - f64::from(point[0]), goal[1] - f64::from(point[1])],
            )?;
            let Some(aim) = choice.aim.filter(|aim| {
                (f64::from(aim[0]) - goal[0]).hypot(f64::from(aim[1]) - goal[1]) <= 100.0
            }) else {
                row["action"] = json!("target_floor_unverified");
                return Ok(true);
            };
            let aim = env.frame_to_desktop(aim)?;
            let mut dispatch = DispatchDiagnostics::default();
            row["approach_dispatch"] =
                json!({"aim":aim,"captured_at":acquired,"expires_at":deadline});
            let sent = dispatch_planned_step(
                inputs.inputs().backend(),
                &Step { inputs, env },
                StepRequest {
                    aim,
                    key: &inputs.inputs().keys()[0],
                    captured_at: acquired,
                    expires_at: Some(deadline),
                },
                DispatchOptions {
                    verify_aim: Some(&mut || {
                        inputs
                            .inputs()
                            .backend()
                            .pointer_matches(aim)
                            .map_err(RunError::from)
                    }),
                    preparation: None,
                },
                &mut dispatch,
            )?;
            let mut record = serde_json::to_value(&dispatch)
                .map_err(|error| RunError::Runtime(error.to_string()))?;
            if let Some(fields) = record.as_object_mut() {
                fields.insert("aim".into(), json!(aim));
                fields.insert("captured_at".into(), json!(acquired));
                fields.insert("expires_at".into(), json!(deadline));
            }
            row["approach_dispatch"] = record;
            row["action"] = json!(if sent {
                "approach_requested"
            } else {
                "approach_not_sent"
            });
            self.report.approach_requests += u64::from(sent);
            self.approach_pending = sent;
            if sent && target.is_some() {
                row["approach_dispatch"]["target"] = json!("current_summoner_appearance");
            }
            if sent && let Some(tracker) = life {
                tracker.moved()?;
                self.alive = None;
                self.first_corpse = None;
            }
            if !sent {
                return Ok(dispatch.reason != "frame_expired");
            }
            env.wait(request.reference_seconds)?;
        }
        Ok(false)
    }
}
