//! 도착을 매번 확인하는 기존 단발 북쪽 구간. 연속 이동 실행기와 구분한다.
mod native;
pub use native::NativeLegacyVision;
#[cfg(test)]
mod tests;
use super::{
    continuous_navigation::CurrentPlan,
    encounter_execution::{EncounterEnvironment, EncounterEvidence},
    navigation::PathChoice,
    navigation_execution::{
        DispatchDiagnostics, DispatchOptions, LandingEnvironment, LandingResult, StepEnvironment,
        StepRequest, dispatch_planned_step,
    },
    scene_vision::{GroundReading, HoverCandidate},
};
use crate::{
    common::{
        config::ConfigError,
        encounters::VisualCandidate,
        field_control::FieldInputBackend,
        field_preparation::{validate_permit, validate_runtime_permit},
        frame::BgrFrame,
        survival::CharacterSurvivalPolicy,
        survival_vision::resource_guard,
    },
    runs::RunError,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub trait LegacyEnvironment: EncounterEnvironment {
    fn wall_clock(&self) -> f64;
}
pub struct LegacyRequest<'a> {
    pub policy: &'a CharacterSurvivalPolicy,
    pub proof: &'a Value,
    pub casts: usize,
    pub reference_seconds: f64,
    pub buff_expires_at: Option<f64>,
    pub movement_key: &'a str,
}
pub struct LegacyPlanRequest<'a> {
    pub acquired: f64,
    pub hub: Option<[f64; 2]>,
    pub position: [f64; 2],
    pub visited: &'a [[f64; 2]],
}
pub struct LegacyLandingRequest<'a, V> {
    pub frame: &'a Arc<BgrFrame>,
    pub view: &'a V,
    pub choice: &'a PathChoice,
    pub policy: &'a CharacterSurvivalPolicy,
    pub reference_seconds: f64,
    pub expires_at: f64,
}
pub trait LegacyVision {
    type View;
    fn has_hazards(&self) -> bool;
    fn has_hover(&self) -> bool;
    fn hub(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError>;
    fn ground(&mut self, frame: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError>;
    fn plan(
        &mut self,
        frame: &Arc<BgrFrame>,
        request: LegacyPlanRequest<'_>,
    ) -> Result<CurrentPlan<Self::View>, RunError>;
    fn hazards(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        now: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, RunError>;
    fn hover(
        &mut self,
        frame: &Arc<BgrFrame>,
        aim: [i32; 2],
    ) -> Result<Vec<HoverCandidate>, RunError>;
    fn landing(
        &mut self,
        env: &mut impl LandingEnvironment,
        request: LegacyLandingRequest<'_, Self::View>,
    ) -> Result<LandingResult, RunError>;
}
struct Context<'a, B, G, E, V> {
    backend: &'a B,
    guards: &'a G,
    env: &'a mut E,
    vision: &'a mut V,
    request: &'a LegacyRequest<'a>,
    expires: f64,
}
struct State<'a> {
    progress: &'a mut Value,
    evidence: &'a mut Vec<EncounterEvidence>,
    retained: Vec<(String, usize)>,
    row: Option<usize>,
}
impl State<'_> {
    fn retain(&mut self, index: usize, stage: &str, frame: Arc<BgrFrame>) {
        let item = EncounterEvidence {
            filename: format!("north-{index:02}-{stage}.png"),
            frame,
        };
        if let Some((_, at)) = self.retained.iter().find(|(name, _)| name == stage) {
            self.evidence[*at] = item;
        } else {
            self.retained.push((stage.into(), self.evidence.len()));
            self.evidence.push(item);
        }
    }
    fn steps(&mut self) -> Result<&mut Vec<Value>, RunError> {
        self.progress["steps"]
            .as_array_mut()
            .ok_or_else(|| RunError::Runtime("navigation steps must be an array".into()))
    }
    fn record(&mut self, key: &str, value: impl serde::Serialize) -> Result<(), RunError> {
        let at = self
            .row
            .ok_or_else(|| RunError::Runtime("step row missing".into()))?;
        self.steps()?[at][key] = json!(value);
        Ok(())
    }
    fn row_value(&self) -> Result<&Value, RunError> {
        self.progress["steps"]
            .as_array()
            .and_then(|steps| self.row.and_then(|at| steps.get(at)))
            .ok_or_else(|| RunError::Runtime("step row missing".into()))
    }
}
fn pair(value: &Value) -> Option<[f64; 2]> {
    let p = value.as_array()?;
    if p.len() != 2 {
        return None;
    }
    Some([p[0].as_f64()?, p[1].as_f64()?])
}
fn unavailable(error: &RunError) -> bool {
    matches!(
        error,
        RunError::Runtime(_)
            | RunError::Action(crate::actions::ActionError::Runtime(_))
            | RunError::Observation(crate::common::error::ObservationError::Invalid(_))
    )
}
pub fn run_north_segment<
    B: FieldInputBackend,
    G: StepEnvironment,
    E: LegacyEnvironment,
    V: LegacyVision,
