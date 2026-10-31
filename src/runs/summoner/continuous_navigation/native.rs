use super::*;
mod slow;
use crate::common::{
    frame::{PixelRect, Size},
    scene_motion::{BackgroundMotionObserver, LandmarkMask, MotionLimits},
    time::MonotonicClock,
    vision_ops::{
        color,
        resize::{self, Interpolation},
    },
    workers::{CooperativeStop, ObservationPool, ObservationTask, OwnedObserver},
};
use crate::runs::summoner::main_run::native::PanelObservers;
use crate::runs::summoner::{
    camera_ground::{CameraGroundHandle, CameraOnlyGround},
    navigation::{ArcanePathObserver, PathView},
    navigation_execution::{PathMotionMasks, PlanOptions, StepPlan, plan_arcane_step},
    objective_vision::ArcaneObjectiveObserver,
    route_graph::{ArcaneRouteGraph, GraphChooseOptions},
    scene_vision::{
        ArcaneHazardHandle, ArcaneHoverObserver, ArcaneHubObserver, ArcaneTargetObserver,
        HubStabilityTracker,
    },
    steering::{ArcaneRouteFlow, ContinuousSteering, RouteFlowOptions, floor_supports_aim},
    terminal_vision::{ArcaneTerminalObserver, TerminalObservation, TerminalStabilityTracker},
    terrain_patterns::ArcaneTerrainPatterns,
};
use slow::SlowScan;
type PanelTask = ObservationTask<Result<(f64, f64), ObservationError>>;

