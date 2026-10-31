use super::*;
use crate::{
    common::{
        encounters::{Appearance, CandidateCategory, CandidateIdentity, SceneContext},
        field_control::InputSessionOptions,
        frame::{PixelRect, Size},
        scene_motion::MotionReason,
        survival::ObservationStamp,
        time::MonotonicClock,
    },
    test_field_world::{World, compare, config, frame_index, point},
};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
#[test]
fn current_stair_bend_and_reverse_guard_use_the_same_isometric_angle() -> TestResult {
    let (_, policy) = config()?;
    // R10의 실제 제안: raw 화면 각도150.2도/등각 보정128.6도. 좌표계 차이로 거부됐었다.
    for (aim, allowed) in [([270, 539], true), ([270, 924], false)] {
        let got = exercise(
            &json!({"seconds":1,"frames":[{"player":[962,578],"route_aim":aim,"flow_reason":"bend_entered"}]}),
            &policy,
        )?;
        let rows = got["result"]["continuous_frames"]
            .as_array()
            .ok_or_else(|| format!("rows {got}"))?;
        assert_eq!(
            rows.iter().any(|row| row["action"] == "hold_teleport"),
            allowed,
            "{got}"
        );
        if !allowed {
            assert_eq!(
                got["result"]["continuous_status"],
                "opposite_wing_aim_rejected"
            );
        }
    }
    let stale = exercise(
        &json!({"seconds":1,"frames":[{"capture_delay":0.3,"route_aim":[270,539],"flow_reason":"bend_entered"}]}),
        &policy,
    )?;
    assert!(
        !stale["events"]
            .as_array()
            .ok_or("events")?
            .iter()
            .any(|e| e[0] == "down")
    );
    Ok(())
}
#[test]
fn native_diagnostics_put_bookends_before_bounded_turn_samples() -> TestResult {
    let (_, policy) = config()?;
    let got = exercise(
        &json!({"seconds":1,"stream_evidence":true,"frames":[
            {"aim":[1400,400],"steering_reason":"previous_floor_closed"},
            {"aim":[1500,440],"steering_reason":"previous_floor_closed"},
            {"aim":[1400,400],"steering_reason":"previous_floor_closed"},
            {"aim":[1500,440],"steering_reason":"previous_floor_closed"}
        ]}),
        &policy,
    )?;
    let files = got["evidence"].as_array().ok_or("evidence")?;
    let names = files
        .iter()
        .filter_map(|row| row["file"].as_str())
        .collect::<Vec<_>>();
    let turns = names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.ends_with("-turn.png"))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(!turns.is_empty() && turns.len() <= 2, "{got}");
    let before = names
        .iter()
        .position(|name| name.starts_with("continuous-") && name.ends_with("-before.png"))
        .ok_or("before")?;
    let after = names
        .iter()
        .position(|name| name.starts_with("continuous-") && name.ends_with("-after.png"))
        .ok_or("after")?;
    assert!(turns.iter().all(|i| *i > before && *i > after));
    Ok(())
}
#[test]
fn exhausted_return_reobserves_once_without_casting_until_a_new_frame() -> TestResult {
    let (_, policy) = config()?;
    for used in [false, true] {
        let got = exercise(
            &json!({"seconds":8,"graph":true,"return_from":"north","reobserve_route":true,
            "progress":{"steps":[],"position":[0,0],"breadcrumbs":[[0,0]],"route_graph_reacquired":used},
            "frames":[{"graph_aim":[500,800],"motion":[0,0],"shade":0}]}),
            &policy,
        )?;
        let rows = got["result"]["continuous_frames"]
            .as_array()
            .ok_or_else(|| format!("rows {got}"))?;
        let holds = rows
            .iter()
            .filter(|r| r["action"] == "route_graph_reacquire_hold")
            .collect::<Vec<_>>();
        assert_eq!(holds.len(), usize::from(!used), "{got}");
        assert_eq!(
            got["result"]["continuous_status"],
            "stuck_no_observed_escape"
        );
        if let Some(hold) = holds.first() {
            assert!(
                hold["blocked_aims"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty())
            );
            let source = hold["acquired_at"].as_f64().ok_or("source")?;
            let next = rows
                .iter()
                .find(|r| r["acquired_at"].as_f64().is_some_and(|t| t > source))
                .ok_or("next fresh frame")?;
            assert_ne!(next["acquired_at"], hold["acquired_at"]);
            let events = got["events"].as_array().ok_or("events")?;
            let reset = events
                .iter()
                .position(|e| e[0] == "graph_reobserve")
                .ok_or("reset")?;
            let capture = events
                .iter()
                .enumerate()
                .find(|(i, e)| *i > reset && e[0] == "capture")
                .map(|(i, _)| i)
                .ok_or("next capture")?;
            assert!(!events[reset..capture].iter().any(|e| e[0] == "down"));
        }
    }
    let stopped = exercise(
        &json!({"seconds":8,"graph":true,"return_from":"north","reobserve_route":true,"stop_on_reobserve":true,
        "frames":[{"graph_aim":[500,800],"motion":[0,0],"shade":0}]}),
        &policy,
    )?;
    let events = stopped["events"].as_array().ok_or("events")?;
    let reset = events
        .iter()
        .position(|e| e[0] == "graph_reobserve")
        .ok_or("reset")?;
    assert!(!events[reset..].iter().any(|e| e[0] == "down"));
    Ok(())
}
impl ContinuousEnvironment for World {
    fn evidence_streaming(&self) -> bool {
        self.state().case["stream_evidence"] == true
    }
    fn wall_clock(&self) -> f64 {
        1010.0
    }
    fn desktop_to_frame(&self, point: DesktopPoint) -> Result<[i32; 2], ControlError> {
        Ok([(point[0] + 1922) as i32, point[1] as i32])
    }
    fn has_map(&self) -> bool {
        self.state().case["has_map"] == true
    }
    fn map_visible(&mut self, _: &Arc<BgrFrame>) -> Result<bool, RunError> {
        self.event(json!(["map_visible", self.index()]));
        Ok(self.sample()["map_visible"].as_bool().unwrap_or(true))
    }
    fn panels_open(&mut self, _: &Arc<BgrFrame>) -> Result<bool, RunError> {
        self.event(json!(["panels", self.index()]));
        Ok(self.sample()["panels"].as_bool().unwrap_or(false))
    }
    fn confirm_map_restored(&mut self) -> Result<Value, RunError> {
        self.event(json!(["restore_map", self.now()]));
        let delay = self.state().case["restore_delay"].as_f64().unwrap_or(0.0);
        self.advance(delay);
        if self.state().case["restore_error"] == true {
            return Err(RunError::Runtime("map restore failed".into()));
        }
        Ok(json!({"status":"on_observed"}))
    }
    fn probe_next_proposal_at(&self) -> f64 {
        0.0
    }
    fn probe_requested_at(&self) -> Option<f64> {
        self.state().probe_requested_at
    }
}
struct Vision {
    w: World,
    aim: Option<[i32; 2]>,
    detour: Option<[f64; 2]>,
    flow_reason: String,
    reference: Option<TerminalReading>,
    reference_at: Option<f64>,
    approach: Option<TerminalReading>,
    approach_motion: Option<[f64; 2]>,
    static_pairs: u64,
    graph_position: [f64; 2],
    localized: bool,
    places: Vec<[f64; 2]>,
    failed_for_reobservation: bool,
}
struct Jobs {
    plan: Option<Result<CurrentPlan<i64>, RunError>>,
    terminal: Option<TerminalReading>,
    objective: Option<ObjectiveReading>,
}
fn terminal(value: &Value) -> Option<TerminalReading> {
    if !truth(value) {
        return None;
    }
    Some(TerminalReading {
        kind: value["kind"]
            .as_str()
            .unwrap_or("nonboss_chest_trio")
            .into(),
        point: point(&value["point"], [800.0, 580.0]),
        evidence_boxes: vec![],
        scores: vec![],
        part_ids: vec![],
        provenance: value["provenance"]
            .as_str()
            .unwrap_or("current_three_parts")
            .into(),
        current_parts: vec![true; 3],
        reference_observed_at: None,
        visible_fractions: vec![],
        minimum_separation: 84.0,
        minimum_area: 8000.0,
    })
}
impl Vision {
    fn new(w: World) -> Self {
        Self {
            w,
            aim: None,
            detour: None,
            flow_reason: "global_flow".into(),
            reference: None,
            reference_at: None,
            approach: None,
            approach_motion: None,
            static_pairs: 0,
            graph_position: [0.0; 2],
            localized: true,
            places: vec![[0.0; 2]],
            failed_for_reobservation: false,
        }
    }
    fn choice(&self, key: &str, reason: &str) -> PathChoice {
        let s = self.w.sample();
        PathChoice {
            aim: if s.get(key) == Some(&Value::Null) {
                None
            } else {
                Some(point(&s[key], [1300.0, 300.0]).map(|v| v as i32))
            },
            reason: s[reason].as_str().unwrap_or("test").into(),
            path: vec![],
        }
    }
    fn candidates(
        &mut self,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        self.w.event(json!([
            "hazards",
            self.w.index(),
            player,
            acquired,
            self.w.now(),
            aim
        ]));
        self.w.delay("hazard_delay");
        if self.w.sample()["near"] != true {
            return Ok(vec![]);
        }
        let context = Arc::new(SceneContext::new(
            ObservationStamp::new("flash", "room", self.w.index() as u64, acquired)?,
            "arcane",
            self.w.index() as u64,
            Size::new(1922, 1140).map_err(ObservationError::from)?,
            PixelRect::new(24, 55, 1900, 900).map_err(ObservationError::from)?,
        )?);
        Ok(vec![VisualCandidate::new(
            context,
            CandidateIdentity::new(
                "hostile",
                "hostile",
                CandidateCategory::Monster,
                Appearance::Alive,
            )?,
            1.0,
            PixelRect::new(700, 500, 810, 610).map_err(ObservationError::from)?,
            Some([800.0, 580.0]),
        )?])
    }
}
impl ContinuousVision for Vision {
    type View = i64;
    type Jobs = Jobs;
    fn initialize(
        &mut self,
        _: f64,
        aim: Option<[i32; 2]>,
        detour: Option<[f64; 2]>,
    ) -> Result<(), RunError> {
        self.aim = aim;
        self.detour = detour;
        Ok(())
    }
    fn clearance(&self) -> f64 {
        24.0
    }
    fn has_hub(&self) -> bool {
        self.w.state().case["has_hub"] == true
    }
    fn has_graph(&self) -> bool {
        self.w.state().case["graph"] == true
    }
    fn prepares_ground(&self) -> bool {
        self.w.state().case["camera"] == true
    }
    fn prepare(
        &mut self,
        _: &Arc<BgrFrame>,
        acquired: f64,
        _: &HudReading,
        focused: bool,
    ) -> Result<(), RunError> {
        self.w
            .event(json!(["prepare", self.w.index(), acquired, focused]));
        Ok(())
    }
    fn ground(&mut self, _: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError> {
        self.w.event(json!(["ground", self.w.index(), acquired]));
        self.w.delay("ground_delay");
        let s = self.w.sample();
        Ok(GroundReading {
            point: if s.get("player") == Some(&Value::Null) {
                None
            } else {
                Some(point(&s["player"], [750.0, 580.0]).map(|v| v as i32))
            },
            reason: s["ground_reason"].as_str().unwrap_or("test").into(),
            score: None,
            calibration: None,
        })
    }
    fn threats(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
        engage: bool,
    ) -> Result<CurrentThreats, RunError> {
        let candidates = if engage {
            self.candidates(player, acquired, aim)?
        } else {
            vec![]
        };
        let hover = aim
            .map(|aim| self.hover(frame, aim))
            .transpose()?
            .unwrap_or_default();
        let target = if engage && self.w.state().case["probe"] == true {
            self.w.event(json!(["target", self.w.index()]));
            self.w.delay("target_delay");
            (self.w.sample()["target"] == true).then(|| HostileTarget {
                kind: "hostile".into(),
                evidence_box: [700, 55, 800, 90],
                title_score: None,
            })
        } else {
            None
        };
        Ok(CurrentThreats {
            candidates,
            hover,
            target,
        })
    }
    fn near_preempts_hover(&self) -> bool {
        self.w.state().case["early_near"] == true
    }
    fn finish_threats(&mut self, _threats: &mut CurrentThreats) -> Result<(), RunError> {
        if self.near_preempts_hover() {
            self.w.event(json!(["hover_join", self.w.now()]));
            self.w.advance(0.2);
            if self.w.state().case["hover_error"] == true {
                return Err(RunError::Runtime("hover fixture failed".into()));
            }
        }
        Ok(())
    }
    fn hover(&mut self, _: &Arc<BgrFrame>, aim: [i32; 2]) -> Result<Vec<HoverCandidate>, RunError> {
        self.w.event(json!(["hover", self.w.index(), aim]));
        self.w.delay("hover_delay");
        Ok(if self.w.sample()["hover"] == true {
            vec![HoverCandidate {
                kind: "chest".into(),
                evidence_box: [1000, 400, 1050, 420],
                score: 1.0,
            }]
        } else {
            vec![]
        })
    }
    fn hazards(
        &mut self,
        _: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        self.candidates(player, acquired, aim)
    }
    fn start_plan(
        &mut self,
        _: &Arc<BgrFrame>,
        request: PlanningRequest<'_>,
    ) -> Result<PlanningJobs<Self::Jobs>, RunError> {
        let w = &self.w;
        w.event(json!([
            "plan",
            w.index(),
            request.acquired,
            request.position,
            request.excluded
        ]));
        w.delay("plan_delay");
        let s = w.sample();
        let plan = if truth(&s["plan_error"]) || request.ground.point.is_none() {
            Err(RunError::Runtime(
                s["plan_error"]
                    .as_str()
                    .unwrap_or(&request.ground.reason)
                    .into(),
            ))
        } else if !fresh(w.now(), request.acquired) {
            Err(RunError::Runtime("planned step frame is stale".into()))
        } else {
            Ok(CurrentPlan {
                choice: self.choice("aim", "choice_reason"),
                ground: request.ground.clone(),
                view: w.index(),
            })
        };
        let terminal = if request.outbound && w.state().case["has_terminal"] == true {
            w.event(json!(["terminal", w.index()]));
            w.delay("terminal_delay");
            terminal(&s["terminal"])
        } else {
            None
        };
        let objective = if request.outbound && w.state().case["has_objective"] == true {
            w.event(json!(["objective", w.index()]));
            w.delay("objective_delay");
            (s["objective"] == true).then(|| ObjectiveReading {
                kind: "summoner_platform".into(),
                journal_box: None,
                journal_score: None,
                rune_boxes: vec![[650, 400, 850, 500]],
                evidence_kind: "runes".into(),
                glyph_scores: vec![],
                ground_point: Some([800.0, 580.0]),
                slot_indices: vec![],
                layout_residual: None,
            })
        } else {
            None
        };
        let hub = if self.has_hub() {
            w.event(json!(["hub", w.index()]));
            w.delay("hub_delay");
            if s.get("hub") == Some(&Value::Null) {
                None
            } else {
                Some(point(&s["hub"], [750.0, 580.0]).map(|v| v as i32))
            }
        } else {
            None
        };
        let hub_evidence = hub.map(|point| HubEvidence::Current {
            point,
            parts: vec![],
            blue_evidence: "test",
        });
        Ok(PlanningJobs {
            jobs: Jobs {
                plan: Some(plan),
                terminal,
                objective,
            },
            hub,
            hub_evidence,
        })
    }
    fn objective(&mut self, jobs: &mut Self::Jobs) -> Result<Option<ObjectiveReading>, RunError> {
        Ok(jobs.objective.take())
    }
    fn plan(&mut self, jobs: &mut Self::Jobs) -> Result<CurrentPlan<i64>, RunError> {
        jobs.plan
            .take()
            .ok_or_else(|| RunError::Runtime("plan consumed".into()))?
    }
    fn terminal(&mut self, jobs: &mut Self::Jobs) -> Result<Option<TerminalReading>, RunError> {
        Ok(jobs.terminal.take())
    }
    fn set_sequence(&mut self, _: &mut i64, _: u64) {}
    fn choose(
        &mut self,
        view: &i64,
        wing: Wing,
        o: ChooseOptions<'_>,
    ) -> Result<PathChoice, RunError> {
        self.w.event(json!(["choose",view,wing,{"position":o.position,"visited":o.visited,"destination":o.destination,"excluded":o.excluded,"allow_detour":o.allow_detour,"heading":o.heading,"prefer_frontier":o.prefer_frontier}]));
        self.w.delay("choose_delay");
        Ok(self.choice("choose_aim", "choose_reason"))
    }
    fn floor_supports(&self, _: &i64, aim: [i32; 2]) -> Result<bool, RunError> {
        self.w.event(json!(["floor", self.w.index(), aim]));
        Ok(self.w.sample()["floor"].as_bool().unwrap_or(true))
    }
    fn cell_pixels(&self, _: &i64) -> f64 {
        12.0
    }
    fn measure(
        &mut self,
        _: &Arc<BgrFrame>,
        _: &i64,
        hint: Option<[f64; 2]>,
    ) -> Result<(CameraMotion, Vec<TerrainLanding>), RunError> {
        self.w
            .event(json!(["motion", self.w.index(), self.w.index() + 1, hint]));
        self.w.delay("motion_delay");
        let s = self.w.sample();
        let displacement = if s.get("motion") == Some(&Value::Null) {
            None
        } else {
            Some(point(&s["motion"], [0.0; 2]))
        };
        Ok((
            CameraMotion {
                displacement,
                inliers: 0,
                reason: if displacement.is_some() {
                    MotionReason::BackgroundTranslation
                } else {
                    MotionReason::CorrespondenceUnavailable
                },
                matches: 0,
                inlier_ratio: None,
                linear_error: None,
            },
            vec![],
        ))
    }
    fn reset_motion(&mut self) -> Result<(), RunError> {
        self.w.event(json!(["motion_reset"]));
        Ok(())
    }
    fn seed_motion(&mut self, frame: &Arc<BgrFrame>, view: &i64) -> Result<(), RunError> {
        self.measure(frame, view, None)?;
        Ok(())
    }
    fn track(&mut self, o: LandmarkObservation<'_>) -> Result<TrackedLandmarks, RunError> {
        self.w.event(json!([
            "terminal_track",
            self.w.index(),
            o.acquired,
            o.displacement
        ]));
        if o.terminal.is_some() {
            self.reference = o.terminal.clone();
            self.reference_at = Some(o.acquired);
        }
        let s = self.w.sample();
        self.approach = terminal(&s["approach"]);
        self.approach_motion = pair(&s["approach_motion"]);
        self.static_pairs = s["static_pairs"].as_u64().unwrap_or(0);
        self.w.event(json!([
            "hub_track",
            self.w.index(),
            o.acquired,
            o.displacement
        ]));
        Ok(TrackedLandmarks {
            terminal: o.terminal,
            approach: self.approach.clone(),
            approach_displacement: self.approach_motion,
            approach_static_pairs: self.static_pairs,
            hub: o.hub,
            hub_evidence: o.evidence,
        })
    }
    fn reset_terminal(&mut self) {
        self.w.event(json!(["terminal_reset"]));
        self.reference = None;
        self.reference_at = None;
        self.approach = None;
        self.approach_motion = None;
        self.static_pairs = 0;
    }
    fn relocalize_terminal(&mut self) {
        self.w.event(json!(["terminal_relocalize"]));
        self.approach = None;
        self.approach_motion = None;
        self.static_pairs = 0;
    }
    fn terminal_reference(&self) -> (bool, Option<f64>) {
        (self.reference.is_some(), self.reference_at)
    }
    fn reset_hub(&mut self) {
        self.w.event(json!(["hub_reset"]));
    }
    fn current_pair_near(
        &mut self,
        _: &Arc<BgrFrame>,
        player: [i32; 2],
    ) -> Result<Option<[f64; 2]>, RunError> {
        self.w.event(json!(["pair", self.w.index(), player]));
        Ok(pair(&self.w.sample()["pair"]))
    }
    fn camera_diagnostics(&self) -> Result<Value, RunError> {
        Ok(json!({"camera_center":null,"last_camera_reset":null}))
    }
    fn graph_observe(
        &mut self,
        player: [i32; 2],
        displacement: Option<[f64; 2]>,
        acquired: f64,
    ) -> Result<GraphLocation, RunError> {
        self.w.event(json!([
            "graph_observe",
            self.w.index(),
            player,
            displacement,
            acquired
        ]));
        self.w.delay("graph_observe_delay");
        let s = self.w.sample();
        self.graph_position = point(&s["graph_position"], [0.0; 2]);
        self.localized = s["localized"].as_bool().unwrap_or(true);
        self.places = s["graph_places"]
            .as_array()
            .map(|v| v.iter().filter_map(pair).collect())
            .unwrap_or(vec![[0.0; 2]]);
        Ok(GraphLocation {
            relocated: s["relocated"] == true,
            localized: self.localized,
            position: self.graph_position,
            places: self.places.clone(),
        })
    }
    fn graph_choose(&mut self, _: &i64, r: RouteChoiceRequest<'_>) -> Result<PathChoice, RunError> {
        self.w.event(json!(["graph_choose",self.w.index(),r.wing,r.now,{"excluded":r.excluded,"landings":r.landings,"hub":r.hub,"prefer_goal_exit":r.returning}]));
        self.w.delay("graph_delay");
        if self.failed_for_reobservation {
            return Ok(PathChoice::unknown("graph_entry_retreat_exhausted"));
        }
        Ok(self.choice("graph_aim", "graph_reason"))
    }
    fn graph_fail_landing(
        &mut self,
        aim: Option<[i32; 2]>,
        player: [i32; 2],
    ) -> Result<(), RunError> {
        self.w
            .event(json!(["graph_fail", self.w.index(), aim, player]));
        if self.w.state().case["reobserve_route"] == true {
            self.failed_for_reobservation = true;
        }
        Ok(())
    }
    fn reobserve_route_graph(&mut self, _: &i64, _: [i32; 2], at: f64) -> Result<bool, RunError> {
        if self.w.state().case["reobserve_route"] != true {
            return Ok(false);
        }
        self.w
            .event(json!(["graph_reobserve", self.w.index(), at, self.w.now()]));
        self.failed_for_reobservation = false;
        self.aim = None;
        self.detour = None;
        if self.w.state().case["stop_on_reobserve"] == true {
            self.w.1.set();
        }
        Ok(true)
    }
    fn graph_summary(&self) -> Option<Value> {
        self.has_graph().then(||json!({"position":self.graph_position,"localized":self.localized,"places":self.places}))
    }
    fn flow_choose(
        &mut self,
        _: &i64,
        _: Wing,
        global: &PathChoice,
        now: f64,
        _: crate::runs::summoner::steering::RouteFlowOptions<'_>,
    ) -> Result<PathChoice, RunError> {
        self.w.event(json!(["flow", self.w.index(), now]));
        self.w.delay("flow_delay");
        let s = self.w.sample();
        self.flow_reason = s["flow_reason"].as_str().unwrap_or("global_flow").into();
        if s.get("flow_detour").is_some() {
            self.detour = pair(&s["flow_detour"]);
        }
        Ok(if s.get("route_aim").is_some() {
            self.choice("route_aim", "route_reason")
        } else {
            global.clone()
        })
    }
    fn flow_state(&self) -> (Option<[f64; 2]>, String) {
        (self.detour, self.flow_reason.clone())
    }
    fn clear_steering_aim(&mut self) {
        self.aim = None;
    }
    fn steering_aim(&self) -> Option<[i32; 2]> {
        self.aim
    }
    fn steer(
        &mut self,
        _: &i64,
        choice: &PathChoice,
        now: f64,
    ) -> Result<(bool, &'static str), RunError> {
        self.w.event(json!(["steer", self.w.index(), now]));
        self.w.delay("steer_delay");
        let changed = self.aim != choice.aim;
        self.aim = choice.aim;
        let s = self.w.sample();
        let reason = match s["steering_reason"].as_str() {
            Some("opposite_floor_recheck") => "opposite_floor_recheck",
            Some("previous_floor_closed") => "previous_floor_closed",
            _ => {
                if changed {
                    "cast_reference"
                } else {
                    "hold_current_aim"
                }
            }
        };
        Ok((s["steer"].as_bool().unwrap_or(changed), reason))
    }
    fn comparison(&mut self, _: &Arc<BgrFrame>) -> Result<Vec<u8>, RunError> {
        Ok(vec![
            self.w.sample()["shade"].as_u64().unwrap_or(0) as u8;
            11360
        ])
    }
    fn finish(&mut self, _: Duration) -> Result<(), RunError> {
        if self.w.state().case["cleanup_error"] == true {
            Err(RunError::Runtime("vision shutdown timed out".into()))
        } else {
            Ok(())
        }
    }
}
fn exercise(case: &Value, policy: &CharacterSurvivalPolicy) -> Result<Value, Box<dyn Error>> {
    let mut case = case.clone();
    case["kind"] = json!("continuous");
    let mut w = World::new(&case);
    if case["stop_at"].as_f64().is_some_and(|at| at <= 10.0) {
        w.1.set();
    }
    let focused = w.clone();
    let mut inputs = FieldInputSession::new(
        Arc::new(w.clone()),
        "f2",
        "f4",
        InputSessionOptions {
            clock: Arc::new(w.clone()),
            stop: w.1.clone(),
            focused: Arc::new(move || focused.focused()),
            repeat_seconds: Some(0.32),
            aim_settle_seconds: if case["early_near"] == true {
                0.0
            } else {
                0.05
            },
        },
    )?;
    let mut vision = Vision::new(w.clone());
    let mut progress = case
        .get("progress")
        .cloned()
        .unwrap_or(json!({"steps":[],"position":[0.0,0.0],"breadcrumbs":[[0.0,0.0]]}));
    let proof=case.get("proof").cloned().unwrap_or(json!({"source":"runtime_screen","area":"arcane_sanctuary","panels_closed":true,"belt_closed":true,"nearby_candidates":0,"safety_scope":"current_nearby_candidates_and_stable_ratios","observed_at":1009.9,"current_wing":"north","wing_phase":"outbound"}));
    let mut evidence = vec![];
    let wing = serde_json::from_value(case.get("wing").cloned().unwrap_or(json!("north")))?;
    let return_from = case
        .get("return_from")
        .filter(|v| !v.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    let result = execute(
        &mut inputs,
        &mut w,
        &mut vision,
        ContinuousRequest {
            policy,
            proof: &proof,
            seconds: case["seconds"].as_f64().unwrap_or(2.0),
            reference_seconds: 0.32,
            wing,
            return_from,
            engage_monsters: case["engage"].as_bool().unwrap_or(true),
            buff_expires_at: number(&case["buff"]),
            continuation: if case["continue"] == true {
                Continuation::Current
            } else if let Some(previous) = case.get("transition") {
                Continuation::Transition(previous)
            } else {
                Continuation::Initial
            },
        },
        &mut progress,
        &mut evidence,
        |_, _| Ok(()),
    );
    assert!(!inputs.workers_retained());
    assert!(inputs.inputs().snapshot()?.held.is_empty());
    let (mut result, error) = match result {
        Ok(result) => (result, Value::Null),
        Err(error) => (Value::Null, json!(error.to_string())),
    };
    // 기존 oracle에는 없는 계측만 분리한다. 관측/입력/시한/오류 필드는 그대로 비교한다.
    for value in [&mut result, &mut progress] {
        if let Some(rows) = value
            .get_mut("continuous_frames")
            .and_then(Value::as_array_mut)
        {
            for row in rows {
                if let Some(fields) = row.as_object_mut() {
                    if let Some(stages) = fields.remove("stages") {
                        assert!(stages["capture_completed_at"].as_f64().is_some_and(
                            |at| at >= fields["acquired_at"].as_f64().unwrap_or(f64::INFINITY)
                        ));
                    }
                    fields.remove("slow_observations");
                }
            }
        }
    }
    let state = w.state();
    Ok(
        json!({"result":result,"error":error,"progress":progress,"events":state.events,"emits":state.emits,"now":state.now,"evidence":evidence.iter().map(|e|json!({"file":e.filename,"frame":frame_index(&e.frame)})).collect::<Vec<_>>()}),
    )
}
#[test]
fn current_near_defense_preempts_delayed_hover_and_hover_failure_releases_it() -> TestResult {
    let (_, policy) = config()?;
    for failed in [false, true] {
        let result = exercise(
            &json!({"early_near":true,"hover_error":failed,"seconds":1.0,"frames":[{"near":true}]}),
            &policy,
        )?;
        let events = result["events"].as_array().ok_or("events")?;
        let down = events
            .iter()
            .position(|e| e[0] == "down" && e[1] == "f4")
            .ok_or_else(|| format!("defense down: {result}"))?;
        let hover = events
            .iter()
            .position(|e| e[0] == "hover_join")
            .ok_or("hover join")?;
        assert!(down < hover);
        assert!(
            events
                .iter()
                .skip(hover)
                .any(|e| e[0] == "up" && e[1] == "f4")
        );
        if failed {
            assert!(
                result["error"]
                    .as_str()
                    .is_some_and(|e| e.contains("hover fixture failed"))
            );
        }
    }
    Ok(())
}
#[test]
fn continuous_engine_matches_original_current_frame_flow() -> TestResult {
    let (raw, policy) = config()?;
    let cases: Value = serde_json::from_str(
        r#"[
        {},{"camera":true},{"engage":false},{"seconds":1},{"buff":10.0},{"buff":11.0},{"stop_at":10.0},{"focus_lost_at":10.0},{"stop_at":10.5},{"focus_lost_at":10.5},
        {"loot":true,"labels_off":false},{"frames":[{"life":5}]},{"frames":[{"merc":"unverified"}]},{"frames":[{"life":null}]},{"frames":[{"belt":"expanded"}]},{"probe":true,"frames":[{"life":5,"recover":true},{}]},
        {"frames":[{"player":null}]},{"frames":[{"plan_error":"camera_actor_displaced"}]},{"frames":[{"capture_delay":0.25}]},{"frames":[{"motion_delay":0.16}]},
        {"frames":[{"aim":null}]},{"frames":[{"route_aim":null,"route_reason":"no_graph_exit"}]},{"frames":[{"aim":[200,850]}]},
        {"frames":[{"near":true}]},{"probe":true,"frames":[{"proposals":[[800,550]]},{"hostile":true}]},
        {"loot":true,"frames":[{"near":true},{},{},{},{},{},{},{},{},{},{},{},{}]},
        {"frames":[{"hover":true}]},{"frames":[{"hover":true,"choose_aim":null}]},{"frames":[{"steering_reason":"opposite_floor_recheck"}]},
        {"has_objective":true,"frames":[{"objective":true}]},{"has_terminal":true,"frames":[{"terminal":true}]},
        {"has_hub":true},{"has_hub":true,"return_from":"north"},{"has_hub":true,"return_from":"north","frames":[{"hub":[1300,580],"choose_aim":[1300,580]}]},
        {"has_terminal":true,"frames":[{"terminal":{"point":[1400,580]},"choose_aim":[1400,580]}]},
        {"has_hub":true,"return_from":"north","frames":[{}, {"hub":null}]},
        {"has_terminal":true,"frames":[{"terminal":{"point":[1400,580]}},{}]},
        {"has_hub":true,"return_from":"north","has_map":true,"frames":[{}, {"hub":null}]},
        {"has_terminal":true,"has_map":true,"frames":[{"terminal":{"point":[1400,580]}},{}]},
        {"seconds":5,"frames":[{}]}, {"frames":[{"motion":null}]}, {"frames":[{"motion":[15,0]}]},
        {"has_hub":true,"proof":{"source":"runtime_screen","area":"arcane_sanctuary","panels_closed":true,"belt_closed":true,"nearby_candidates":0,"safety_scope":"current_nearby_candidates_and_stable_ratios","observed_at":1009.9},"frames":[{"hub":null}]}
    ]"#,
    )?;
    let expected = crate::test_support::legacy_expectation(
        "continuous-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, case) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        compare(
            &exercise(case, &policy)?,
            &expected[i],
            &format!("case[{i}]"),
        );
    }
    Ok(())
}

