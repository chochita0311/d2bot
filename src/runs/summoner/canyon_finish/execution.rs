use crate::{
    common::{
        config::ConfigError,
        error::ControlError,
        field_control::{DesktopPoint, FieldInputBackend},
        frame::BgrFrame,
        survival::CharacterSurvivalPolicy,
        survival_vision::{HudReading, resource_guard},
    },
    runs::{
        RunError,
        summoner::{
            encounter_execution::EncounterEvidence,
            navigation_execution::{
                DispatchDiagnostics, DispatchOptions, StepEnvironment, StepRequest,
                dispatch_planned_step,
            },
        },
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
#[cfg(test)]
mod tests;
pub trait CanyonBackend: FieldInputBackend {
    fn click(&self) -> Result<(), ControlError>;
}
pub trait CanyonEnvironment {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError>;
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError>;
    fn frame_to_desktop(&self, point: [f64; 2]) -> Result<DesktopPoint, ControlError>;
    fn emit(&mut self, message: &str);
}
pub trait CanyonVision {
    fn town_frame(&mut self, frame: &Arc<BgrFrame>) -> Result<(), RunError>;
    fn find(&mut self, name: &str) -> Result<bool, RunError>;
    fn steps(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError>;
    fn portal(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError>;
    fn journal(&mut self, frame: &Arc<BgrFrame>) -> Result<Option<[f64; 2]>, RunError>;
    fn prepares_ground(&self) -> bool;
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        hud: &HudReading,
        focused: bool,
    ) -> Result<(), RunError>;
    fn ground(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
    ) -> Result<Option<[f64; 2]>, RunError>;
    fn landing(
        &mut self,
        frame: &Arc<BgrFrame>,
        sequence: u64,
        acquired: f64,
        player: [f64; 2],
        journal: [f64; 2],
    ) -> Result<Option<[f64; 2]>, RunError>;
}
pub struct CanyonRequest<'a> {
    pub policy: &'a CharacterSurvivalPolicy,
    pub kill_confirmed: bool,
    pub loot_complete: bool,
    pub buff_expires_at: f64,
    pub reference_seconds: f64,
    pub movement_key: &'a str,
}
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Journal,
    Dismiss,
    Portal,
    Arrival,
}
struct Click {
    aim: DesktopPoint,
    at: f64,
}
fn guard(g: &impl StepEnvironment, deadline: f64) -> Result<(), RunError> {
    if g.stopped() || !g.focused()? {
        return Err(RunError::Runtime(
            "canyon finish interrupted or unfocused".into(),
        ));
    }
    if g.clock() >= deadline {
        return Err(RunError::Runtime("canyon finish deadline reached".into()));
    }
    Ok(())
}
fn click_world(
    backend: &impl CanyonBackend,
    g: &impl StepEnvironment,
    env: &impl CanyonEnvironment,
    pending: &mut Option<Click>,
    point: [f64; 2],
    acquired: f64,
    deadline: f64,
) -> Result<bool, RunError> {
    if !(24.0..1900.0).contains(&point[0]) || !(55.0..900.0).contains(&point[1]) {
        return Err(RunError::Runtime(
            "canyon finish target outside world".into(),
        ));
    }
    guard(g, deadline)?;
    let aim = env.frame_to_desktop(point)?;
    if pending
        .as_ref()
        .is_none_or(|p| (0..2).any(|i| p.aim[i].abs_diff(aim[i]) > 3))
    {
        backend.move_pointer(aim)?;
        *pending = Some(Click { aim, at: g.clock() });
        return Ok(false);
    }
    guard(g, deadline)?;
    if !(0.0..0.25).contains(&(g.clock() - acquired)) || !backend.pointer_matches(aim)? {
        return Err(RunError::Runtime(
            "canyon finish screen or pointer stale".into(),
        ));
    }
    if pending.as_ref().is_some_and(|p| g.clock() - p.at < 0.05) {
        return Ok(false);
    }
    backend.click()?;
    *pending = None;
    Ok(true)
}
pub fn enter_canyon(
    backend: &impl CanyonBackend,
    g: &impl StepEnvironment,
    env: &mut impl CanyonEnvironment,
    vision: &mut impl CanyonVision,
    request: CanyonRequest<'_>,
    evidence: &mut Vec<EncounterEvidence>,
) -> Result<Value, RunError> {
    if !request.kill_confirmed || !request.loot_complete {
        return Err(ConfigError::value(
            "confirmed Summoner death and completed loot required before journal exit",
        )
        .into());
    }
    let mut phase = Phase::Journal;
    let mut actions = vec![];
    let (mut canyon_approaches, mut journal_approaches) = (0, 0);
    let mut last = None;
    let mut pending = None;
    let deadline = (g.clock() + 30.0).min(request.buff_expires_at);
    loop {
        guard(g, deadline)?;
        let acquired = g.clock();
        let frame = env.capture()?;
        let item = EncounterEvidence {
            filename: "journal-exit-last.png".into(),
            frame: frame.clone(),
        };
        if let Some(at) = last {
            evidence[at] = item;
        } else {
            last = Some(evidence.len());
            evidence.push(item);
        }
        let reading = env.hud(&frame)?;
        if let Some(blocked) = resource_guard(&reading, request.policy) {
            if phase == Phase::Arrival && blocked == "observation_unverified" {
                g.wait(0.04)?;
                continue;
            }
            return Err(RunError::Runtime(blocked.into()));
        }
        vision.town_frame(&frame)?;
        if vision.find("area_canyon")? && vision.find("hud")? {
            if vision.find("world_canyon_waypoint")? {
                evidence.push(EncounterEvidence {
                    filename: "canyon-arrival.png".into(),
                    frame,
                });
                return Ok(json!({"status":"canyon_waypoint_observed","actions":actions}));
            }
            let point = vision
                .steps(&frame)?
                .filter(|_| canyon_approaches < 2)
                .ok_or_else(|| RunError::Runtime("Canyon waypoint approach unverified".into()))?;
            if !(0.0..0.25).contains(&(g.clock() - acquired)) {
                return Err(RunError::Runtime("canyon finish screen stale".into()));
            }
            guard(g, deadline)?;
            if click_world(backend, g, env, &mut pending, point, acquired, deadline)? {
                actions.push(json!({"reason":"Approach current Canyon waypoint stairs","point":point,"acquired_at":acquired}));
                evidence.push(EncounterEvidence {
                    filename: format!("canyon-approach-{canyon_approaches:02}.png"),
                    frame,
                });
                canyon_approaches += 1;
                g.wait(0.4)?;
            } else {
                g.wait(0.04)?;
            }
            continue;
        }
        if vision.find("save_exit")? || vision.find("waypoint_header")? {
            return Err(RunError::Runtime(
                "unexpected panel during journal exit".into(),
            ));
        }
        let mut point = None;
        let mut reason = "";
        let mut next = phase;
        let arcane = vision.find("area_arcane")?;
        let portal = if matches!(phase, Phase::Journal | Phase::Portal) && arcane {
            vision.portal(&frame)?
        } else {
            None
        };
        if let Some(portal) = portal {
            point = Some(portal);
            reason = "Enter current red portal to Canyon of the Magi";
            next = Phase::Arrival;
        } else if phase == Phase::Journal && arcane {
            if let Some(journal) = vision.journal(&frame)? {
                if vision.prepares_ground() {
                    let focus = g.focused()?;
                    vision.prepare(&frame, acquired, &reading, focus)?;
                }
                let player = vision.ground(&frame, acquired)?.ok_or_else(|| {
                    RunError::Runtime("journal approach ground unverified".into())
                })?;
                if (journal[0] - player[0]).hypot(journal[1] - player[1]) > 220.0 {
                    if journal_approaches >= 2 {
                        return Err(RunError::Runtime("journal approach limit reached".into()));
                    }
                    let landing = vision
                        .landing(&frame, actions.len() as u64, acquired, player, journal)?
                        .ok_or_else(|| {
                            RunError::Runtime("journal approach floor unverified".into())
                        })?;
                    let aim = env.frame_to_desktop(landing)?;
                    let mut verify = || Ok(backend.pointer_matches(aim)?);
                    if dispatch_planned_step(
                        backend,
                        g,
                        StepRequest {
                            aim,
                            key: request.movement_key,
                            captured_at: acquired,
                            expires_at: Some(deadline),
                        },
                        DispatchOptions {
                            verify_aim: Some(&mut verify),
                            preparation: None,
                        },
                        &mut DispatchDiagnostics::default(),
                    )? {
                        journal_approaches += 1;
                        actions.push(json!({"reason":"Teleport near current Horazon journal","point":landing,"acquired_at":acquired}));
                        evidence.push(EncounterEvidence {
                            filename: format!("journal-approach-{journal_approaches:02}.png"),
                            frame,
                        });
                        env.emit("SUMMONER_FINISH: Teleport near current Horazon journal");
                        g.wait(request.reference_seconds)?;
                    }
                    continue;
                }
                point = Some(journal);
                reason = "Click current Horazon journal";
                next = Phase::Dismiss;
            }
        } else if phase == Phase::Dismiss {
            if vision.prepares_ground() {
                let focus = g.focused()?;
                vision.prepare(&frame, acquired, &reading, focus)?;
            }
            if let Some(player) = vision.ground(&frame, acquired)? {
                point = Some([player[0] - 45.0, player[1] + 30.0]);
                reason = "Click nearby floor immediately after journal";
                next = Phase::Portal;
            }
        }
        if !(0.0..0.25).contains(&(g.clock() - acquired)) {
            return Err(RunError::Runtime("canyon finish screen stale".into()));
        }
        if let Some(point) = point
            && click_world(backend, g, env, &mut pending, point, acquired, deadline)?
        {
            phase = next;
            actions.push(json!({"reason":reason,"point":point,"acquired_at":acquired}));
            evidence.push(EncounterEvidence {
                filename: format!("journal-exit-{:02}.png", actions.len()),
                frame,
            });
            env.emit(&format!("SUMMONER_FINISH: {reason}"));
        }
        g.wait(0.04)?;
    }
}