pub struct ContinuousObservers {
    pub panels: Option<PanelObservers>,
    pub camera: CameraGroundHandle,
    pub path: ArcanePathObserver,
    pub hazards: ArcaneHazardHandle,
    pub hover: Arc<ArcaneHoverObserver>,
    pub target: Option<Arc<ArcaneTargetObserver>>,
    pub hub: Option<ArcaneHubObserver>,
    pub graph: Option<ArcaneRouteGraph>,
    pub terrain: Option<Arc<ArcaneTerrainPatterns>>,
    pub terminal: Option<ArcaneTerminalObserver>,
    pub objective: Option<Arc<ArcaneObjectiveObserver>>,
}
pub struct NativeContinuousVision {
    panels: Option<PanelObservers>,
    pending_panels: Option<(std::sync::Weak<BgrFrame>, PanelTask)>,
    panel_timing: Option<(f64, f64)>,
    camera: CameraGroundHandle,
    hazards: ArcaneHazardHandle,
    hover: Arc<ArcaneHoverObserver>,
    target: Option<Arc<ArcaneTargetObserver>>,
    hub_observer: Option<ArcaneHubObserver>,
    graph: Option<ArcaneRouteGraph>,
    terrain: Option<Arc<ArcaneTerrainPatterns>>,
    terminal_observer: Option<OwnedObserver<ArcaneTerminalObserver>>,
    objective_observer: Option<Arc<ArcaneObjectiveObserver>>,
    path: Arc<ArcanePathObserver>,
    motion: Option<BackgroundMotionObserver>,
    terminal: TerminalStabilityTracker,
    hub: HubStabilityTracker,
    steering: Option<ContinuousSteering>,
    flow: ArcaneRouteFlow,
    clock: Arc<dyn MonotonicClock>,
    pool: ObservationPool,
    slow_pool: ObservationPool,
    terminal_scan: SlowScan<TerminalReading>,
    objective_scan: SlowScan<ObjectiveReading>,
    current_goals: bool,
    local_field: bool,
    mapping_current: bool,
    last_mapping_source: Option<f64>,
    pending_hover: Option<ObservationTask<Result<Vec<HoverCandidate>, ObservationError>>>,
    stop: CooperativeStop,
    initialized: bool,
    pool_finished: bool,
    clearance: f64,
}
pub struct NativePlanningJobs {
    plan: Option<ObservationTask<Result<StepPlan, ObservationError>>>,
    terminal: Option<ObservationTask<Result<Option<TerminalReading>, ObservationError>>>,
    objective: Option<ObservationTask<Result<Option<ObjectiveReading>, ObservationError>>>,
}
fn missing(message: &str) -> RunError {
    RunError::Runtime(message.into())
}
impl NativeContinuousVision {
    pub fn new(
        observers: ContinuousObservers,
        clock: Arc<dyn MonotonicClock>,
        stop: CooperativeStop,
    ) -> Result<Self, RunError> {
        let ContinuousObservers {
            panels,
            camera,
            mut path,
            hazards,
            hover,
            target,
            hub,
            graph,
            terrain,
            terminal,
            objective,
        } = observers;
        let mut teleport = path.teleport().ok_or_else(|| {
            crate::common::config::ConfigError::value(
                "explicit reviewed teleport geometry required for continuous steering",
            )
        })?;
        teleport.preferred_distance = Some(teleport.maximum_distance);
        path.set_teleport(Some(teleport))?;
        let path = Arc::new(path);
        Ok(Self {
            panels,
            pending_panels: None,
            panel_timing: None,
            camera,
            hazards,
            hover,
            target,
            hub_observer: hub,
            graph,
            terrain,
            terminal_observer: terminal.map(|v| OwnedObserver::new(v, "terminal")),
            objective_observer: objective,
            path,
            motion: Some(BackgroundMotionObserver::new(
                MotionLimits::new(12, 0.65, 3.0, 0.5)?,
                Some(LandmarkMask {
                    viewport: PixelRect::new(120, 180, 1720, 890)
                        .map_err(ObservationError::from)?,
                    actor_exclusion: [260, 230, 260, 120],
                }),
            )),
            terminal: TerminalStabilityTracker::new(2.0)?,
            hub: HubStabilityTracker::new(2.0)?,
            steering: None,
            flow: ArcaneRouteFlow::new(None),
            clock,
            pool: ObservationPool::new(3, 3, stop.clone())?,
            slow_pool: ObservationPool::new(1, 2, stop.clone())?,
            terminal_scan: SlowScan::default(),
            objective_scan: SlowScan::default(),
            current_goals: false,
            local_field: false,
            mapping_current: true,
            last_mapping_source: None,
            pending_hover: None,
            stop,
            initialized: false,
            pool_finished: false,
            clearance: teleport.clearance_pixels,
        })
    }
    fn motion_mut(&mut self) -> Result<&mut BackgroundMotionObserver, RunError> {
        self.motion
            .as_mut()
            .ok_or_else(|| missing("motion observation unavailable"))
    }
    /// 다른 날개/복귀 단계는 종료된 관찰 pool 뒤에 새 경로 기억으로 시작한다.
    pub fn reset_route_memory(&mut self) -> Result<(), RunError> {
        if self.initialized && !self.pool_finished {
            return Err(missing(
                "previous continuous observation session is not finished",
            ));
        }
        if self.graph.is_some() {
            self.graph = Some(ArcaneRouteGraph::default());
        }
        Ok(())
    }
}
impl ContinuousVision for NativeContinuousVision {
    type View = PathView;
    type Jobs = NativePlanningJobs;
    fn initialize(
        &mut self,
        reference: f64,
        last_aim: Option<[i32; 2]>,
        detour: Option<[f64; 2]>,
    ) -> Result<(), RunError> {
        if self.initialized && !self.pool_finished {
            return Err(missing(
                "previous continuous observation session is not finished",
            ));
        }
        if self.pool_finished {
            self.pool = ObservationPool::new(3, 3, self.stop.clone())?;
            self.slow_pool = ObservationPool::new(1, 2, self.stop.clone())?;
            self.terminal_scan = SlowScan::default();
            self.objective_scan = SlowScan::default();
            self.pool_finished = false;
        }
        self.initialized = true;
        self.current_goals = false;
        self.local_field = false;
        self.mapping_current = true;
        self.last_mapping_source = None;
        self.steering = Some(ContinuousSteering::new(
            reference,
            self.clearance(),
            last_aim,
        )?);
        self.flow = ArcaneRouteFlow::new(detour);
        self.terminal.reset();
        self.hub.reset();
        self.reset_motion()
    }
    fn clearance(&self) -> f64 {
        self.clearance
    }
    fn has_hub(&self) -> bool {
        self.hub_observer.is_some()
    }
    fn has_graph(&self) -> bool {
        self.graph.is_some()
    }
    fn prepares_ground(&self) -> bool {
        true
    }
    fn begin_frame(&mut self, frame: &Arc<BgrFrame>) -> Result<(), RunError> {
        if self.pending_panels.is_some() {
            return Err(missing("previous panel preparation is unconsumed"));
        }
        self.panel_timing = None;
        if let Some(panels) = self.panels.clone() {
            let source = Arc::downgrade(frame);
            let image = frame.clone();
            let clock = self.clock.clone();
            let task = self.pool.submit(move || {
                let started = clock.now();
                panels.check(&image)?;
                Ok((started, clock.now()))
            })?;
            self.pending_panels = Some((source, task));
        }
        Ok(())
    }
    fn panel_timing(&self) -> Option<(f64, f64)> {
        self.panel_timing
    }
    fn prepare(
        &mut self,
        frame: &Arc<BgrFrame>,
        acquired: f64,
        hud: &HudReading,
        focused: bool,
    ) -> Result<(), RunError> {
        if let Some((source, task)) = self.pending_panels.take() {
            self.panel_timing = Some(task.join()??);
            if !source.ptr_eq(&Arc::downgrade(frame)) {
                return Err(missing("panel preparation frame differs"));
            }
        }
        Ok(self
            .camera
            .with(|camera| camera.prepare_frame(frame, acquired, hud, focused))?)
    }
    fn ground(&mut self, frame: &Arc<BgrFrame>, acquired: f64) -> Result<GroundReading, RunError> {
        Ok(self.camera.with(|camera| camera.observe(frame, acquired))?)
    }
    fn threats(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
        engage: bool,
    ) -> Result<CurrentThreats, RunError> {
        let mut threats = self.near_threats(frame, player, acquired, aim, engage)?;
        self.finish_threats(&mut threats)?;
        Ok(threats)
    }
    fn near_preempts_hover(&self) -> bool {
        true
    }
    fn near_threats(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
        engage: bool,
    ) -> Result<CurrentThreats, RunError> {
        if self.pending_hover.is_some() {
            return Err(missing("previous hover work is unconsumed"));
        }
        let dangerous = if engage {
            let observer = self.hazards.clone();
            let image = frame.clone();
            let clock = self.clock.clone();
            Some(self.pool.submit(move || {
                observer.observe(image.view(), player, acquired, clock.now(), aim)
            })?)
        } else {
            None
        };
        self.pending_hover = aim
            .map(|aim| {
                let observer = self.hover.clone();
                let image = frame.clone();
                self.pool
                    .submit(move || observer.observe(image.view(), aim))
            })
            .transpose()?;
        let target = if engage {
            self.target
                .as_ref()
                .map(|observer| observer.observe(frame.view()))
                .transpose()?
                .flatten()
        } else {
            None
        };
        let candidates = dangerous
            .map(ObservationTask::join)
            .transpose()?
            .transpose()?
            .unwrap_or_default();
        Ok(CurrentThreats {
            candidates,
            hover: vec![],
            target,
        })
    }
    fn finish_threats(&mut self, threats: &mut CurrentThreats) -> Result<(), RunError> {
        if let Some(task) = self.pending_hover.take() {
            threats.hover = task.join()??;
        }
        Ok(())
    }
    fn hover(
        &mut self,
        frame: &Arc<BgrFrame>,
        aim: [i32; 2],
    ) -> Result<Vec<HoverCandidate>, RunError> {
        Ok(self.hover.observe(frame.view(), aim)?)
    }
    fn hazards(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
        acquired: f64,
        aim: Option<[i32; 2]>,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        Ok(self
            .hazards
            .observe(frame.view(), player, acquired, self.clock.now(), aim)?)
    }
    fn start_plan(
        &mut self,
        frame: &Arc<BgrFrame>,
        request: PlanningRequest<'_>,
    ) -> Result<PlanningJobs<Self::Jobs>, RunError> {
        let path = self.path.clone();
        let image = frame.clone();
        let ground = request.ground.clone();
        let excluded = request.excluded.to_vec();
        let clock = self.clock.clone();
        let graph = self.has_graph();
        let PlanningRequest {
            wing,
            acquired,
            position,
            outbound,
            ..
        } = request;
        let plan = self.pool.submit(move || {
            plan_arcane_step(
                &image,
                wing,
                &mut CameraOnlyGround,
                &path,
                acquired,
                PlanOptions {
                    ground_reading: Some(&ground),
                    allow_no_aim: true,
                    choose: ChooseOptions {
                        position,
                        allow_detour: true,
                        prefer_frontier: !graph,
                        excluded: &excluded,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                clock.as_ref(),
            )
        })?;
        let terminal = if outbound {
            self.terminal_observer
                .as_ref()
                .map(|observer| {
                    let observer = observer.clone();
                    let image = frame.clone();
                    self.terminal_scan.schedule(
                        &self.slow_pool,
                        &self.pool,
                        acquired,
                        self.current_goals || self.terminal.reference_state().0,
                        move || observer.with(|observer| observer.observe(image.view())),
                    )
                })
                .transpose()?
                .flatten()
        } else {
            None
        };
        let objective = if outbound {
            self.objective_observer
                .as_ref()
                .map(|observer| {
                    let observer = observer.clone();
                    let image = frame.clone();
                    self.objective_scan.schedule(
                        &self.slow_pool,
                        &self.pool,
                        acquired,
                        self.current_goals || self.terminal.reference_state().0,
                        move || observer.observe(image.view()),
                    )
                })
                .transpose()?
                .flatten()
        } else {
            None
        };
        if self.slow_confirmation_pending() {
            self.local_field = false;
        }
        let hub = if self.local_field {
            None
        } else {
            self.hub_observer
                .as_mut()
                .map(|observer| observer.observe(frame))
                .transpose()?
                .flatten()
        };
        let hub_evidence = hub.and_then(|_| {
            self.hub_observer
                .as_ref()
                .and_then(|observer| observer.last_evidence().cloned())
        });
        Ok(PlanningJobs {
            jobs: NativePlanningJobs {
                plan: Some(plan),
                terminal,
                objective,
            },
            hub,
            hub_evidence,
        })
    }
    fn objective(&mut self, jobs: &mut Self::Jobs) -> Result<Option<ObjectiveReading>, RunError> {
        Ok(jobs
            .objective
            .take()
            .map(ObservationTask::join)
            .transpose()?
            .transpose()?
            .flatten())
    }
    fn slow_confirmation_pending(&self) -> bool {
        self.terminal_scan.confirmation() || self.objective_scan.confirmation()
    }
    fn planning_diagnostics(&self) -> Value {
        json!({"terminal":self.terminal_scan.diagnostics(),"objective":self.objective_scan.diagnostics(),"local_field":self.local_field,"mapping_current":self.mapping_current,"last_mapping_source_at":self.last_mapping_source})
    }
    fn require_current_goals(&mut self, required: bool) {
        self.current_goals = required;
    }
    fn local_field_phase(&mut self, eligible: bool) {
        self.local_field = eligible && !self.current_goals;
    }
    fn plan(&mut self, jobs: &mut Self::Jobs) -> Result<CurrentPlan<Self::View>, RunError> {
        let StepPlan {
            choice,
            ground,
            view,
        } = jobs
            .plan
            .take()
            .ok_or_else(|| missing("path work already consumed"))?
            .join()??;
        Ok(CurrentPlan {
            choice,
            ground,
            view,
        })
    }
    fn terminal(&mut self, jobs: &mut Self::Jobs) -> Result<Option<TerminalReading>, RunError> {
        Ok(jobs
            .terminal
            .take()
            .map(ObservationTask::join)
            .transpose()?
            .transpose()?
            .flatten())
    }
    fn set_sequence(&mut self, view: &mut Self::View, sequence: u64) {
        view.context.sequence = sequence;
    }
    fn choose(
        &mut self,
        view: &Self::View,
        wing: Wing,
        options: ChooseOptions<'_>,
    ) -> Result<PathChoice, RunError> {
        Ok(self.path.choose(view, wing, options)?)
    }
    fn floor_supports(&self, view: &Self::View, aim: [i32; 2]) -> Result<bool, RunError> {
        Ok(floor_supports_aim(view, aim, self.clearance())?)
    }
    fn cell_pixels(&self, view: &Self::View) -> f64 {
        view.cell_pixels as f64
    }
    fn measure(
        &mut self,
        frame: &Arc<BgrFrame>,
        view: &Self::View,
        hint: Option<[f64; 2]>,
    ) -> Result<(CameraMotion, Vec<TerrainLanding>), RunError> {
        let source = view.context.captured_at;
        if self
            .last_mapping_source
            .is_some_and(|previous| source < previous)
        {
            return Err(ObservationError::invalid("mapping source moved backwards").into());
        }
        if self.local_field
            && self
                .last_mapping_source
                .is_some_and(|previous| source < previous + 0.1)
        {
            self.mapping_current = false;
            return Ok((
                CameraMotion {
                    displacement: None,
                    inliers: 0,
                    reason: crate::common::scene_motion::MotionReason::MappingDeferred,
                    matches: 0,
                    inlier_ratio: None,
                    linear_error: None,
                },
                vec![],
            ));
        }
        self.mapping_current = true;
        let mut motion = self
            .motion
            .take()
            .ok_or_else(|| missing("motion already in flight"))?;
        let masks = PathMotionMasks::new(view)?;
        let image = frame.clone();
        let moving = self.pool.submit(move || {
            let result = motion.observe(image.view(), masks.view(), hint);
            (motion, result)
        })?;
        let patterns = self
            .terrain
            .as_ref()
            .map(|observer| {
                let observer = observer.clone();
                let image = frame.clone();
                self.pool.submit(move || observer.observe(image.view()))
            })
            .transpose()?;
        let (motion, measured) = moving.join()?;
        self.motion = Some(motion);
        let measured = measured?;
        self.last_mapping_source = Some(source);
        let landings = patterns
            .map(ObservationTask::join)
            .transpose()?
            .transpose()?
            .unwrap_or_default();
        Ok((measured, landings))
    }
    fn reset_motion(&mut self) -> Result<(), RunError> {
        self.last_mapping_source = None;
        self.motion_mut()?.reset();
        Ok(())
    }
    fn seed_motion(&mut self, frame: &Arc<BgrFrame>, view: &Self::View) -> Result<(), RunError> {
        let masks = PathMotionMasks::new(view)?;
        self.motion_mut()?
            .observe(frame.view(), masks.view(), None)?;
        self.last_mapping_source = Some(view.context.captured_at);
        self.mapping_current = true;
        Ok(())
    }
    fn track(
        &mut self,
        observation: LandmarkObservation<'_>,
    ) -> Result<TrackedLandmarks, RunError> {
        let LandmarkObservation {
            frame,
            terminal,
            hub,
            evidence,
            acquired,
            displacement,
        } = observation;
        let terminal = if terminal.is_none() && !self.terminal.reference_state().0 {
            None
        } else {
            self.terminal_observer
                .as_ref()
                .map(|observer| {
                    observer.with(|observer| {
                        self.terminal.update(
                            observer,
                            TerminalObservation {
                                frame,
                                reading: terminal,
                                acquired_at: acquired,
                                displacement,
                            },
                        )
                    })
                })
                .transpose()?
                .flatten()
        };
        let (hub, hub_evidence) = if !self.local_field
            && let Some(observer) = &self.hub_observer
        {
            self.hub
                .update(observer, frame, hub, evidence, acquired, displacement)?
        } else {
            (None, None)
        };
        Ok(TrackedLandmarks {
            terminal,
            approach: self.terminal.approach().cloned(),
            approach_displacement: self.terminal.approach_displacement(),
            approach_static_pairs: self.terminal.approach_static_pairs(),
            hub,
            hub_evidence,
        })
    }
    fn reset_terminal(&mut self) {
        self.terminal.reset();
    }
    fn relocalize_terminal(&mut self) {
        self.terminal.require_relocalization();
    }
    fn terminal_reference(&self) -> (bool, Option<f64>) {
        self.terminal.reference_state()
    }
    fn reset_hub(&mut self) {
        self.hub.reset();
    }
    fn current_pair_near(
        &mut self,
        frame: &Arc<BgrFrame>,
        player: [i32; 2],
    ) -> Result<Option<[f64; 2]>, RunError> {
        Ok(self
            .hub_observer
            .as_ref()
            .map(|observer| observer.current_pair_near(frame, player, 100.0))
            .transpose()?
            .flatten()
            .map(|v| v.map(f64::from)))
    }
    fn camera_diagnostics(&self) -> Result<Value, RunError> {
        Ok(self.camera.with(|camera| {
            let d = camera.diagnostics();
            Ok(json!({"camera_center":d.center,"last_camera_reset":d.reset_events.back()}))
        })?)
    }
    fn graph_observe(
        &mut self,
        player: [i32; 2],
        displacement: Option<[f64; 2]>,
        acquired: f64,
    ) -> Result<GraphLocation, RunError> {
        let features = self
            .motion
            .as_ref()
            .and_then(BackgroundMotionObserver::features);
        let graph = self
            .graph
            .as_mut()
            .ok_or_else(|| missing("route graph unavailable"))?;
        if !self.mapping_current {
            return Ok(GraphLocation {
                relocated: false,
                localized: false,
                position: graph.position(),
                places: graph.places().to_vec(),
            });
        }
        let relocated = graph.observe(features, player.map(f64::from), displacement, acquired)?;
        Ok(GraphLocation {
            relocated,
            localized: graph.localized(),
            position: graph.position(),
            places: graph.places().to_vec(),
        })
    }
    fn reobserve_route_graph(
        &mut self,
        view: &Self::View,
        player: [i32; 2],
        acquired: f64,
    ) -> Result<bool, RunError> {
        if self.graph.is_none()
            || !self.mapping_current
            || self.last_mapping_source != Some(acquired)
            || view.context.captured_at != acquired
            || !(0.0..0.25).contains(&(self.clock.now() - acquired))
            || self.stop.is_set()
        {
            return Ok(false);
        }
        let mut graph = ArcaneRouteGraph::default();
        graph.observe(
            self.motion
                .as_ref()
                .and_then(BackgroundMotionObserver::features),
            player.map(f64::from),
            None,
            acquired,
        )?;
        self.graph = Some(graph);
        self.flow = ArcaneRouteFlow::new(None);
        self.clear_steering_aim();
        Ok(true)
    }
    fn graph_choose(
        &mut self,
        view: &Self::View,
        request: RouteChoiceRequest<'_>,
    ) -> Result<PathChoice, RunError> {
        if !request.returning && request.hub.is_none() && !self.graph_position_current() {
            // 위치 대응을 잃으면 과거 port 대신 현재 통로의 굴곡 연속성만 사용한다.
            let global = self.path.choose(
                view,
                request.wing,
                ChooseOptions {
                    allow_detour: true,
                    prefer_frontier: true,
                    excluded: request.excluded,
                    ..Default::default()
                },
            )?;
            return Ok(self.flow.choose(
                &self.path,
                view,
                request.wing,
                &global,
                request.now,
                RouteFlowOptions {
                    position: [0.0, 0.0],
                    visited: &[],
                    excluded: request.excluded,
                },
            )?);
        }
        if !self.mapping_current {
            // 관측하지 않은 pose/port를 현재 좌표로 투영하지 않고 같은 frame의 floor로만 고른다.
            return Ok(self.path.choose(
                view,
                request.wing,
                ChooseOptions {
                    allow_detour: true,
                    prefer_frontier: true,
                    excluded: request.excluded,
                    ..Default::default()
                },
            )?);
        }
        let graph = self
            .graph
            .as_mut()
            .ok_or_else(|| missing("route graph unavailable"))?;
        let choice = graph.choose(
            &self.path,
            view,
            request.wing,
            request.now,
            GraphChooseOptions {
                excluded: request.excluded,
                landings: request.landings,
                hub: request.hub.map(|v| v.map(f64::from)),
                prefer_goal_exit: request.returning,
            },
        )?;
        if !request.returning
            && let Some(aim) = choice.aim
        {
            // 현재 graph가 고른 통로를 이어받아 위치 대응이 끊겨도 굴곡을 잃지 않는다.
            let heading = ArcaneRouteFlow::axis([
                f64::from(aim[0]) - view.context.anchor[0],
                f64::from(aim[1]) - view.context.anchor[1],
            ]);
            self.flow =
                ArcaneRouteFlow::new((heading != request.wing.heading()).then_some(heading));
        }
        Ok(choice)
    }
    fn graph_fail_landing(
        &mut self,
        aim: Option<[i32; 2]>,
        player: [i32; 2],
    ) -> Result<(), RunError> {
        if self.mapping_current
            && let Some(graph) = &mut self.graph
        {
            graph.fail_landing(aim, player.map(f64::from))?;
        }
        Ok(())
    }
    fn graph_summary(&self) -> Option<Value> {
        self.graph.as_ref().map(ArcaneRouteGraph::summary)
    }
    fn graph_position_current(&self) -> bool {
        self.mapping_current && self.graph.as_ref().is_none_or(|graph| graph.localized())
    }
    fn flow_choose(
        &mut self,
        view: &Self::View,
        wing: Wing,
        global: &PathChoice,
        now: f64,
        options: RouteFlowOptions<'_>,
    ) -> Result<PathChoice, RunError> {
        Ok(self
            .flow
            .choose(&self.path, view, wing, global, now, options)?)
    }
    fn flow_state(&self) -> (Option<[f64; 2]>, String) {
        (self.flow.detour(), self.flow.reason().into())
    }
    fn clear_steering_aim(&mut self) {
        if let Some(steering) = &mut self.steering {
            steering.clear_aim();
        }
    }
    fn steering_aim(&self) -> Option<[i32; 2]> {
        self.steering.as_ref().and_then(ContinuousSteering::aim)
    }
    fn steer(
        &mut self,
        view: &Self::View,
        choice: &PathChoice,
        now: f64,
    ) -> Result<(bool, &'static str), RunError> {
        Ok(self
            .steering
            .as_mut()
            .ok_or_else(|| missing("steering not initialized"))?
            .update(view, choice, now)?)
    }
    fn comparison(&mut self, frame: &Arc<BgrFrame>) -> Result<Vec<u8>, RunError> {
        let patch = frame
            .view()
            .crop(PixelRect::new(120, 180, 1720, 890).map_err(ObservationError::from)?)
            .map_err(ObservationError::from)?;
        let gray = color::gray(patch).map_err(ObservationError::from)?;
        Ok(resize::resize(
            gray.view(),
            Size::new(160, 71).map_err(ObservationError::from)?,
            Interpolation::Linear,
        )
        .map_err(ObservationError::from)?
        .pixels()
        .to_vec())
    }
    fn finish(&mut self, timeout: Duration) -> Result<(), RunError> {
        let fast = self.pool.finish(timeout).map_err(RunError::from);
        let slow = self.slow_pool.finish(timeout).map_err(RunError::from);
        fast?;
        slow?;
        self.pool_finished = true;
        let panels = self
            .pending_panels
            .take()
            .map(|(_, task)| task.join())
            .transpose();
        let hover = self
            .pending_hover
            .take()
            .map(ObservationTask::join)
            .transpose();
        let terminal = self.terminal_scan.drain();
        let objective = self.objective_scan.drain();
        panels?.transpose()?;
        hover?.transpose()?;
        terminal?;
        objective?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        runs::summoner::{
            camera_ground::{CalibratedCameraGroundObserver, CameraLimits},
            navigation_execution::load_arcane_step_planner,
        },
        test_field_world::World,
    };
    use std::{error::Error, path::Path, sync::mpsc};
    type TestResult = Result<(), Box<dyn Error>>;
    #[test]
    fn initial_stair_corner_uses_a_current_short_side_exit() -> TestResult {
        use crate::common::vision_ops::codec;
        use crate::runs::summoner::navigation::PathContext;
        let (mut vision, _) = fixture()?;
        vision.initialize(0.32, None, None)?;
        let frame = codec::load_color(Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-stair-cap-short-side-exit.png",
        ))?;
        let view = vision.path.observe(
            frame.view(),
            PathContext {
                sequence: 147,
                generation: 0,
                captured_at: 1.0,
                frame_size: frame.size(),
                viewport: PixelRect::new(242, 55, 1682, 900)?,
                anchor: [962.0, 580.0],
            },
        )?;
        let current = vision.graph_choose(
            &view,
            RouteChoiceRequest {
                wing: Wing::North,
                now: 1.05,
                excluded: &[],
                landings: &[],
                hub: None,
                returning: false,
            },
        )?;
        assert_eq!(vision.flow.reason(), "observed_corner_probe", "{current:?}");
        let aim = current.aim.ok_or("short current exit")?;
        assert_eq!(
            ArcaneRouteFlow::axis([f64::from(aim[0]) - 962.0, f64::from(aim[1]) - 580.0]),
            Wing::East.heading()
        );
        assert!(vision.floor_supports(&view, aim)?);
        let excluded = [[
            f64::from(aim[0]) - 40.0,
            f64::from(aim[1]) - 40.0,
            f64::from(aim[0]) + 40.0,
            f64::from(aim[1]) + 40.0,
        ]];
        vision.flow = ArcaneRouteFlow::new(None);
        let alternate = vision.graph_choose(
            &view,
            RouteChoiceRequest {
                wing: Wing::North,
                now: 1.05,
                excluded: &excluded,
                landings: &[],
                hub: None,
                returning: false,
            },
        )?;
        if let Some(point) = alternate.aim {
            let [x, y] = point.map(f64::from);
            let [left, top, right, bottom] = excluded[0];
            assert!(x < left || x > right || y < top || y > bottom);
        }
        vision.flow = ArcaneRouteFlow::new(None);
        let stale = vision.graph_choose(
            &view,
            RouteChoiceRequest {
                wing: Wing::North,
                now: 1.25,
                excluded: &[],
                landings: &[],
                hub: None,
                returning: false,
            },
        )?;
        assert!(stale.aim.is_none());
        let blank = PathView::from_grid(
            view.context,
            view.cell_pixels,
            crate::common::frame::GrayFrame::zeros(view.floor().size())?,
        )?;
        vision.flow = ArcaneRouteFlow::new(None);
        assert!(
            vision
                .graph_choose(
                    &blank,
                    RouteChoiceRequest {
                        wing: Wing::North,
                        now: 1.05,
                        excluded: &[],
                        landings: &[],
                        hub: None,
                        returning: false,
                    }
                )?
                .aim
                .is_none()
        );
        vision.finish(Duration::from_secs(2))?;
        Ok(())
    }
    #[test]
    fn graph_to_floor_transition_keeps_the_current_stair_bend() -> TestResult {
        use crate::common::vision_ops::codec;
        use crate::runs::summoner::navigation::PathContext;
        let (mut vision, _) = fixture()?;
        vision.initialize(0.32, None, Some(Wing::West.heading()))?;
        let owner = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation");
        let frame = codec::load_color(owner.join("north-descending-stair-graph-current.png"))?;
        let view = vision.path.observe(
            frame.view(),
            PathContext {
                sequence: 25,
                generation: 0,
                captured_at: 1.0,
                frame_size: frame.size(),
                viewport: PixelRect::new(244, 55, 1684, 900)?,
                anchor: [964.0, 577.0],
            },
        )?;
        let graph = vision.graph.as_mut().ok_or("graph")?;
        graph.observe(None, view.context.anchor, None, 1.0)?;
        // R02의 직전 현재 graph가 선택한 West 통로를 같은 바닥에 복원한다.
        graph.choose(
            &vision.path,
            &view,
            Wing::West,
            1.05,
            GraphChooseOptions::default(),
        )?;
        let request = |now| RouteChoiceRequest {
            wing: Wing::North,
            now,
            excluded: &[],
            landings: &[],
            hub: None,
            returning: false,
        };
        assert!(vision.graph_position_current());
        vision.graph_choose(&view, request(1.05))?;
        assert_eq!(vision.flow.detour(), Some(Wing::West.heading()));
        vision.mapping_current = false;
        let frame =
            codec::load_color(owner.join("north-descending-stair-localization-transition.png"))?;
        let view = vision.path.observe(
            frame.view(),
            PathContext {
                sequence: 26,
                generation: 0,
                captured_at: 1.1,
                frame_size: frame.size(),
                viewport: PixelRect::new(242, 55, 1682, 900)?,
                anchor: [962.0, 578.0],
            },
        )?;
        let current = vision.graph_choose(&view, request(1.15))?;
        assert_eq!(vision.flow.reason(), "observed_corner_turn");
        let aim = current.aim.ok_or("current stair floor")?;
        assert_eq!(
            ArcaneRouteFlow::axis([f64::from(aim[0]) - 962.0, f64::from(aim[1]) - 578.0,]),
            Wing::South.heading()
        );
        let stale = vision.graph_choose(&view, request(1.35))?;
        assert!(stale.aim.is_none());
        assert_eq!(stale.reason, "route_frame_stale");
        vision.finish(Duration::from_secs(1))?;
        Ok(())
    }
    #[test]
    fn unlocalized_outbound_uses_current_floor_instead_of_old_exit_intent() -> TestResult {
        use crate::common::frame::GrayFrame;
        use crate::runs::summoner::navigation::PathContext;
        let (mut vision, _) = fixture()?;
        vision.initialize(0.32, None, None)?;
        let view = PathView::from_grid(
            PathContext {
                sequence: 1,
                generation: 0,
                captured_at: 0.0,
                frame_size: Size::new(1600, 800)?,
                viewport: PixelRect::new(0, 0, 1600, 800)?,
                anchor: [800.0, 400.0],
            },
            8,
            GrayFrame::from_vec(Size::new(200, 100)?, vec![1; 200 * 100])?,
        )?;
        let graph = vision.graph.as_mut().ok_or("graph")?;
        let old = graph.choose(
            &vision.path,
            &view,
            Wing::West,
            0.1,
            GraphChooseOptions::default(),
        )?;
        assert_eq!(old.reason, "graph_observed_exit_west");
        let before = vision.graph_summary();
        assert!(!vision.graph_position_current());
        let next = vision.graph_choose(
            &view,
            RouteChoiceRequest {
                wing: Wing::North,
                now: 0.1,
                excluded: &[],
                landings: &[],
                hub: None,
                returning: false,
            },
        )?;
        let point = next.aim.ok_or("current floor aim")?;
        assert_eq!(
            ArcaneRouteFlow::axis([f64::from(point[0]) - 800.0, f64::from(point[1]) - 400.0]),
            Wing::North.heading()
        );
        assert_eq!(vision.graph_summary(), before);
        let returning = vision.graph_choose(
            &view,
            RouteChoiceRequest {
                wing: Wing::North,
                now: 0.1,
                excluded: &[],
                landings: &[],
                hub: None,
                returning: true,
            },
        )?;
        assert!(returning.reason.starts_with("graph_observed_exit_"));
        vision.finish(Duration::from_secs(2))?;
        Ok(())
    }
    #[test]
    fn route_reobservation_requires_current_mapping_and_drops_old_chart() -> TestResult {
        let (mut vision, world) = fixture()?;
        vision.initialize(0.32, None, None)?;
        let frame = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
        let at = world.now();
        let view = vision.path.observe(
            frame.view(),
            crate::runs::summoner::navigation::PathContext {
                sequence: 1,
                generation: 0,
                captured_at: at,
                frame_size: frame.size(),
                viewport: PixelRect::new(120, 180, 1720, 890)?,
                anchor: [962.0, 580.0],
            },
        )?;
        assert!(!vision.reobserve_route_graph(&view, [962, 580], at)?);
        vision.seed_motion(&frame, &view)?;
        let graph = vision.graph.as_mut().ok_or("graph")?;
        graph.observe(None, [962.0, 580.0], None, at - 0.2)?;
        graph.observe(None, [962.0, 580.0], Some([240.0, 0.0]), at - 0.1)?;
        assert_ne!(graph.position(), [0.0, 0.0]);
        assert!(vision.reobserve_route_graph(&view, [962, 580], at)?);
        assert_eq!(vision.graph.as_ref().ok_or("graph")?.position(), [0.0, 0.0]);
        assert_eq!(vision.graph.as_ref().ok_or("graph")?.places().len(), 1);
        let stable = vision.graph_summary();
        assert!(!vision.reobserve_route_graph(&view, [962, 580], at + 0.1)?);
        vision.mapping_current = false;
        assert!(!vision.reobserve_route_graph(&view, [962, 580], at)?);
        vision.mapping_current = true;
        world.advance(0.25);
        assert!(!vision.reobserve_route_graph(&view, [962, 580], at)?);
        assert_eq!(vision.graph_summary(), stable);
        vision.finish(Duration::from_secs(2))?;
        Ok(())
    }
    fn fixture() -> Result<(NativeContinuousVision, World), Box<dyn Error>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
        let (ground, path) = load_arcane_step_planner(root, true, "other")?;
        let world = World::new(&json!({}));
        let observers = ContinuousObservers {
            panels: None,
            camera: CameraGroundHandle::new(CalibratedCameraGroundObserver::new(
                ground,
                |_| Ok(false),
                CameraLimits::default(),
            )?),
            path,
            hazards: ArcaneHazardHandle::new(
                crate::runs::summoner::scene_vision::ArcaneHazardObserver::load(
                    owner.join("manifest.json"),
                )?,
            ),
            hover: Arc::new(ArcaneHoverObserver::load(root.join("assets"))?),
            target: Some(Arc::new(ArcaneTargetObserver::load(&owner)?)),
            hub: Some(ArcaneHubObserver::load(owner.join("hub-patterns.json"))?),
            graph: Some(ArcaneRouteGraph::new(12, 96, 120.0, 300.0)?),
            terrain: None,
            terminal: Some(ArcaneTerminalObserver::load(
                owner.join("terminal-patterns.json"),
            )?),
            objective: Some(Arc::new(ArcaneObjectiveObserver::load(&owner)?)),
        };
        Ok((
            NativeContinuousVision::new(observers, Arc::new(world.clone()), world.1.clone())?,
            world,
        ))
    }
    #[test]
    fn native_parallel_pipeline_releases_frames_and_keeps_graph_between_intervals() -> TestResult {
        let (mut vision, world) = fixture()?;
        for sequence in 0..2 {
            let previous = vision.graph_summary();
            vision.initialize(0.32, None, None)?;
            assert_eq!(vision.graph_summary(), previous);
            assert!(vision.initialize(0.32, None, None).is_err());
            let frame = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
            let weak = Arc::downgrade(&frame);
            let acquired = world.now();
            let threats = vision.threats(&frame, [750, 580], acquired, Some([1300, 300]), true)?;
            assert!(threats.candidates.is_empty());
            assert!(threats.hover.is_empty());
            assert!(threats.target.is_none());
            let ground = GroundReading {
                point: Some([750, 580]),
                reason: "fixture".into(),
                score: None,
                calibration: None,
            };
            let PlanningJobs {
                mut jobs,
                hub,
                hub_evidence,
            } = vision.start_plan(
                &frame,
                PlanningRequest {
                    wing: Wing::North,
                    acquired,
                    position: [0.0; 2],
                    excluded: &[],
                    ground: &ground,
                    outbound: true,
                },
            )?;
            assert!(vision.objective(&mut jobs)?.is_none());
            let mut plan = vision.plan(&mut jobs)?;
            vision.set_sequence(&mut plan.view, 1);
            let terminal = vision.terminal(&mut jobs)?;
            let (motion, _) = vision.measure(&frame, &plan.view, None)?;
            let landmarks = vision.track(LandmarkObservation {
                frame: &frame,
                terminal,
                hub,
                evidence: hub_evidence,
                acquired,
                displacement: motion.displacement,
            })?;
            assert!(landmarks.terminal.is_none());
            vision.graph_observe([750, 580], motion.displacement, acquired)?;
            assert!(vision.pending_hover.is_none());
            vision.near_threats(&frame, [750, 580], acquired, Some([1300, 300]), true)?;
            assert!(vision.pending_hover.is_some());
            assert!(
                vision
                    .near_threats(&frame, [750, 580], acquired, None, true)
                    .is_err()
            );
            let mut current = CurrentThreats::default();
            vision.finish_threats(&mut current)?;
            assert!(vision.pending_hover.is_none());
            vision.local_field_phase(true);
            world.advance(0.05);
            let mut next = plan.view.clone();
            next.context.captured_at = world.now();
            let previous = vision.graph_summary();
            let (deferred, landings) = vision.measure(&frame, &next, None)?;
            assert_eq!(
                deferred.reason,
                crate::common::scene_motion::MotionReason::MappingDeferred
            );
            assert!(deferred.displacement.is_none() && landings.is_empty());
            assert!(
                !vision
                    .graph_observe([750, 580], None, world.now())?
                    .localized
            );
            assert_eq!(vision.graph_summary(), previous);
            let choice = vision.graph_choose(
                &next,
                RouteChoiceRequest {
                    wing: Wing::North,
                    now: world.now(),
                    excluded: &[],
                    landings: &[],
                    hub: None,
                    returning: false,
                },
            )?;
            assert!(choice.aim.is_none());
            world.advance(0.06);
            next.context.captured_at = world.now();
            assert_ne!(
                vision.measure(&frame, &next, None)?.0.reason,
                crate::common::scene_motion::MotionReason::MappingDeferred
            );
            vision.require_current_goals(true);
            vision.local_field_phase(true);
            world.advance(0.01);
            next.context.captured_at = world.now();
            assert_ne!(
                vision.measure(&frame, &next, None)?.0.reason,
                crate::common::scene_motion::MotionReason::MappingDeferred
            );
            vision.finish(Duration::from_secs(2))?;
            drop(jobs);
            drop(plan);
            drop(landmarks);
            drop(frame);
            assert!(weak.upgrade().is_none(), "scope {sequence}");
            world.advance(1.0);
        }
        Ok(())
    }
    #[test]
    fn pending_panel_work_is_bounded_and_releases_its_frame_on_finish() -> TestResult {
        let (mut vision, _) = fixture()?;
        vision.panels = Some(PanelObservers::load(Path::new(env!("CARGO_MANIFEST_DIR")))?);
        vision.initialize(0.32, None, None)?;
        let frame = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
        let weak = Arc::downgrade(&frame);
        vision.begin_frame(&frame)?;
        assert!(vision.begin_frame(&frame).is_err());
        drop(frame);
        vision.finish(Duration::from_secs(2))?;
        assert!(vision.pending_panels.is_none());
        assert!(weak.upgrade().is_none());
        Ok(())
    }
    #[test]
    fn panel_preparation_cannot_be_applied_to_a_different_frame() -> TestResult {
        let (mut vision, mut world) = fixture()?;
        vision.panels = Some(PanelObservers::load(Path::new(env!("CARGO_MANIFEST_DIR")))?);
        vision.initialize(0.32, None, None)?;
        let frame = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
        let different = Arc::new(frame.as_ref().clone());
        let hud = world.hud(&different)?;
        vision.begin_frame(&frame)?;
        assert!(
            vision
                .prepare(&different, 0.0, &hud, true)
                .is_err_and(|e| e.to_string().contains("panel preparation frame differs"))
        );
        assert!(vision.pending_panels.is_none());
        vision.finish(Duration::from_secs(2))?;
        Ok(())
    }
    #[test]
    fn unconsumed_invalid_panel_frame_reports_error_after_actual_pool_join() -> TestResult {
        let (mut vision, _) = fixture()?;
        vision.panels = Some(PanelObservers::load(Path::new(env!("CARGO_MANIFEST_DIR")))?);
        vision.initialize(0.32, None, None)?;
        let frame = Arc::new(BgrFrame::zeros(Size::new(1, 1)?)?);
        let weak = Arc::downgrade(&frame);
        vision.begin_frame(&frame)?;
        drop(frame);
        assert!(vision.finish(Duration::from_secs(2)).is_err());
        assert!(vision.pool_finished && vision.pool.is_finished());
        assert!(vision.pending_panels.is_none());
        assert!(weak.upgrade().is_none());
        Ok(())
    }
    #[test]
    fn native_pending_pool_must_finish_before_next_interval() -> TestResult {
        let (mut vision, _) = fixture()?;
        vision.initialize(0.32, None, None)?;
        assert!(vision.reset_route_memory().is_err());
        let (started_rx_send, started_rx) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let task = vision.pool.submit(move || {
            let _ = started_rx_send.send(());
            blocked.recv().map_err(|e| e.to_string())
        })?;
        started_rx.recv_timeout(Duration::from_secs(2))?;
        assert!(vision.finish(Duration::from_millis(1)).is_err());
        assert!(vision.initialize(0.32, None, None).is_err());
        release.send(())?;
        assert!(task.join()?.is_ok());
        vision.finish(Duration::from_secs(2))?;
        vision.reset_route_memory()?;
        assert_eq!(
            vision.graph_summary(),
            Some(ArcaneRouteGraph::default().summary())
        );
        vision.initialize(0.32, None, None)?;
        vision.finish(Duration::from_secs(2))?;
        Ok(())
    }
}