#[test]
fn graph_handoffs_map_cleanup_and_frame_cap_match_original() -> TestResult {
    let (raw, policy) = config()?;
    let cases: Value = serde_json::from_str(
        r#"[
      {"graph":true},
      {"graph":true,"frames":[{"relocated":true,"graph_position":[50,80],"graph_places":[[0,0],[50,80]]}]},
      {"graph":true,"frames":[{"graph_aim":null,"graph_reason":"no_exit"}]},
      {"graph":true,"frames":[{"graph_delay":0.16}]},
      {"graph":true,"frames":[{"hover":true}]},
      {"graph":true,"seconds":5},
      {"graph":true,"frames":[{"graph_reason":"current_hub_departure_north"}],"progress":{"steps":[],"position":[0,0],"breadcrumbs":[[0,0]],"last_aim":[200,800]}},
      {"has_hub":true,"return_from":"north","has_map":true,"restore_error":true,"frames":[{}, {"hub":null}]},
      {"has_terminal":true,"has_map":true,"restore_error":true,"frames":[{"terminal":{"point":[1400,580]}},{}]},
      {"has_hub":true,"return_from":"north","has_map":true,"restore_delay":0.6,"progress":{"steps":[],"position":[0,0],"breadcrumbs":[[0,0]],"hub_acquired":true},"frames":[{"hub":null},{}]},
      {"has_terminal":true,"has_map":true,"restore_delay":0.6,"progress":{"steps":[],"position":[0,0],"breadcrumbs":[[0,0]],"terminal_acquired":true},"frames":[{}, {"terminal":true}]},
      {"has_hub":true,"return_from":"north","has_map":true,"stop_at":10.5,"frames":[{}, {"hub":null}]},
      {"has_terminal":true,"has_map":true,"focus_lost_at":10.5,"frames":[{"terminal":{"point":[1400,580]}},{}]},
      {"continue":true},
      {"continue":true,"proof":{},"progress":{"steps":[],"position":[0,0],"breadcrumbs":[[0,0]],"continuous_frames":[{"processed_at":9.9}],"continuous_status":"interval_complete","wing":"north","return_from":null,"interval_finished_at":9.9}},
      {"frames":[{"capture_delay":0}]},
      {"has_hub":true,"return_from":"north","frames":[{"hub":[1300,580],"choose_aim":[1300,580]}, {"hub":[1300,580],"choose_aim":[1300,580]}, {"hub":[1300,580],"choose_aim":[1300,580]}, {"hub":[1300,580],"choose_aim":[1300,580]}, {"hub":[750,580],"motion":[70,0]}]},
      {"has_terminal":true,"frames":[{"terminal":{"point":[1400,580]},"choose_aim":[1400,580]}, {"terminal":{"point":[1400,580]},"choose_aim":[1400,580]}, {"terminal":{"point":[1400,580]},"choose_aim":[1400,580]}, {"terminal":{"point":[1400,580]},"choose_aim":[1400,580]}, {"terminal":true}]},
      {"has_terminal":true,"frames":[{"terminal":true,"near":true}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}],"loot":true},
      {"has_terminal":true,"frames":[{"terminal":true,"near":true}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}],"loot":true,"inspection":{"status":"inspection_complete","labels_state":"off","movement_allowed":true,"clicks":[],"teleports":[1]}}
    ]"#,
    )?;
    let expected = crate::test_support::legacy_expectation(
        "continuous-execution-oracle",
        &json!({"policy":raw,"cases":cases}),
    )?;
    for (i, case) in cases.as_array().ok_or("cases")?.iter().enumerate() {
        compare(
            &exercise(case, &policy)?,
            &expected[i],
            &format!("graph/map case[{i}]"),
        );
    }
    Ok(())
}