>(
    backend: &B,
    guards: &G,
    env: &mut E,
    vision: &mut V,
    request: LegacyRequest<'_>,
    progress: &mut Value,
    evidence: &mut Vec<EncounterEvidence>,
) -> Result<Value, RunError> {
    if !(1..=12).contains(&request.casts) {
        return Err(
            ConfigError::value("supervised north segment requires one to twelve casts").into(),
        );
    }
    if request.proof["source"] == "runtime_screen" {
        validate_runtime_permit(request.proof, env.wall_clock())?;
    } else {
        validate_permit(
            request.proof,
            "flash",
            request.policy,
            env.wall_clock(),
            true,
        )?;
    }
    let started = guards.clock();
    let expires = request
        .buff_expires_at
        .map_or(started + 12.0, |at| (started + 12.0).min(at));
    if expires <= started {
        return Err(RunError::Runtime("buff_renewal_required".into()));
    }
    if progress.is_null() {
        *progress = json!({"steps":[],"position":[0.0,0.0],"breadcrumbs":[[0.0,0.0]]});
    }
    let mut state = State {
        progress,
        evidence,
        retained: vec![],
        row: None,
    };
    let offset = state.steps()?.len();
    let mut c = Context {
        backend,
        guards,
        env,
        vision,
        request: &request,
        expires,
    };
    for index in offset..offset + request.casts {
        if guards.stopped() || !guards.focused()? {
            return Err(RunError::Runtime(
                "north segment interrupted or unfocused".into(),
            ));
        }
        if guards.clock() >= expires {
            return Err(RunError::Runtime("supervised segment time limit".into()));
        }
        state.row = None;
        state.retained.clear();
        if let Some(status) = state.step(&mut c, index)? {
            let mut result = state.progress.clone();
            result["status"] = json!(status);
            return Ok(result);
        }
    }
    let mut result = state.progress.clone();
    result["status"] = json!("north_segment_observed");
    Ok(result)
}
impl State<'_> {
    fn step<B: FieldInputBackend, G: StepEnvironment, E: LegacyEnvironment, V: LegacyVision>(
        &mut self,
        c: &mut Context<'_, B, G, E, V>,
        index: usize,
    ) -> Result<Option<&'static str>, RunError> {
        let deadline = c.expires.min(c.guards.clock() + 2.0);
        let mut attempts = 0;
        let (frame, view, choice) = loop {
            if c.guards.stopped() || !c.guards.focused()? {
                return Err(RunError::Runtime(
                    "north segment interrupted or unfocused".into(),
                ));
            }
            if c.guards.clock() >= c.expires {
                return Err(RunError::Runtime("supervised segment time limit".into()));
            }
            if c.guards.clock() >= deadline {
                return Err(RunError::Runtime("step preparation time limit".into()));
            }
            let acquired = c.guards.clock();
            attempts += 1;
            let frame = c.env.capture()?;
            let reading = c.env.hud(&frame)?;
            if let Some(blocked) = resource_guard(&reading, c.request.policy) {
                if blocked == "resource_low" || c.guards.clock() >= deadline {
                    return Err(RunError::Runtime(blocked.into()));
                }
                c.guards.wait(0.02)?;
                continue;
            }
            let plan = (|| {
                let hub = c.vision.hub(&frame)?;
                let player = if c.vision.has_hazards() {
                    let reading = c.vision.ground(&frame, acquired)?;
                    Some(
                        reading
                            .point
                            .ok_or_else(|| RunError::Runtime(reading.reason))?,
                    )
                } else {
                    None
                };
                let position = pair(&self.progress["position"])
                    .ok_or_else(|| RunError::Runtime("navigation position missing".into()))?;
                let visited = self.progress["breadcrumbs"]
                    .as_array()
                    .ok_or_else(|| RunError::Runtime("navigation breadcrumbs missing".into()))?
                    .iter()
                    .map(|v| {
                        pair(v).ok_or_else(|| {
                            RunError::Runtime("navigation breadcrumb invalid".into())
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let plan = c.vision.plan(
                    &frame,
                    LegacyPlanRequest {
                        acquired,
                        hub,
                        position,
                        visited: &visited,
                    },
                )?;
                let candidates = player
                    .map(|player| {
                        c.vision.hazards(
                            &frame,
                            player,
                            acquired,
                            c.guards.clock(),
                            plan.choice.aim,
                        )
                    })
                    .transpose()?
                    .unwrap_or_default();
                if candidates.is_empty() && c.guards.clock() - acquired >= 0.10 {
                    if c.guards.clock() >= deadline {
                        return Err(RunError::Runtime("planning_budget_exhausted".into()));
                    }
                    return Ok(None);
                }
                Ok(Some((plan, candidates)))
            })();
            let (plan, candidates) = match plan {
                Ok(Some(plan)) => plan,
                Ok(None) => continue,
                Err(error) if unavailable(&error) => {
                    if !matches!(
                        error.to_string().as_str(),
                        "ground_unverified"
                            | "planned step frame is stale"
                            | "no_observed_progress"
                            | "player_floor_unobserved"
                    ) || c.guards.clock() >= deadline
                    {
                        self.retain(index, "planning-failed", frame);
                        self.progress["planning_failure"] =
                            json!({"index":index,"reason":error.to_string(),"attempts":attempts});
                        return Err(error);
                    }
                    c.guards.wait(0.02)?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let CurrentPlan {
                choice,
                ground,
                view,
            } = plan;
            if self.row.is_none() {
                let at = self.steps()?.len();
                self.steps()?
                    .push(json!({"index":index,"dispatch_retries":[]}));
                self.row = Some(at);
            }
            self.record("choice", &choice)?;
            self.record("ground", &ground)?;
            self.record("planned_at", acquired)?;
            self.record("planning_seconds", c.guards.clock() - acquired)?;
            self.record("planning_attempts", attempts)?;
            let at = self
                .row
                .ok_or_else(|| RunError::Runtime("step row missing".into()))?;
            if let Some(row) = self.steps()?[at].as_object_mut() {
                row.remove("aim_check");
            }
            self.retain(index, "before", frame.clone());
            if !candidates.is_empty() {
                self.record("status", "monster_candidate_review")?;
                self.record("candidates", candidates)?;
                c.env
                    .emit("NORTH_REVIEW: current monster appearance candidate; no next cast");
                return Ok(Some("north_hazard_review"));
            }
            let aim = c.env.frame_to_desktop(
                choice
                    .aim
                    .ok_or_else(|| RunError::Runtime("planned aim missing".into()))?,
            )?;
            let mut diagnostics = DispatchDiagnostics::default();
            let sent = if c.vision.has_hover() {
                let backend = c.backend;
                let guards = c.guards;
                let key = c.request.movement_key;
                let expires = c.expires;
                let mut verify = || self.verify(c, index, &choice, acquired);
                dispatch_planned_step(
                    backend,
                    guards,
                    StepRequest {
                        aim,
                        key,
                        captured_at: acquired,
                        expires_at: Some(expires),
                    },
                    DispatchOptions {
                        verify_aim: Some(&mut verify),
                        preparation: None,
                    },
                    &mut diagnostics,
                )?
            } else {
                dispatch_planned_step(
                    c.backend,
                    c.guards,
                    StepRequest {
                        aim,
                        key: c.request.movement_key,
                        captured_at: acquired,
                        expires_at: Some(c.expires),
                    },
                    DispatchOptions::default(),
                    &mut diagnostics,
                )?
            };
            if sent {
                break (frame, view, choice);
            }
            self.record("status", "dispatch_blocked")?;
            let check = self.row_value()?["aim_check"].clone();
            let now = c.guards.clock();
            let resources = check["resources"].as_str();
            let hover = check["hover"].as_array().is_some_and(|v| !v.is_empty());
            let retry = !c.guards.stopped()
                && c.guards.focused()?
                && now < deadline
                && matches!(resources, None | Some("observation_unverified"))
                && !hover
                && (resources == Some("observation_unverified") || now - acquired >= 0.25);
            if !retry {
                return Ok(Some("north_failed"));
            }
            let at = self
                .row
                .ok_or_else(|| RunError::Runtime("step row missing".into()))?;
            self.steps()?[at]["dispatch_retries"]
                .as_array_mut()
                .ok_or_else(|| RunError::Runtime("dispatch retries missing".into()))?
                .push(json!({"at":now,"reason":resources.unwrap_or("screen_stale")}));
            c.guards.wait(0.02)?;
        };
        let mut env = Landing {
            env: c.env,
            guards: c.guards,
        };
        let landing = c.vision.landing(
            &mut env,
            LegacyLandingRequest {
                frame: &frame,
                view: &view,
                choice: &choice,
                policy: c.request.policy,
                reference_seconds: c.request.reference_seconds,
                expires_at: c.expires,
            },
        )?;
        self.record("status", landing.status)?;
        self.record("landing", &landing)?;
        self.retain(index, "after", c.env.capture()?);
        c.env.emit(&format!(
            "NORTH_STEP {}: {} displacement=({:?}, {:?})",
            index + 1,
            landing.status,
            landing.displacement[0],
            landing.displacement[1]
        ));
        if landing.status != "arrival_observed" {
            return Ok(Some("north_failed"));
        }
        let position = pair(&self.progress["position"])
            .ok_or_else(|| RunError::Runtime("navigation position missing".into()))?;
        self.progress["position"] = json!([
            position[0] + landing.displacement[0],
            position[1] + landing.displacement[1]
        ]);
        let position = self.progress["position"].clone();
        self.progress["breadcrumbs"]
            .as_array_mut()
            .ok_or_else(|| RunError::Runtime("navigation breadcrumbs missing".into()))?
            .push(position);
        Ok(None)
    }
    fn verify<B: FieldInputBackend, G: StepEnvironment, E: LegacyEnvironment, V: LegacyVision>(
        &mut self,
        c: &mut Context<'_, B, G, E, V>,
        index: usize,
        choice: &PathChoice,
        acquired: f64,
    ) -> Result<bool, RunError> {
        if c.guards.stopped() || !c.guards.focused()? {
            return Ok(false);
        }
        let aimed = c.guards.clock();
        let frame = c.env.capture()?;
        let grabbed = c.guards.clock();
        let reading = c.env.hud(&frame)?;
        let blocked = resource_guard(&reading, c.request.policy);
        let hud_at = c.guards.clock();
        let hover = c.vision.hover(
            &frame,
            choice
                .aim
                .ok_or_else(|| RunError::Runtime("planned aim missing".into()))?,
        )?;
        let checked = c.guards.clock();
        self.record("aim_check",json!({"captured_at":aimed,"capture_seconds":grabbed-aimed,"hud_seconds":hud_at-grabbed,"hover_seconds":checked-hud_at,"checked_at":checked,"original_frame_age":checked-acquired,"resources":blocked,"hover":hover}))?;
        let clear = blocked.is_none() && hover.is_empty() && checked - aimed < 0.25;
        if !clear {
            self.retain(index, "aim-blocked", frame);
        }
        Ok(clear)
    }
}
struct Landing<'a, E, G> {
    env: &'a mut E,
    guards: &'a G,
}
impl<E, G: StepEnvironment> StepEnvironment for Landing<'_, E, G> {
    fn clock(&self) -> f64 {
        self.guards.clock()
    }
    fn stopped(&self) -> bool {
        self.guards.stopped()
    }
    fn focused(&self) -> Result<bool, crate::common::error::ControlError> {
        self.guards.focused()
    }
    fn wait(&self, seconds: f64) -> Result<(), crate::common::error::ControlError> {
        self.guards.wait(seconds)
    }
}
impl<E: LegacyEnvironment, G: StepEnvironment> LandingEnvironment for Landing<'_, E, G> {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.env.capture()
    }
    fn hud(
        &mut self,
        frame: &Arc<BgrFrame>,
    ) -> Result<crate::common::survival_vision::HudReading, RunError> {
        self.env.hud(frame)
    }
}
