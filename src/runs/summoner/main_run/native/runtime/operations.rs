use super::*;
use crate::{
    actions::{
        runtime::{ActionContext, PointerStyle},
        town_loop::{Destination, TownVision},
    },
    common::{encounters::VisualCandidate, frame::BgrFrame, survival_vision::HudReading},
    runs::summoner::{
        appearance_vision::SummonerAppearanceObserver,
        camera_ground::CameraGroundHandle,
        canyon_finish::{CanyonRequest, enter_canyon},
        continuous_navigation::{
            Continuation, ContinuousObservers, ContinuousRequest, run_continuous_route,
        },
        encounter_execution::{EncounterEvidence, EncounterRequest, run_platform_encounter},
        field_defense::{DefenseVision, PausedRequest, maintain_paused_field},
        legacy_segment::{LegacyRequest, run_north_segment},
        life_vision::SummonerLifeTracker,
        main_run::{InitialField, NavigationInterval, SummonerRuntime},
        room_finish::{NativeTownFinish, exit_to_character, return_to_act1_and_exit},
        route_graph::ArcaneRouteGraph,
        scene_vision::GroundReading,
    },
};

/// 대기 방어에는 경로 graph나 관찰 pool을 새로 만들지 않는다.
struct CameraDefense {
    camera: CameraGroundHandle,
    hazards: ArcaneHazardHandle,
    clock: Arc<dyn MonotonicClock>,
}
impl DefenseVision for CameraDefense {
    fn prepares_ground(&self) -> bool {
        true
    }
    fn prepare(&mut self, f: &Arc<BgrFrame>, at: f64, hud: &HudReading) -> Result<(), RunError> {
        Ok(self
            .camera
            .with(|camera| camera.prepare_frame(f, at, hud, true))?)
    }
    fn ground(&mut self, f: &Arc<BgrFrame>, at: f64) -> Result<GroundReading, RunError> {
        Ok(self.camera.with(|camera| camera.observe(f, at))?)
    }
    fn hazards(
        &mut self,
        f: &Arc<BgrFrame>,
        p: [i32; 2],
        at: f64,
    ) -> Result<Vec<VisualCandidate>, RunError> {
        Ok(self
            .hazards
            .observe(f.view(), p, at, self.clock.now(), None)?)
    }
}
impl WindowsSummonerRuntime {
    fn new_probe(&mut self, enabled: bool, radius: [f64; 2]) -> Result<(), RunError> {
        self.field.probe = None;
        if let Some(target) = self.target.clone().filter(|_| enabled) {
            let observer: super::super::world::TargetReader =
                Box::new(move |frame| Ok(target.observe(frame)?));
            let core = self.field.core()?;
            let input = core.input();
            let guard = core.guards().window;
            self.field.probe = Some(
                crate::common::hostile_probe::NearbyHostileProbe::new(
                    observer,
                    input,
                    [0, 0],
                    self.context.clock.clone(),
                )
                .with_mapper(move |p| guard.to_desktop(p.map(f64::from)))
                .with_radius(radius)?,
            );
        }
        Ok(())
    }
    fn continuous_vision(&mut self) -> Result<(), RunError> {
        if self.continuous.is_none() {
            let live = self
                .live
                .as_mut()
                .ok_or_else(|| missing("continuous assets"))?;
            self.continuous = Some(NativeContinuousVision::new(
                ContinuousObservers {
                    panels: Some(self.panels.clone()),
                    camera: self.field.camera.clone(),
                    path: self.path.clone(),
                    hazards: self.field.hazards.clone(),
                    hover: self.hover.clone(),
                    target: self.target.clone(),
                    hub: self.hub.take(),
                    graph: Some(ArcaneRouteGraph::default()),
                    terrain: Some(live.terrain.clone()),
                    terminal: live.terminal.take(),
                    objective: Some(live.objective.clone()),
                },
                self.context.clock.clone(),
                self.stop.clone(),
            )?);
        }
        Ok(())
    }
}
impl SummonerRuntime for WindowsSummonerRuntime {
    fn clock(&self) -> f64 {
        self.context.clock.now()
    }
    fn stopped(&self) -> bool {
        self.stop.is_set()
    }
    fn stop(&self) {
        self.stop.set();
        self.gate.stop();
    }
    fn focused(&self) -> Result<bool, ControlError> {
        self.field
            .core
            .as_ref()
            .map_or(Ok(false), |core| core.guards().window.focused())
    }
    fn has_capture(&self) -> bool {
        self.field.core.is_some() || self.pending_capture.is_some()
    }
    fn has_backend(&self) -> bool {
        self.backend_ready
    }
    fn input_events(&self) -> Result<Value, RunError> {
        if !self.backend_ready {
            return Ok(serde_json::json!([]));
        }
        self.field.core.as_ref().map_or_else(
            || Ok(serde_json::json!([])),
            |core| Ok(core.input().events()?),
        )
    }
    fn snapshot(&mut self, report: &mut Value) -> Result<(), RunError> {
        self.snapshot_records(report, false)
    }
    fn initialize(
        &mut self,
        options: &SummonerOptions,
        _selected: &SelectedRun,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<InitialField, RunError> {
        self.initialize_native(options, report, evidence)
    }
    fn observe_runtime(
        &mut self,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        let mut world = self.field.world(LootContext::Navigation)?;
        observe_runtime(
            &mut world,
            self.area.as_mut().ok_or_else(|| missing("area observer"))?,
            &self.recipe.character_key,
            evidence,
        )
    }
    fn navigate(
        &mut self,
        interval: NavigationInterval<'_>,
        progress: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        self.check_inputs()?;
        self.field.deadline = Some(interval.buff_expires_at);
        let policy = self.field.policy.clone();
        if interval.options.continuous {
            if self.evidence_stream.is_none() {
                self.evidence_stream = Some(EvidenceStream::new(interval.options.output.clone())?);
            }
            let channel = self
                .evidence_stream
                .as_ref()
                .ok_or_else(|| missing("navigation evidence writer"))?
                .channel(interval.evidence_prefix);
            self.field.core_mut()?.set_navigation_evidence(channel);
            self.new_probe(
                interval.options.engage_monsters,
                self.navigation_reaction_radius,
            )?;
            self.continuous_vision()?;
            self.field.core_mut()?.enable_latest_capture()?;
            let mut inputs = self.field.core()?.input_session(
                &self.recipe.movement_key,
                &self.recipe.attack_key,
                Some(self.recipe.reference_seconds),
                // Native 포인터는 실제 좌표를 확인한다. 중복 고정 대기 없이 같은 hold를 조준한다.
                0.0,
            )?;
            let result = {
                let mut world = self.field.world(LootContext::Navigation)?;
                run_continuous_route(
                    &mut inputs,
                    &mut world,
                    self.continuous
                        .as_mut()
                        .ok_or_else(|| missing("continuous vision"))?,
                    ContinuousRequest {
                        policy: &policy,
                        proof: interval.proof,
                        seconds: f64::from(interval.options.field_seconds),
                        reference_seconds: self.recipe.reference_seconds,
                        wing: interval.wing,
                        return_from: interval.return_from,
                        engage_monsters: interval.options.engage_monsters,
                        buff_expires_at: Some(interval.buff_expires_at),
                        continuation: interval.transition.map_or_else(
                            || {
                                if interval.continue_current {
                                    Continuation::Current
                                } else {
                                    Continuation::Initial
                                }
                            },
                            Continuation::Transition,
                        ),
                    },
                    progress,
                    evidence,
                )
            };
            self.finish_inputs(inputs, result)
        } else {
            if self.legacy.is_none() {
                self.legacy = Some(NativeLegacyVision {
                    ground: Box::new(self.field.camera.clone()),
                    path: self.path.clone(),
                    hub: Some(TownVision::load(&self.context.root)?),
                    hazards: Some(self.field.hazards.clone()),
                    hover: Some(self.hover.clone()),
                    clock: self.context.clock.clone(),
                });
            }
            let backend = self.field.core()?.input().clone();
            let guards = self.field.core()?.guards().clone();
            let mut world = self.field.world(LootContext::Navigation)?;
            run_north_segment(
                backend.as_ref(),
                &guards,
                &mut world,
                self.legacy
                    .as_mut()
                    .ok_or_else(|| missing("legacy vision"))?,
                LegacyRequest {
                    policy: &policy,
                    proof: interval.proof,
                    casts: usize::from(interval.options.casts),
                    reference_seconds: self.recipe.reference_seconds,
                    buff_expires_at: Some(interval.buff_expires_at),
                    movement_key: &self.recipe.movement_key,
                },
                progress,
                evidence,
            )
        }
    }
    fn renew(
        &mut self,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        self.automatic(
            report["buff_renewals"].as_array().map_or(0, Vec::len),
            evidence,
        )
    }
    fn reset_route_memory(&mut self) -> Result<(), RunError> {
        if let Some(vision) = &mut self.continuous {
            vision.reset_route_memory()?;
        }
        Ok(())
    }
    fn encounter(
        &mut self,
        buff_expires_at: f64,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        self.check_inputs()?;
        self.field.deadline = Some(buff_expires_at);
        self.new_probe(true, [300.0, 180.0])?;
        if self.encounter_vision.is_none() {
            let owner = self
                .context
                .root
                .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
            self.encounter_vision = Some(NativeEncounterVision::new(
                self.field.camera.clone(),
                Arc::new(self.path.clone()),
                self.live
                    .as_ref()
                    .ok_or_else(|| missing("continuous assets"))?
                    .objective
                    .clone(),
                Arc::new(SummonerAppearanceObserver::load(
                    owner.join("summoner-encounter.json"),
                )?),
                self.stop.clone(),
            )?);
        }
        let policy = self.field.policy.clone();
        let mut life = SummonerLifeTracker::new(&self.recipe.character_key, &self.room)?;
        let mut inputs = self.field.core()?.input_session(
            &self.recipe.movement_key,
            &self.recipe.attack_key,
            Some(self.recipe.reference_seconds),
            0.0,
        )?;
        let result = {
            let mut world = self.field.world(LootContext::Encounter)?;
            run_platform_encounter(
                &mut inputs,
                &mut world,
                self.encounter_vision
                    .as_mut()
                    .ok_or_else(|| missing("encounter vision"))?,
                EncounterRequest {
                    policy: &policy,
                    buff_expires_at,
                    reference_seconds: self.recipe.reference_seconds,
                },
                Some(&mut life),
                evidence,
            )
            .and_then(|report| Ok(serde_json::to_value(report)?))
        };
        // finish가 닫은 관찰 pool은 재진입에 재사용하지 않는다.
        if result.is_ok() {
            self.encounter_vision = None;
        }
        let result = self.finish_inputs(inputs, result)?;
        self.last_encounter = result.clone();
        Ok(result)
    }
    fn enter_canyon(
        &mut self,
        buff_expires_at: f64,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        self.check_inputs()?;
        if self.canyon.is_none() {
            self.canyon = Some(NativeCanyonVision::new(
                &self.context.root,
                self.field.camera.clone(),
                self.path.clone(),
            )?);
        }
        let policy = self.field.policy.clone();
        let core = self.field.core_mut()?;
        let input = core.input().clone();
        let guards = core.guards().clone();
        enter_canyon(
            input.as_ref(),
            &guards,
            core,
            self.canyon
                .as_mut()
                .ok_or_else(|| missing("Canyon vision"))?,
            CanyonRequest {
                policy: &policy,
                kill_confirmed: self.last_encounter["kill_confirmed"] == true,
                loot_complete: self.last_encounter["loot_complete"] == true,
                buff_expires_at,
                reference_seconds: self.recipe.reference_seconds,
                movement_key: &self.recipe.movement_key,
            },
            evidence,
        )
    }
    fn finish_room(
        &mut self,
        destination: Destination,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<Value, RunError> {
        self.check_inputs()?;
        let core = self.field.core_mut()?;
        let guards = core.guards().clone();
        let panels = core.panels().clone();
        let emit = self.context.emit.clone();
        panels.with_town(|town| {
            let backend = BorrowedTownBackend::new(core)?;
            let mut context =
                ActionContext::new(backend, Cancellation::default(), PointerStyle::Lifecycle);
            let result = (|| {
                let mut runtime = NativeTownFinish::new(&mut context, town, destination)?;
                return_to_act1_and_exit(&guards, &mut runtime, evidence, |text| emit(text))
            })();
            combine(result, context.close().map_err(RunError::from))
        })
    }
    fn pause(
        &mut self,
        report: &mut Value,
        buff: &mut Option<f64>,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<(), RunError> {
        if self.driver.weapon_state_unverified() {
            self.snapshot_records(report, false)?;
            return Err(RunError::Runtime("buff_weapon_state_unverified".into()));
        }
        self.check_inputs()?;
        self.field.deadline = *buff;
        self.new_probe(self.recipe.engage_monsters, [300.0, 180.0])?;
        let policy = self.field.policy.clone();
        let mut inputs = self.field.core()?.input_session(
            &self.recipe.movement_key,
            &self.recipe.attack_key,
            None,
            0.0,
        )?;
        let mut defense = CameraDefense {
            camera: self.field.camera.clone(),
            hazards: self.field.hazards.clone(),
            clock: self.context.clock.clone(),
        };
        let encounter_recheck = if self.last_encounter["status"] == "kill_unconfirmed" {
            Some(EncounterRecheck {
                objective: self.live.as_ref().ok_or_else(|| missing("continuous assets"))?.objective.clone(),
                appearance: Arc::new(SummonerAppearanceObserver::load(self.context.root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/summoner-encounter.json"))?),
                next_at: 0.0,
            })
        } else {
            None
        };
        let (result, deadline) = {
            let world = self.field.world(LootContext::Paused)?;
            let mut paused = PausedField {
                world,
                driver: &mut self.driver,
                area: self.area.as_mut().ok_or_else(|| missing("area observer"))?,
                recipe: &self.recipe,
                retained_inputs: &mut self.retained_inputs,
                encounter_recheck,
            };
            let result = maintain_paused_field(
                &mut inputs,
                &mut paused,
                Some(&mut defense),
                PausedRequest {
                    policy: &policy,
                    buff_expires_at: *buff,
                    engage_monsters: self.recipe.engage_monsters,
                },
                report,
                evidence,
            );
            (result, paused.world.loot.deadline)
        };
        self.field.deadline = deadline;
        *buff = deadline;
        self.finish_inputs(inputs, result)
    }
    fn safe_exit(
        &mut self,
        diagnostics: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Result<(), RunError> {
        self.check_inputs()?;
        let core = self.field.core_mut()?;
        let guards = core.guards().clone();
        let input = core.input().clone();
        let panels = core.panels().clone();
        panels.with_town(|town| {
            exit_to_character(input.as_ref(), &guards, core, town, evidence, diagnostics)
        })
    }
    fn cleanup(
        &mut self,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Vec<String> {
        self.cleanup_native(report, evidence)
    }
}
