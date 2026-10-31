use super::*;
use crate::runs::summoner::navigation_execution::{
    DispatchDiagnostics, DispatchOptions, PointerPreparation, StepEnvironment, StepRequest,
    dispatch_planned_step,
};
impl State {
    fn sync_landing(&mut self) -> Result<(), RunError> {
        let value = serde_json::to_value(&self.hub_landing)
            .map_err(|e| RunError::Runtime(e.to_string()))?;
        if let Some(progress) = self.progress.as_object_mut() {
            progress.remove("hub_approach_origin");
            progress.remove("hub_approach_landing");
            if let Some(fields) = value.as_object() {
                progress.extend(fields.clone());
            }
        }
        Ok(())
    }
    fn hide_map<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        f: &Frame<V::View>,
        kind: &'static str,
    ) -> Result<(), RunError> {
        c.inputs.inputs().backend().press("tab")?;
        self.map_hidden = true;
        self.map_kind = Some(kind);
        let mut inspection = json!({"status":"hide_requested","requested_at":c.now()});
        if kind == "terminal" {
            inspection["reference_observed_at"] = json!(c.vision.terminal_reference().1);
        }
        self.progress[format!("{kind}_map_inspection")] = inspection;
        self.retain(format!("{kind}-map-before.png"), &f.frame);
        self.approach_pointer = None;
        if kind == "terminal" {
            c.vision.relocalize_terminal();
            self.terminal_arrival.reset();
            self.terminal_missing_since = None;
        } else {
            c.vision.reset_hub();
            self.hub_missing_since = None;
            self.hub_near_wait = false;
        }
        c.vision.reset_motion()?;
        c.vision.clear_steering_aim();
        self.last_aim = None;
        self.action(&format!("{kind}_map_hide"));
        Ok(())
    }
    fn may_hide_map<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &self,
        c: &mut Context<'_, B, E, V>,
        f: &Frame<V::View>,
        kind: &str,
    ) -> Result<bool, RunError> {
        Ok(c.env.has_map()
            && !truth(&self.progress[format!("{kind}_map_inspection")])
            && self.attack_started.is_none()
            && !f.close
            && f.measured.displacement.is_some_and(|d| norm(d) < 2.0)
            && c.env.map_visible(&f.frame)?
            && !c.env.panels_open(&f.frame)?
            && !c.stopped()
            && c.focused()?
            && fresh(c.now(), f.acquired)
            && c.now() < c.deadline)
    }
    fn approach_deadline<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &self,
        c: &Context<'_, B, E, V>,
        kind: &str,
    ) -> Result<f64, RunError> {
        if !self.map_hidden {
            return Ok(c.deadline);
        }
        Ok(c.deadline.min(
            number(&self.progress[format!("{kind}_map_inspection")]["requested_at"])
                .ok_or_else(|| RunError::Runtime("map inspection time missing".into()))?
                + 1.5,
        ))
    }
    pub(super) fn approach<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        f: &mut Frame<V::View>,
    ) -> Result<Option<Flow>, RunError> {
        if c.request.return_from.is_some() && truth(&self.progress["hub_acquired"]) {
            self.stationary_since = None;
            c.release()?;
            if self.hub_cast_at.is_some() {
                let moved = f.measured.displacement.is_some_and(|d| norm(d) >= 2.0);
                let landmark = if !moved && let Some(hub) = f.landmarks.hub {
                    let result = self.hub_landing.update(
                        hub,
                        f.player,
                        f.landmarks.hub_evidence.as_ref(),
                        f.measured.displacement,
                        f.acquired,
                        (self.offset + f.sequence as usize - 1) as u64,
                    );
                    self.sync_landing()?;
                    result
                } else {
                    false
                };
                if moved || landmark {
                    self.record(
                        "hub_landing_confirmed",
                        if moved {
                            "current_background_displacement"
                        } else {
                            "current_waypoint_relative_displacement"
                        },
                    );
                    self.hub_cast_at = None;
                    if let Some(p) = self.progress.as_object_mut() {
                        p.remove("hub_approach_cast_at");
                    }
                    self.hub_landing = HubLandingTracker::default();
                    self.sync_landing()?;
                }
            }
            let Some(hub) = f.landmarks.hub else {
                self.hub_arrival.reset();
                self.approach_settle.reset();
                let since = *self.hub_missing_since.get_or_insert(f.now);
                self.action("central_reacquire_hold");
                if (self.last_aim.is_some() || self.approach_pointer.is_some())
                    && !truth(&self.progress["hub_hover_cleared"])
                    && !c.stopped()
                    && c.focused()?
                {
                    c.clear_pointer()?;
                    self.approach_pointer = None;
                    self.progress["hub_hover_cleared"] = json!(true);
                    self.action("central_reacquire_pointer_clear");
                    return Ok(Some(Flow::Next));
                }
                let pair = c.vision.current_pair_near(&f.frame, f.player)?;
                let stationary = f.measured.displacement.is_some_and(|d| norm(d) < 2.0);
                if self.may_hide_map(c, f, "central")? {
                    self.hide_map(c, f, "central")?;
                    return Ok(Some(Flow::Next));
                }
                let pair_wait = pair.is_some() && stationary;
                self.hub_near_wait = stationary && (self.hub_near_wait || pair_wait);
                self.record("current_near_blue_pair", pair.filter(|_| pair_wait));
                let limit = if self.hub_near_wait { 6.0 } else { 1.5 };
                self.record("central_reacquisition_limit", limit);
                if f.now - since >= limit {
                    return Ok(Some(self.pause("central_reacquisition_required")));
                }
                return Ok(Some(Flow::Next));
            };
            self.hub_missing_since = None;
            self.hub_near_wait = false;
            self.progress["hub_hover_cleared"] = json!(false);
            let distance = norm(delta(hub.map(f64::from), f.player.map(f64::from)));
            if distance <= 100.0 {
                self.approach_settle.reset();
                self.action("central_arrival_hold");
                let arrived = self
                    .hub_arrival
                    .update(true, f.measured.displacement, f.now)?;
                self.record(
                    "arrival_hold_seconds",
                    self.hub_arrival.since().map(|at| f.now - at),
                );
                if arrived {
                    self.status = "hub_arrived".into();
                    self.reason = "central_waypoint_and_player_settled".into();
                    self.retain("hub-arrived.png", &f.frame);
                    return Ok(Some(Flow::Done));
                }
                return Ok(Some(Flow::Next));
            }
            self.hub_arrival.reset();
            if let Some(cast) = self.hub_cast_at {
                self.action("central_approach_landing_hold");
                if f.now - cast >= 1.5 {
                    return Ok(Some(self.pause("central_approach_landing_unconfirmed")));
                }
                return Ok(Some(Flow::Next));
            }
            self.action("central_approach_settle");
            if !self
                .approach_settle
                .update(true, f.measured.displacement, f.now)?
            {
                return Ok(Some(Flow::Next));
            }
            f.choice = c.vision.choose(
                &f.view,
                c.route_wing,
                ChooseOptions {
                    destination: Some(delta(hub.map(f64::from), f.player.map(f64::from))),
                    ..Default::default()
                },
            )?;
            self.record("choice", &f.choice);
            let remaining = f
                .choice
                .aim
                .map(|aim| norm(delta(aim.map(f64::from), hub.map(f64::from))));
            let limit =
                100.0_f64.max(distance - 200.0) + c.vision.cell_pixels(&f.view) / 2.0_f64.sqrt();
            if remaining.is_none_or(|remaining| remaining >= distance || remaining > limit) {
                if !self.hub_floor_pointer_cleared
                    && !c.stopped()
                    && c.focused()?
                    && fresh(c.now(), f.acquired)
                {
                    c.clear_pointer()?;
                    self.hub_floor_pointer_cleared = true;
                    self.action("central_floor_pointer_clear");
                    return Ok(Some(Flow::Next));
                }
                return Ok(Some(self.pause("central_approach_floor_unverified")));
            }
            let aim = c.env.frame_to_desktop(
                f.choice
                    .aim
                    .ok_or_else(|| RunError::Runtime("central aim missing".into()))?,
            )?;
            let (sent, diagnostics) = dispatch(
                c,
                aim,
                f.acquired,
                self.approach_deadline(c, "central")?,
                None,
            )?;
            self.record("approach_dispatch", &diagnostics);
            self.action(&if sent {
                "central_approach_cast".into()
            } else {
                format!("central_approach_{}", diagnostics.reason)
            });
            self.travel_frames += u64::from(sent);
            if sent {
                self.hub_cast_at = Some(c.now());
                self.progress["hub_approach_cast_at"] = json!(self.hub_cast_at);
                self.hub_landing = HubLandingTracker::default();
                if matches!(f.landmarks.hub_evidence, Some(HubEvidence::Current { .. })) {
                    self.hub_landing.begin(hub, f.player, f.acquired)?;
                }
                self.sync_landing()?;
                self.hub_floor_pointer_cleared = false;
                self.approach_settle.reset();
            }
            return Ok(Some(Flow::Next));
        }
        self.hub_arrival.reset();
        let valid = valid_terminal(f.landmarks.terminal.as_ref());
        let target = if valid {
            f.landmarks.terminal.as_ref()
        } else {
            f.landmarks.approach.as_ref()
        };
        let distance = target.map(|t| norm(delta(t.point, f.player.map(f64::from))));
        self.record("terminal_target_distance", distance);
        if c.request.return_from.is_none() && truth(&self.progress["terminal_acquired"]) {
            c.release()?;
            self.stationary_since = None;
            let radius = if valid { 400.0 } else { 200.0 };
            if target.is_none() || (!valid && distance.is_some_and(|d| d <= radius)) {
                self.terminal_arrival.reset();
                self.approach_settle.reset();
                let since = *self.terminal_missing_since.get_or_insert(f.now);
                self.action("terminal_reacquire_hold");
                if self.may_hide_map(c, f, "terminal")? {
                    self.hide_map(c, f, "terminal")?;
                    return Ok(Some(Flow::Next));
                }
                let limit = if f.measured.displacement.is_some_and(|d| norm(d) < 2.0) {
                    6.0
                } else {
                    1.5
                };
                self.record("terminal_reacquisition_limit", limit);
                if f.now - since >= limit {
                    return Ok(Some(self.pause("terminal_reacquisition_required")));
                }
                return Ok(Some(Flow::Next));
            }
            self.terminal_missing_since = None;
            if let (Some(target), Some(distance)) = (target, distance)
                && distance > radius
            {
                self.terminal_arrival.reset();
                self.action("terminal_approach_settle");
                let displacement = f
                    .measured
                    .displacement
                    .or(f.landmarks.approach_displacement);
                let mut ready = self.approach_settle.update(true, displacement, f.now)?;
                let last_cast = c
                    .snapshot()?
                    .last_dispatch_at
                    .filter(|v| *v != 0.0)
                    .unwrap_or(c.started)
                    .max(
                        self.terminal_cast_at
                            .filter(|v| *v != 0.0)
                            .unwrap_or(c.started),
                    );
                if !ready
                    && f.measured.displacement.is_none()
                    && f.landmarks.approach_static_pairs >= 2
                    && f.now - last_cast >= c.request.reference_seconds
                {
                    ready = true;
                    self.record(
                        "terminal_approach_ready_source",
                        "three_current_static_parts_after_cast_reference",
                    );
                }
                if !ready {
                    return Ok(Some(Flow::Next));
                }
                let prepared = self
                    .approach_pointer
                    .map(|p| c.env.desktop_to_frame(p.aim))
                    .transpose()?;
                let limit = radius.max(distance - 200.0);
                let floor = match prepared {
                    Some(aim) => c.vision.floor_supports(&f.view, aim)?,
                    None => false,
                };
                let prepared_distance =
                    prepared.map(|p| norm(delta(p.map(f64::from), target.point)));
                self.record("terminal_prepared_aim", prepared);
                self.record("terminal_prepared_floor", floor);
                self.record("terminal_prepared_target_distance", prepared_distance);
                let approach = if floor && prepared_distance.is_some_and(|d| d <= limit) {
                    PathChoice {
                        aim: prepared,
                        reason: "current_terminal_prepared_floor".into(),
                        path: f.choice.path.clone(),
                    }
                } else {
                    c.vision.choose(
                        &f.view,
                        c.route_wing,
                        ChooseOptions {
                            destination: Some(delta(target.point, f.player.map(f64::from))),
                            ..Default::default()
                        },
                    )?
                };
                self.record("terminal_approach_choice", &approach);
                if approach
                    .aim
                    .is_none_or(|aim| norm(delta(aim.map(f64::from), target.point)) > limit)
                {
                    return Ok(Some(self.pause("terminal_approach_floor_unverified")));
                }
                let same = self.map_hidden && (0.0..0.20).contains(&(c.now() - f.acquired));
                if same {
                    self.approach_pointer = None;
                }
                let aim = c.env.frame_to_desktop(
                    approach
                        .aim
                        .ok_or_else(|| RunError::Runtime("terminal aim missing".into()))?,
                )?;
                let deadline = self.approach_deadline(c, "terminal")?;
                let (sent, diagnostics) = dispatch(
                    c,
                    aim,
                    f.acquired,
                    deadline,
                    if same {
                        None
                    } else {
                        Some(&mut self.approach_pointer)
                    },
                )?;
                let mut record = serde_json::to_value(&diagnostics)
                    .map_err(|e| RunError::Runtime(e.to_string()))?;
                record["pointer_mode"] = json!(if same { "same_frame" } else { "next_frame" });
                record["expires_at"] = json!(deadline);
                self.record("approach_dispatch", record);
                self.action(&if sent {
                    "terminal_approach_cast".into()
                } else {
                    format!("terminal_approach_{}", diagnostics.reason)
                });
                self.travel_frames += u64::from(sent);
                if sent {
                    self.terminal_cast_at = Some(c.now());
                    self.approach_settle.reset();
                } else if diagnostics.reason == "pointer_settling" {
                    let at = self
                        .approach_pointer
                        .ok_or_else(|| RunError::Runtime("pointer preparation missing".into()))?
                        .requested_at
                        + 0.05;
                    let started = c.now();
                    let remaining = at.min(deadline) - started;
                    if remaining > 0.0 {
                        c.env.wait(remaining)?;
                    }
                    self.record("approach_capture_ready_at", at);
                    self.record("approach_capture_wait_seconds", c.now() - started);
                }
                return Ok(Some(Flow::Next));
            }
        }
        self.terminal_arrival.reset();
        Ok(None)
    }
}
struct Step<'a, B: FieldInputBackend, E, V>(&'a Context<'a, B, E, V>);
impl<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision> StepEnvironment
    for Step<'_, B, E, V>
{
    fn clock(&self) -> f64 {
        self.0.now()
    }
    fn stopped(&self) -> bool {
        self.0.stopped()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        self.0.focused()
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        self.0.env.wait(seconds)
    }
}
fn dispatch<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
    c: &Context<'_, B, E, V>,
    aim: DesktopPoint,
    acquired: f64,
    deadline: f64,
    preparation: Option<&mut Option<PointerPreparation>>,
) -> Result<(bool, DispatchDiagnostics), RunError> {
    let mut diagnostics = DispatchDiagnostics::default();
    let backend = c.inputs.inputs().backend();
    let sent = dispatch_planned_step(
        backend,
        &Step(c),
        StepRequest {
            aim,
            key: &c.inputs.inputs().keys()[0],
            captured_at: acquired,
            expires_at: Some(deadline),
        },
        DispatchOptions {
            verify_aim: Some(&mut || backend.pointer_matches(aim).map_err(RunError::from)),
            preparation,
        },
        &mut diagnostics,
    )?;
    Ok((sent, diagnostics))
}
