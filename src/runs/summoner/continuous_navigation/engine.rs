use super::*;
use crate::common::{
    field_control::{FieldAction, FieldDirective, HeldInputSnapshot, InputObservation},
    field_preparation::validate_travel_permit,
    survival_vision::resource_guard,
};
use std::collections::VecDeque;
mod approaches;
mod movement;
#[cfg(test)]
mod tests;
enum Flow {
    Next,
    Done,
}
#[derive(Default, serde::Serialize)]
struct FrameStages {
    requested_at: Option<f64>,
    capture_sequence: Option<u64>,
    capture_method: Option<&'static str>,
    capture_started_at: Option<f64>,
    capture_completed_at: Option<f64>,
    hud_completed_at: Option<f64>,
    panels_started_at: Option<f64>,
    panels_observed_at: Option<f64>,
    panels_completed_at: Option<f64>,
    ground_completed_at: Option<f64>,
    near_completed_at: Option<f64>,
    hover_completed_at: Option<f64>,
    threats_completed_at: Option<f64>,
    planning_submitted_at: Option<f64>,
    objective_joined_at: Option<f64>,
    path_joined_at: Option<f64>,
    terminal_joined_at: Option<f64>,
    motion_completed_at: Option<f64>,
}
struct Context<'a, B: FieldInputBackend, E, V> {
    inputs: &'a FieldInputSession<B>,
    env: &'a mut E,
    vision: &'a mut V,
    request: &'a ContinuousRequest<'a>,
    route_wing: Wing,
    started: f64,
    deadline: f64,
}
impl<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision> Context<'_, B, E, V> {
    fn now(&self) -> f64 {
        self.inputs.clock().now()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        self.inputs.focused()
    }
    fn stopped(&self) -> bool {
        self.inputs.stop().is_set()
    }
    fn snapshot(&self) -> Result<HeldInputSnapshot, RunError> {
        Ok(self.inputs.inputs().snapshot()?)
    }
    fn dispatches(&self, index: usize) -> Result<u64, RunError> {
        Ok(self
            .snapshot()?
            .dispatches
            .get(&self.inputs.inputs().keys()[index])
            .copied()
            .unwrap_or(0))
    }
    fn release(&self) -> Result<(), RunError> {
        Ok(self.inputs.inputs().release(false)?)
    }
    fn release_movement(&self) -> Result<(), RunError> {
        let s = self.snapshot()?;
        let key = &self.inputs.inputs().keys()[0];
        if s.held.contains(key) || s.pending.as_ref() == Some(key) {
            self.release()?;
        }
        Ok(())
    }
    fn combat(&self, reason: &str, acquired: f64, sequence: u64) -> Result<(), RunError> {
        let now = self.now();
        Ok(self.inputs.inputs().apply_observed(
            &FieldDirective::new(
                FieldAction::Combat,
                reason,
                None,
                None,
                Some((acquired + 0.25).min(self.deadline)),
                false,
            )?,
            now,
            InputObservation::new(sequence, acquired, now)?,
        )?)
    }
    fn clear_pointer(&self) -> Result<(), RunError> {
        Ok(self
            .inputs
            .inputs()
            .backend()
            .move_pointer(self.env.frame_to_desktop([961, 24])?)?)
    }
}
struct Frame<V> {
    sequence: u64,
    acquired: f64,
    frame: Arc<BgrFrame>,
    player: [i32; 2],
    view: V,
    choice: PathChoice,
    threats: CurrentThreats,
    landmarks: TrackedLandmarks,
    measured: CameraMotion,
    landings: Vec<TerrainLanding>,
    close: bool,
    probe_confirmed: bool,
    probe_pending: bool,
    direct_hover_near: bool,
    now: f64,
}
struct PositionObservation<'a, V> {
    frame: &'a Arc<BgrFrame>,
    view: &'a V,
    player: [i32; 2],
    measured: &'a CameraMotion,
    input_before: &'a HeldInputSnapshot,
    acquired: f64,
    sequence: u64,
}
struct State {
    progress: Value,
    row: Option<Value>,
    evidence: Vec<EncounterEvidence>,
    streamed_files: Vec<String>,
    offset: usize,
    first: Option<Arc<BgrFrame>>,
    last: Option<Arc<BgrFrame>>,
    matched: Option<Arc<BgrFrame>>,
    last_aim: Option<[i32; 2]>,
    comparison: Option<Vec<u8>>,
    last_changed_at: f64,
    last_changed_casts: u64,
    stationary_casts: u64,
    attack_started: Option<f64>,
    quiet_frames: u64,
    combat_baseline: u64,
    position_current: bool,
    status: String,
    reason: String,
    travel_frames: u64,
    unavailable_since: Option<f64>,
    aim_unavailable_since: Option<f64>,
    stationary_since: Option<f64>,
    accepted_detour: Option<[f64; 2]>,
    recent_turns: VecDeque<EncounterEvidence>,
    blocked: VecDeque<[f64; 4]>,
    recoveries: u64,
    hub_arrival: ArrivalDwell,
    terminal_arrival: ArrivalDwell,
    approach_settle: ArrivalDwell,
    hub_missing_since: Option<f64>,
    terminal_missing_since: Option<f64>,
    departure_pointer_cleared: bool,
    approach_pointer: Option<super::super::navigation_execution::PointerPreparation>,
    terminal_cast_at: Option<f64>,
    hub_near_wait: bool,
    hub_floor_pointer_cleared: bool,
    hub_cast_at: Option<f64>,
    hub_landing: HubLandingTracker,
    map_hidden: bool,
    map_kind: Option<&'static str>,
    motion_missing_since: Option<f64>,
    motion_evidence: usize,
}
impl State {
    fn new(progress: Value, reference: f64, started: f64) -> Result<Self, RunError> {
        let progress = if progress.is_null() {
            json!({"steps":[],"position":[0.0,0.0],"breadcrumbs":[[0.0,0.0]]})
        } else {
            progress
        };
        if !progress.is_object() {
            return Err(crate::common::config::ConfigError::value(
                "navigation progress must be an object",
            )
            .into());
        }
        let mut state = Self {
            offset: progress["continuous_frames"].as_array().map_or(0, Vec::len),
            last_aim: pixel(&progress["last_aim"]),
            position_current: progress["position_current"].as_bool().unwrap_or(true),
            accepted_detour: pair(&progress["route_detour"]),
            hub_cast_at: number(&progress["hub_approach_cast_at"]),
            progress,
            row: None,
            evidence: vec![],
            streamed_files: vec![],
            first: None,
            last: None,
            matched: None,
            comparison: None,
            last_changed_at: started,
            last_changed_casts: 0,
            stationary_casts: 0,
            attack_started: None,
            quiet_frames: 0,
            combat_baseline: 0,
            status: "north_segment_observed".into(),
            reason: "interval_complete".into(),
            travel_frames: 0,
            unavailable_since: None,
            aim_unavailable_since: None,
            stationary_since: None,
            recent_turns: VecDeque::new(),
            blocked: VecDeque::new(),
            recoveries: 0,
            hub_arrival: ArrivalDwell::new(reference)?,
            terminal_arrival: ArrivalDwell::new(reference)?,
            approach_settle: ArrivalDwell::new(reference)?,
            hub_missing_since: None,
            terminal_missing_since: None,
            departure_pointer_cleared: false,
            approach_pointer: None,
            terminal_cast_at: None,
            hub_near_wait: false,
            hub_floor_pointer_cleared: false,
            hub_landing: HubLandingTracker::default(),
            map_hidden: false,
            map_kind: None,
            motion_missing_since: None,
            motion_evidence: 0,
        };
        if state.progress.get("continuous_frames").is_none() {
            state.progress["continuous_frames"] = json!([]);
        }
        // 파일 목록은 이번 interval의 제출만 가리킨다. 이전 interval의 이름을 재사용하지 않는다.
        if let Some(progress) = state.progress.as_object_mut() {
            progress.remove("streamed_evidence_files");
        }
        state.hub_landing = serde_json::from_value(state.progress.clone())
            .map_err(|e| RunError::Runtime(format!("invalid current hub landing: {e}")))?;
        Ok(state)
    }
    fn record(&mut self, key: &str, value: impl serde::Serialize) {
        if let Some(row) = &mut self.row {
            row[key] = json!(value);
        }
    }
    fn action(&mut self, value: &str) {
        self.record("action", value);
    }
    fn flush_row(&mut self) -> Result<(), RunError> {
        if let Some(row) = self.row.take() {
            self.progress["continuous_frames"]
                .as_array_mut()
                .ok_or_else(|| RunError::Runtime("continuous frames must be an array".into()))?
                .push(row);
        }
        Ok(())
    }
    fn previous_row(&self) -> Option<&Value> {
        self.progress["continuous_frames"]
            .as_array()
            .and_then(|r| r.last())
    }
    fn retain(&mut self, name: impl Into<String>, frame: &Arc<BgrFrame>) {
        self.evidence.push(EncounterEvidence {
            filename: name.into(),
            frame: frame.clone(),
        });
    }
    fn pause(&mut self, reason: impl Into<String>) -> Flow {
        self.status = "field_paused".into();
        self.reason = reason.into();
        Flow::Done
    }
    fn position(&self) -> [f64; 2] {
        pair(&self.progress["position"]).unwrap_or([0.0; 2])
    }
    fn defend_near<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        sequence: u64,
    ) -> Result<Option<f64>, RunError> {
        if c.stopped() || !c.focused()? {
            return Ok(None);
        }
        let ready = c.now();
        if !fresh(ready, acquired) || ready >= c.deadline {
            return Ok(None);
        }
        if self.attack_started.is_none() {
            self.attack_started = Some(ready);
            self.approach_pointer = None;
            self.terminal_missing_since = None;
            self.hub_missing_since = None;
            self.combat_baseline = c.dispatches(1)?;
            c.env.emit(
                "NORTH_COMBAT: current nearby hostile -> release F2 -> F4 before route analysis",
            );
            self.retain(
                format!(
                    "continuous-{:04}-combat.png",
                    self.offset + sequence as usize
                ),
                frame,
            );
        }
        c.combat("current_near_hostile", acquired, sequence)?;
        let requested = c.now();
        let timing = json!({"acquired_at":acquired,"requested_at":requested,"player":player});
        if self.progress.get("first_nearby_defense").is_none() {
            self.progress["first_nearby_defense"] = timing.clone();
        }
        self.progress["last_nearby_defense"] = timing;
        Ok(Some(requested))
    }
    fn breadcrumbs(&self) -> Vec<[f64; 2]> {
        self.progress["breadcrumbs"]
            .as_array()
            .map(|v| v.iter().filter_map(pair).collect())
            .unwrap_or_default()
    }
    fn blocked(&self) -> Vec<[f64; 4]> {
        self.blocked.iter().copied().collect()
    }
    fn exclude(&mut self, aim: [i32; 2], clearance: f64) {
        let [x, y] = aim.map(f64::from);
        let r = clearance * 2.0;
        self.blocked.push_back([x - r, y - r, x + r, y + r]);
        while self.blocked.len() > 8 {
            self.blocked.pop_front();
        }
    }
    fn reset_dwell<V: ContinuousVision>(&mut self, vision: &mut V) {
        self.stationary_since = None;
        self.hub_arrival.reset();
        self.terminal_arrival.reset();
        vision.reset_terminal();
        vision.reset_hub();
    }
}
fn merge(primary: Result<(), RunError>, cleanup: Result<(), RunError>) -> Result<(), RunError> {
    match (primary, cleanup) {
        (Err(p), Err(c)) => Err(p.with_cleanup(c)),
        (Err(e), _) | (_, Err(e)) => Err(e),
        _ => Ok(()),
    }
}
pub fn run_continuous_route<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    vision: &mut V,
    request: ContinuousRequest<'_>,
    progress: &mut Value,
    evidence: &mut Vec<EncounterEvidence>,
) -> Result<Value, RunError> {
    execute(
        inputs,
        env,
        vision,
        request,
        progress,
        evidence,
        |inputs, deadline| inputs.start("summoner-held-input-watchdog", deadline),
    )
}
fn execute<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
    inputs: &mut FieldInputSession<B>,
    env: &mut E,
    vision: &mut V,
    request: ContinuousRequest<'_>,
    progress: &mut Value,
    evidence: &mut Vec<EncounterEvidence>,
    start: impl FnOnce(&mut FieldInputSession<B>, f64) -> Result<(), ControlError>,
) -> Result<Value, RunError> {
    use crate::common::config::ConfigError;
    if !request.seconds.is_finite() || !(1.0..=60.0).contains(&request.seconds) {
        return Err(ConfigError::value(
            "continuous supervised interval must be within one to sixty seconds",
        )
        .into());
    }
    if !request.reference_seconds.is_finite()
        || !(0.0..1.5).contains(&request.reference_seconds)
        || request.reference_seconds == 0.0
    {
        return Err(ConfigError::value(
            "configured cast reference required for continuous steering",
        )
        .into());
    }
    let now = inputs.clock().now();
    match request.continuation {
        Continuation::Initial => {
            validate_travel_permit(request.proof, request.policy, env.wall_clock())?
        }
        Continuation::Current => {
            validate_live_continuation(progress, request.wing, request.return_from, now)?
        }
        Continuation::Transition(previous) => {
            if previous["status"] == "hub_arrived" {
                validate_hub_departure(previous, request.wing, request.return_from, now)?;
            } else {
                validate_terminal_return(previous, request.wing, request.return_from, now)?;
            }
        }
    }
    let started = inputs.clock().now();
    let deadline = request
        .buff_expires_at
        .map_or(started + request.seconds, |buff| {
            (started + request.seconds).min(buff)
        });
    let mut state = State::new(progress.clone(), request.reference_seconds, started)?;
    if env.reaction_radius() != NEAR_GROUND_RADIUS {
        state.progress["combat_reaction_radius"] = json!(env.reaction_radius());
    }
    let route_wing = request.return_from.map_or(request.wing, |wing| match wing {
        Wing::North => Wing::South,
        Wing::East => Wing::West,
        Wing::South => Wing::North,
        Wing::West => Wing::East,
    });
    let result = (|| {
        vision.initialize(
            request.reference_seconds,
            state.last_aim,
            state.accepted_detour,
        )?;
        start(inputs, deadline)?;
        if request.return_from.is_some()
            && matches!(request.continuation, Continuation::Initial)
            && request.proof["at_central_waypoint"] == true
        {
            state.progress["hub_acquired"] = json!(true);
        }
        let mut c = Context {
            inputs,
            env,
            vision,
            request: &request,
            route_wing,
            started,
            deadline,
        };
        for sequence in 1..=1800 {
            let iteration = state.step(&mut c, sequence);
            let flushed = state.flush_row();
            state
                .streamed_files
                .extend(c.env.drain_evidence(&mut state.evidence));
            match (iteration, flushed) {
                (Err(e), _) | (_, Err(e)) => return Err(e),
                (Ok(Flow::Done), _) => break,
                _ => {}
            }
        }
        Ok(())
    })();
    let joined = vision.finish(Duration::from_secs(1));
    let closed = inputs
        .finish(Duration::from_secs(1))
        .map_err(RunError::from);
    let mut cleanup = merge(joined, closed);
    let mut c = Context {
        inputs,
        env,
        vision,
        request: &request,
        route_wing,
        started,
        deadline,
    };
    cleanup = merge(cleanup, state.finish(&mut c));
    state
        .streamed_files
        .extend(c.env.drain_evidence(&mut state.evidence));
    if !state.streamed_files.is_empty() {
        state.progress["streamed_evidence_files"] = json!(state.streamed_files);
    }
    evidence.append(&mut state.evidence);
    *progress = state.progress.clone();
    merge(result, cleanup)?;
    if let Some(fault) = inputs.faults()?.first() {
        return Err(RunError::Runtime(format!(
            "continuous watchdog: {}",
            fault.message
        )));
    }
    if state.travel_frames == 0 && state.status == "north_segment_observed" {
        state.pause("no_valid_steering");
    }
    if let Some(p) = state.progress.as_object_mut() {
        p.remove("steering_heading");
    }
    state.progress["continuous_status"] = json!(state.reason);
    state.progress["position_current"] = json!(state.position_current);
    state.progress["wing"] = json!(request.wing);
    state.progress["return_from"] = json!(request.return_from);
    state.progress["engage_monsters"] = json!(request.engage_monsters);
    state.progress["route_detour"] = json!(state.accepted_detour);
    if let Some(summary) = vision.graph_summary() {
        state.progress["route_graph"] = summary;
    }
    state.progress["interval_finished_at"] = json!(inputs.clock().now());
    env.emit(&format!("NORTH_CONTINUOUS_STOP: {}", state.reason));
    *progress = state.progress.clone();
    let mut output = state.progress;
    output["status"] = json!(state.status);
    Ok(output)
}
impl State {
    fn step<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        sequence: u64,
    ) -> Result<Flow, RunError> {
        if c.stopped() || !c.focused()? {
            return Err(RunError::Runtime(
                "continuous navigation interrupted or unfocused".into(),
            ));
        }
        if c.env.loot_movement_allowed() == Some(false) {
            c.release()?;
            return Ok(self.pause("item_labels_not_off"));
        }
        if c.now() >= c.deadline {
            if c.request
                .buff_expires_at
                .is_some_and(|buff| c.now() >= buff)
            {
                self.pause("buff_renewal_due");
            }
            return Ok(Flow::Done);
        }
        let requested = c.now();
        let mut stages = FrameStages {
            requested_at: Some(requested),
            ..Default::default()
        };
        let input_before = c.snapshot()?;
        let frame = c.env.capture_for_control()?;
        let (capture_sequence, acquired, completed) =
            c.env
                .capture_source()
                .unwrap_or((sequence, requested, c.now()));
        stages.capture_sequence = Some(capture_sequence);
        stages.capture_method = c.env.capture_method();
        stages.capture_started_at = Some(acquired);
        stages.capture_completed_at = Some(completed);
        self.last = Some(frame.clone());
        if self.first.is_none() {
            self.first = Some(frame.clone());
        }
        c.vision.begin_frame(&frame)?;
        let hud = c.env.hud(&frame)?;
        stages.hud_completed_at = Some(c.now());
        if c.vision.prepares_ground() {
            c.vision.prepare(&frame, acquired, &hud, c.focused()?)?;
        }
        if let Some((started, observed)) = c.vision.panel_timing() {
            stages.panels_started_at = Some(started);
            stages.panels_observed_at = Some(observed);
        }
        stages.panels_completed_at = Some(c.now());
        if let Some(blocked) = resource_guard(&hud, c.request.policy) {
            c.release()?;
            self.approach_pointer = None;
            self.reset_dwell(c.vision);
            if blocked == "mercenary_unverified" {
                self.retain("mercenary-unverified.png", &frame);
                return Err(RunError::Runtime(blocked.into()));
            }
            if blocked == "resource_low" {
                if c.env.recover_resources(&frame, &hud, acquired)? {
                    c.env.reset_probe();
                    self.attack_started = None;
                    self.quiet_frames = 0;
                    self.comparison = None;
                    return Ok(Flow::Next);
                }
                return Err(RunError::Runtime(blocked.into()));
            }
            let since = *self.unavailable_since.get_or_insert(acquired);
            if c.now() - since >= 1.5 {
                return Ok(self.pause(blocked));
            }
            return Ok(Flow::Next);
        }
        if self.map_hidden && self.check_map_timeout(c, &frame, acquired)? {
            return Ok(if self.status == "field_paused" {
                Flow::Done
            } else {
                Flow::Next
            });
        }
        let ground = c.vision.ground(&frame, acquired)?;
        stages.ground_completed_at = Some(c.now());
        let mut observed_aim = None;
        let mut proposals = vec![];
        let mut probe_confirmed = false;
        let mut probe_pending = false;
        let mut direct_hover_near = false;
        let mut combat_at = None;
        let mut threats = CurrentThreats::default();
        let mut close = false;
        if let Some(player) = ground.point {
            let observed_pointer = c.snapshot()?.pointer;
            observed_aim = observed_pointer
                .map(|point| c.env.desktop_to_frame(point))
                .transpose()?
                .or(self.last_aim);
            threats = c.vision.near_threats(
                &frame,
                player,
                acquired,
                observed_aim,
                c.request.engage_monsters,
            )?;
            stages.near_completed_at = Some(c.now());
            if c.vision.near_preempts_hover()
                && has_near(&threats.candidates, player, c.env.reaction_radius())?
            {
                combat_at = self.defend_near(c, &frame, player, acquired, sequence)?;
            }
            c.vision.finish_threats(&mut threats)?;
            stages.hover_completed_at = Some(c.now());
            if c.request.engage_monsters
                && c.env.has_probe()
                && !has_near(&threats.candidates, player, c.env.reaction_radius())?
            {
                if c.env.probe_pending() || self.attack_started.is_none() {
                    proposals = c.env.actor_proposals(&frame, player)?;
                }
                if (c.env.probe_pending()
                    || (!proposals.is_empty() && c.now() >= c.env.probe_next_proposal_at()))
                    && !c.stopped()
                    && c.focused()?
                    && c.now() < c.deadline
                {
                    c.release_movement()?;
                    let target = c.env.probe(&frame, player, acquired, Some(&proposals))?;
                    probe_confirmed = target.is_some();
                    probe_pending = c.env.probe_pending() && !probe_confirmed;
                    if probe_confirmed {
                        threats.target = target;
                    }
                }
            }
            direct_hover_near = threats.target.is_some()
                && observed_pointer.is_some()
                && near_ground(
                    observed_aim.map(|p| p.map(f64::from)),
                    player.map(f64::from),
                    c.env.reaction_radius(),
                )?
                && c.inputs.inputs().backend().pointer_matches(
                    observed_pointer
                        .ok_or_else(|| RunError::Runtime("observed pointer missing".into()))?,
                )?;
            close = has_near(&threats.candidates, player, c.env.reaction_radius())?
                || probe_confirmed
                || direct_hover_near;
            if close && combat_at.is_none() {
                combat_at = self.defend_near(c, &frame, player, acquired, sequence)?;
            }
        }
        stages.threats_completed_at = Some(c.now());
        c.vision.require_current_goals(
            truth(&self.progress["terminal_acquired"]) || truth(&self.progress["hub_acquired"]),
        );
        let resumed = c.request.proof["current_wing"] == json!(c.request.wing)
            && c.request.proof["wing_phase"] == "outbound";
        c.vision.local_field_phase(
            c.request.return_from.is_none()
                && !truth(&self.progress["terminal_acquired"])
                && !truth(&self.progress["hub_acquired"])
                && (truth(&self.progress["central_departure_observed"]) || resumed),
        );
        let excluded = self.blocked();
        let PlanningJobs {
            mut jobs,
            mut hub,
            mut hub_evidence,
        } = c.vision.start_plan(
            &frame,
            PlanningRequest {
                wing: c.route_wing,
                acquired,
                position: self.position(),
                excluded: &excluded,
                ground: &ground,
                outbound: c.request.return_from.is_none(),
            },
        )?;
        stages.planning_submitted_at = Some(c.now());
        if c.vision.slow_confirmation_pending() {
            c.release_movement()?;
        }
        if c.request.return_from.is_some() && hub.is_some() {
            if !truth(&self.progress["hub_acquired"]) {
                self.retain("hub-first-acquired.png", &frame);
            }
            self.progress["hub_acquired"] = json!(true);
            c.release_movement()?;
        }
        if c.request.return_from.is_none() && truth(&self.progress["terminal_acquired"]) {
            c.release_movement()?;
        }
        let objective = c.vision.objective(&mut jobs)?;
        stages.objective_joined_at = Some(c.now());
        if let Some(objective) = objective.filter(|_| fresh(c.now(), acquired)) {
            c.release()?;
            self.row = Some(
                json!({"index":self.offset+sequence as usize-1,"acquired_at":acquired,"processed_at":c.now(),"action":"summoner_platform_hold","objective":objective,"stages":stages}),
            );
            self.progress["objective"] = json!(objective);
            self.retain("summoner-platform.png", &frame);
            self.status = "summoner_platform_observed".into();
            self.reason = objective.evidence_kind;
            return Ok(Flow::Done);
        }
        let CurrentPlan {
            mut choice,
            ground,
            mut view,
        } = match c.vision.plan(&mut jobs) {
            Ok(plan) => plan,
            Err(error) if unavailable(&error) => {
                let terminal = c.vision.terminal(&mut jobs)?;
                if terminal.is_some() && fresh(c.now(), acquired) {
                    if !truth(&self.progress["terminal_acquired"]) {
                        self.retain("terminal-first-acquired.png", &frame);
                    }
                    self.progress["terminal_acquired"] = json!(true);
                    c.release()?;
                }
                c.inputs.inputs().expire(c.now())?;
                let diagnostics = c.vision.camera_diagnostics()?;
                let failure = json!({"acquired_at":acquired,"reason":error.to_string(),"life_ratio":hud.life.ratio(),"mana_ratio":hud.mana.ratio(),"belt_visibility":hud.belt_visibility,"camera_center":diagnostics["camera_center"],"last_camera_reset":diagnostics["last_camera_reset"]});
                if self.progress.get("ground_failures").is_none() {
                    self.progress["ground_failures"] = json!([]);
                }
                let failures = self.progress["ground_failures"]
                    .as_array_mut()
                    .ok_or_else(|| RunError::Runtime("ground failures must be an array".into()))?;
                failures.push(failure.clone());
                if failures.len() > 16 {
                    failures.drain(..failures.len() - 16);
                }
                for (key, name, needed) in [
                    (
                        "first_ground_failure",
                        "continuous-ground-failure-first.png",
                        true,
                    ),
                    (
                        "first_camera_displacement",
                        "continuous-camera-displacement-first.png",
                        error.to_string() == "camera_actor_displaced",
                    ),
                ] {
                    if needed && self.progress.get(key).is_none() {
                        let mut first = failure.clone();
                        first["evidence_file"] = json!(name);
                        self.progress[key] = first;
                        self.retain(name, &frame);
                    }
                }
                self.reset_dwell(c.vision);
                let since = *self.unavailable_since.get_or_insert(acquired);
                if c.now() - since >= 1.5 {
                    c.release()?;
                    return Ok(self.pause(error.to_string()));
                }
                return Ok(Flow::Next);
            }
            Err(error) => return Err(error),
        };
        stages.path_joined_at = Some(c.now());
        let player = ground
            .point
            .ok_or_else(|| RunError::Runtime("planned ground missing".into()))?;
        if c.request.return_from.is_none()
            && c.vision.has_hub()
            && !resumed
            && !truth(&self.progress["central_departure_observed"])
            && self.attack_started.is_none()
        {
            if hub.is_none_or(|hub| norm(delta(player.map(f64::from), hub.map(f64::from))) > 100.0)
            {
                c.release()?;
                c.vision.terminal(&mut jobs)?;
                let since =
                    number(&self.progress["central_departure_wait_started_at"]).unwrap_or(acquired);
                self.progress["central_departure_wait_started_at"] = json!(since);
                self.row = Some(
                    json!({"index":self.offset+sequence as usize-1,"acquired_at":acquired,"processed_at":c.now(),"action":"outbound_central_reacquire_hold","player":ground,"hub_point":hub,"stages":stages}),
                );
                if !self.departure_pointer_cleared
                    && !c.stopped()
                    && c.focused()?
                    && fresh(c.now(), acquired)
                {
                    c.clear_pointer()?;
                    self.departure_pointer_cleared = true;
                    self.record("pointer_cleared", true);
                }
                if c.now() - since >= 1.5 {
                    return Err(RunError::Runtime(
                        "outbound_requires_central_waypoint".into(),
                    ));
                }
                return Ok(Flow::Next);
            }
            self.progress["central_departure_observed"] = json!(true);
            if let Some(p) = self.progress.as_object_mut() {
                p.remove("central_departure_wait_started_at");
            }
        }
        self.unavailable_since = None;
        c.vision.set_sequence(&mut view, sequence);
        if observed_aim.is_none()
            && let Some(aim) = choice.aim
        {
            observed_aim = Some(aim);
            threats.hover = c.vision.hover(&frame, aim)?;
            if c.request.engage_monsters {
                threats.candidates = c.vision.hazards(&frame, player, acquired, observed_aim)?;
            }
        }
        let terminal = c.vision.terminal(&mut jobs)?;
        stages.terminal_joined_at = Some(c.now());
        let valid = valid_terminal(terminal.as_ref());
        if valid && fresh(c.now(), acquired) {
            if !truth(&self.progress["terminal_acquired"]) {
                self.retain("terminal-first-acquired.png", &frame);
            }
            self.progress["terminal_acquired"] = json!(true);
            c.release_movement()?;
        }
        let preflight = c.now();
        let mut renewed = None;
        let active = c.snapshot()?;
        if let Some(pointer) = active
            .pointer
            .filter(|_| active.held == [c.inputs.inputs().keys()[0].clone()])
        {
            let aim = c.env.desktop_to_frame(pointer)?;
            close = has_near(&threats.candidates, player, c.env.reaction_radius())?
                || direct_hover_near;
            if close || !threats.hover.is_empty() {
                c.release()?;
            } else if self.attack_started.is_none()
                && Some(aim) == observed_aim
                && !truth(&self.progress["hub_acquired"])
                && !truth(&self.progress["terminal_acquired"])
                && (c.request.return_from.is_none() || hub.is_none())
                && !valid
                && !self.blocked.iter().any(|[l, t, r, b]| {
                    *l <= f64::from(aim[0])
                        && f64::from(aim[0]) < *r
                        && *t <= f64::from(aim[1])
                        && f64::from(aim[1]) < *b
                })
                && c.vision.floor_supports(&view, aim)?
                && !c.stopped()
                && c.focused()?
                && fresh(c.now(), acquired)
                && c.now() < c.deadline
                && c.inputs.inputs().renew_lease_observed(
                    &c.inputs.inputs().keys()[0],
                    pointer,
                    (acquired + 0.25).min(c.deadline),
                    c.now(),
                    InputObservation::new(sequence, acquired, preflight)?,
                )?
            {
                renewed = Some(c.now());
            }
        }
        let hint = c
            .snapshot()?
            .cast_pointer
            .map(|p| c.env.desktop_to_frame(p))
            .transpose()?
            .map(|p| delta(p.map(f64::from), player.map(f64::from)));
        let (measured, landings) = c.vision.measure(&frame, &view, hint)?;
        stages.motion_completed_at = Some(c.now());
        if measured.displacement.is_some_and(|d| norm(d) < 2.0) {
            if self.stationary_since.is_none() {
                self.stationary_casts = input_before
                    .dispatches
                    .get(&c.inputs.inputs().keys()[0])
                    .copied()
                    .unwrap_or(0);
            }
            self.stationary_since.get_or_insert(acquired);
        } else {
            self.stationary_since = None;
        }
        let landmarks = c.vision.track(LandmarkObservation {
            frame: &frame,
            terminal,
            hub,
            evidence: hub_evidence,
            acquired,
            displacement: measured.displacement,
        })?;
        hub = landmarks.hub;
        hub_evidence = landmarks.hub_evidence.clone();
        self.observe_position(
            c,
            PositionObservation {
                frame: &frame,
                view: &view,
                player,
                measured: &measured,
                input_before: &input_before,
                acquired,
                sequence,
            },
        )?;
        if !c.vision.has_graph() && self.position_current && self.breadcrumbs().len() > 1 {
            let visited = self.breadcrumbs();
            choice = c.vision.choose(
                &view,
                c.route_wing,
                ChooseOptions {
                    position: self.position(),
                    visited: &visited,
                    allow_detour: true,
                    prefer_frontier: true,
                    excluded: &self.blocked(),
                    ..Default::default()
                },
            )?;
        }
        let global = choice.clone();
        let now = c.now();
        let (detour, flow_reason) = c.vision.flow_state();
        self.row = Some(
            json!({"index":self.offset+sequence as usize-1,"acquired_at":acquired,"processed_at":now,"preflight_processed_at":preflight,"preflight_input":active,"lease_renewed_at":renewed,"ground":ground,"choice":choice,"global_choice":global,"flow_reason":flow_reason,"route_detour":detour,"motion":measured,"terminal_approach_displacement":landmarks.approach_displacement,"terminal_approach_static_pairs":landmarks.approach_static_pairs,"candidates":threats.candidates,"hover":threats.hover,"target":threats.target,"nearby_hover_confirmed":probe_confirmed,"direct_hover_near":direct_hover_near,"nearby_hover_pending":probe_pending,"nearby_actor_proposals":proposals,"combat_preflight_at":combat_at,"hub_point":hub,"hub_evidence":hub_evidence,"route_heading":c.route_wing.heading(),"engage_monsters":c.request.engage_monsters,"terminal":landmarks.terminal,"terminal_approach":landmarks.approach,"position":self.position(),"position_generation":self.progress["position_generation"].as_u64().unwrap_or(0),"position_current":self.position_current,"input_before":input_before}),
        );
        self.record("stages", &stages);
        self.record("slow_observations", c.vision.planning_diagnostics());
        if let Some(summary) = c.vision.graph_summary() {
            self.record("route_graph", summary);
            self.record("terrain_landings", &landings);
        }
        let mut current = Frame {
            sequence,
            acquired,
            frame,
            player,
            view,
            choice,
            threats,
            landmarks,
            measured,
            landings,
            close,
            probe_confirmed,
            probe_pending,
            direct_hover_near,
            now,
        };
        if !fresh(now, acquired) || now >= c.deadline {
            c.release()?;
            self.hub_arrival.reset();
            self.terminal_arrival.reset();
            self.action("stale_hold");
            return Ok(Flow::Next);
        }
        if let Some(flow) = self.terminal_arrival(c, &current)? {
            return Ok(flow);
        }
        if let Some(flow) = self.combat(c, &mut current)? {
            return Ok(flow);
        }
        if let Some(flow) = self.approach(c, &mut current)? {
            return Ok(flow);
        }
        self.movement(c, &mut current)
    }
}
fn has_near(
    candidates: &[VisualCandidate],
    player: [i32; 2],
    radius: [f64; 2],
) -> Result<bool, RunError> {
    for candidate in candidates {
        if near_ground(candidate.ground(), player.map(f64::from), radius)? {
            return Ok(true);
        }
    }
    Ok(false)
}

impl State {
    fn restore_map<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
    ) -> Result<(), RunError> {
        let kind = self
            .map_kind
            .ok_or_else(|| RunError::Runtime("map inspection owner missing".into()))?;
        if c.stopped() || !c.focused()? {
            return Err(RunError::Runtime(
                "map restoration interrupted or unfocused".into(),
            ));
        }
        c.inputs.inputs().backend().press("tab")?;
        self.map_hidden = false;
        let restoration = c.env.confirm_map_restored()?;
        self.progress[format!("{kind}_map_inspection")]["restoration"] = restoration;
        self.progress[format!("{kind}_map_inspection")]["status"] = json!("on_observed");
        self.progress[format!("{kind}_map_inspection")]["restored_at"] = json!(c.now());
        Ok(())
    }
    fn check_map_timeout<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        frame: &Arc<BgrFrame>,
        acquired: f64,
    ) -> Result<bool, RunError> {
        let kind = self
            .map_kind
            .ok_or_else(|| RunError::Runtime("map inspection owner missing".into()))?;
        let key = format!("{kind}_map_inspection");
        if self.progress[&key].get("first_hidden_at").is_none() {
            self.progress[&key]["first_hidden_at"] = json!(acquired);
            self.retain(format!("{kind}-map-first-hidden.png"), frame);
        }
        let at = number(&self.progress[&key]["requested_at"])
            .ok_or_else(|| RunError::Runtime("map inspection time missing".into()))?;
        if c.now() - at < 1.5 {
            return Ok(false);
        }
        c.release()?;
        if let Err(error) = self.restore_map(c) {
            if !unavailable(&error) {
                return Err(error);
            }
            self.progress[&key]["status"] = json!("restore_unverified");
            self.progress[&key]["error"] = json!(error.to_string());
            self.pause(format!("{kind}_map_restore_unverified"));
            return Ok(true);
        }
        self.progress[&key]["outcome"] = json!("timeout_then_normal_reacquisition");
        c.vision.reset_motion()?;
        if kind == "terminal" {
            c.vision.relocalize_terminal();
            self.terminal_arrival.reset();
            self.terminal_missing_since = None;
        } else {
            c.vision.reset_hub();
            self.hub_arrival.reset();
            self.hub_missing_since = None;
            self.hub_near_wait = false;
        }
        self.approach_settle.reset();
        self.approach_pointer = None;
        c.vision.clear_steering_aim();
        self.last_aim = None;
        c.env.emit(&format!(
            "{}_MAP_INSPECTION: ON restored; require fresh target in normal bounded reacquisition",
            kind.to_uppercase()
        ));
        Ok(true)
    }
    fn finish<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
    ) -> Result<(), RunError> {
        if self.map_hidden {
            let kind = self
                .map_kind
                .ok_or_else(|| RunError::Runtime("map inspection owner missing".into()))?;
            let key = format!("{kind}_map_inspection");
            if c.stopped() || !c.focused()? {
                self.progress[&key]["status"] = json!("restore_interrupted");
                self.pause(format!("{kind}_map_restore_interrupted"));
            } else {
                match self.restore_map(c) {
                    Ok(()) => {
                        if matches!(self.status.as_str(), "wing_end_observed" | "hub_arrived")
                            && self
                                .previous_row()
                                .and_then(|r| number(&r["processed_at"]))
                                .is_some_and(|at| c.now() - at >= 0.5)
                        {
                            self.pause(format!("{kind}_map_restore_delayed"));
                        }
                    }
                    Err(error) if unavailable(&error) => {
                        self.progress[&key]["status"] = json!("restore_unverified");
                        self.progress[&key]["error"] = json!(error.to_string());
                        self.pause(format!("{kind}_map_restore_unverified"));
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        let snapshot = c.snapshot()?;
        self.progress["input_dispatches"] = json!(snapshot.dispatches);
        self.progress["approach_dispatches"] = json!(self.travel_frames);
        if let Some(summary) = c.vision.graph_summary() {
            self.progress["route_graph"] = summary;
        }
        self.travel_frames += snapshot
            .dispatches
            .get(&c.inputs.inputs().keys()[0])
            .copied()
            .unwrap_or(0);
        if !c.env.evidence_streaming() {
            self.evidence.extend(self.recent_turns.drain(..));
        }
        if let Some(first) = self.first.clone() {
            self.retain(format!("continuous-{:04}-before.png", self.offset), &first);
        }
        if let Some(last) = self.last.clone() {
            let len = self.progress["continuous_frames"]
                .as_array()
                .map_or(0, Vec::len);
            self.retain(format!("continuous-{len:04}-after.png"), &last);
        }
        // 제한 큐의 마지막 두 자리는 시작/종료를 먼저 제출하고 회전 참고를 뒤에 둔다.
        self.evidence.extend(self.recent_turns.drain(..));
        Ok(())
    }
    fn observe_position<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        observation: PositionObservation<'_, V::View>,
    ) -> Result<(), RunError> {
        let PositionObservation {
            frame,
            view,
            player,
            measured,
            input_before,
            acquired,
            sequence,
        } = observation;
        if c.vision.has_graph() {
            let location = c
                .vision
                .graph_observe(player, measured.displacement, acquired)?;
            self.position_current = location.localized;
            self.progress["position"] = json!(location.position);
            self.progress["breadcrumbs"] = json!(location.places);
            if location.relocated {
                c.vision.reset_motion()?;
                c.vision.seed_motion(frame, view)?;
            }
            if measured.displacement.is_some_and(|d| norm(d) >= 12.0) {
                self.blocked.clear();
                self.recoveries = 0;
            }
            if measured.displacement.is_some() {
                self.matched = Some(frame.clone());
            }
        } else if measured.displacement.is_none() {
            if !c.snapshot()?.held.is_empty()
                && self.matched.is_some()
                && self.motion_evidence < 3
                && self.motion_missing_since.is_none()
            {
                let prefix = format!("continuous-{:04}-motion", self.offset + sequence as usize);
                if let Some(before) = self.matched.clone() {
                    self.retain(format!("{prefix}-before.png"), &before);
                }
                self.retain(format!("{prefix}-after.png"), frame);
                self.motion_evidence += 1;
            }
            if input_before
                .dispatches
                .get(&c.inputs.inputs().keys()[0])
                .copied()
                .unwrap_or(0)
                > 0
                || self.offset > 0
                || self.matched.is_some()
            {
                self.position_current = false;
                let since = *self.motion_missing_since.get_or_insert(acquired);
                if acquired - since >= c.request.reference_seconds * 2.0 {
                    self.progress["position_generation"] =
                        json!(self.progress["position_generation"].as_u64().unwrap_or(0) + 1);
                    self.progress["position"] = json!([0.0, 0.0]);
                    self.progress["breadcrumbs"] = json!([[0.0, 0.0]]);
                    self.progress["position_reference"] = json!("local_scene");
                    self.position_current = true;
                    self.motion_missing_since = None;
                    c.vision.reset_motion()?;
                    c.vision.seed_motion(frame, view)?;
                    self.matched = Some(frame.clone());
                }
            }
        } else if let Some(displacement) = measured.displacement {
            self.position_current = true;
            self.motion_missing_since = None;
            self.matched = Some(frame.clone());
            let position = self.position();
            self.progress["position"] =
                json!([position[0] + displacement[0], position[1] + displacement[1]]);
            if norm(displacement) >= 12.0 {
                let position = self.position();
                self.progress["breadcrumbs"]
                    .as_array_mut()
                    .ok_or_else(|| {
                        RunError::Runtime("navigation breadcrumbs must be an array".into())
                    })?
                    .push(json!(position));
                self.blocked.clear();
                self.recoveries = 0;
            }
        }
        Ok(())
    }
    fn terminal_arrival<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        f: &Frame<V::View>,
    ) -> Result<Option<Flow>, RunError> {
        let valid = valid_terminal(f.landmarks.terminal.as_ref());
        let distance = f
            .landmarks
            .terminal
            .as_ref()
            .filter(|_| valid)
            .map(|t| norm(delta(t.point, f.player.map(f64::from))));
        self.record("terminal_distance", distance);
        if c.request.return_from.is_none()
            && distance.is_some_and(|d| d <= 400.0)
            && (self.attack_started.is_none() || !f.close)
        {
            c.release()?;
            self.action("terminal_arrival_hold");
            let arrived = self
                .terminal_arrival
                .update(true, f.measured.displacement, f.now)?;
            self.record(
                "arrival_hold_seconds",
                self.terminal_arrival.since().map(|since| f.now - since),
            );
            if arrived {
                let t = f
                    .landmarks
                    .terminal
                    .as_ref()
                    .ok_or_else(|| RunError::Runtime("arrival terminal missing".into()))?;
                self.status = "wing_end_observed".into();
                self.reason = if matches!(
                    t.provenance.as_str(),
                    "current_static_two_parts"
                        | "current_relocalized_two_parts"
                        | "current_motion_settled_two_parts"
                ) {
                    "recent_terminal_layout_and_player_settled"
                } else if t.kind == "nonboss_chest_trio" {
                    "current_chest_trio_and_player_settled"
                } else {
                    "current_terminal_landmarks_and_player_settled"
                }
                .into();
                self.progress["terminal"] = json!(t);
                self.retain("wing-end.png", &f.frame);
                return Ok(Some(Flow::Done));
            }
            return Ok(Some(Flow::Next));
        }
        Ok(None)
    }
    fn combat<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        f: &mut Frame<V::View>,
    ) -> Result<Option<Flow>, RunError> {
        f.close = has_near(&f.threats.candidates, f.player, c.env.reaction_radius())?
            || f.probe_confirmed
            || f.direct_hover_near;
        if f.probe_pending && self.attack_started.is_none() && !f.close {
            c.release()?;
            self.action("nearby_hover_probe");
            let ready = c
                .env
                .probe_requested_at()
                .ok_or_else(|| RunError::Runtime("pending hover request missing".into()))?
                + 0.05;
            let started = c.now();
            let remaining = ready.min(c.deadline) - started;
            if remaining > 0.0 {
                c.env.wait(remaining)?;
            }
            self.record("hover_capture_ready_at", ready);
            self.record("hover_capture_wait_seconds", c.now() - started);
            return Ok(Some(Flow::Next));
        }
        if f.close {
            if self.attack_started.is_none() {
                self.attack_started = Some(f.now);
                self.approach_pointer = None;
                self.terminal_missing_since = None;
                self.hub_missing_since = None;
                self.combat_baseline = c.dispatches(1)?;
                c.env.emit(
                    "NORTH_COMBAT: release F2 -> hold F4 Nova for current nearby hostile evidence",
                );
                self.retain(
                    format!(
                        "continuous-{:04}-combat.png",
                        self.offset + f.sequence as usize
                    ),
                    &f.frame,
                );
            }
            self.quiet_frames = 0;
        } else if self.attack_started.is_some() {
            self.quiet_frames += 1;
        }
        if let Some(started) = self.attack_started {
            self.stationary_since = None;
            if f.now - started >= 6.0 {
                c.release()?;
                return Ok(Some(self.pause("nova_target_requires_review")));
            }
            if self.quiet_frames >= 3 && f.now - started >= 1.0 {
                c.release()?;
                c.env.emit(
                    "NORTH_COMBAT: Nova block finished; inspect approved loot (kill unconfirmed)",
                );
                self.attack_started = None;
                self.comparison = None;
                self.last_changed_at = f.now;
                let dispatches = c.dispatches(1)?;
                if dispatches > self.combat_baseline
                    && let Some(inspection) =
                        c.env
                            .inspect_loot(c.started, dispatches, c.request.buff_expires_at)?
                {
                    self.record("loot_inspection", &inspection);
                    self.action("post_combat_loot_inspection");
                    c.vision.clear_steering_aim();
                    self.last_aim = None;
                    self.hub_arrival.reset();
                    self.terminal_arrival.reset();
                    let (has_reference, at) = c.vision.terminal_reference();
                    if has_reference
                        && inspection["status"] == "inspection_complete"
                        && inspection["labels_state"] == "off"
                        && inspection["movement_allowed"] == true
                        && inspection["clicks"] == json!([])
                        && inspection["teleports"] == json!([])
                    {
                        self.record("terminal_reference_retained_at", at);
                        c.vision.relocalize_terminal();
                    } else {
                        c.vision.reset_terminal();
                    }
                    c.vision.reset_hub();
                    c.env.reset_probe();
                    return Ok(Some(Flow::Next));
                }
            } else {
                c.combat("near_hostile_training", f.acquired, f.sequence)?;
                self.action("hold_nova");
                self.approach_settle.reset();
                return Ok(Some(Flow::Next));
            }
        }
        Ok(None)
    }
}
