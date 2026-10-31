//! 방 하나의 가변 상태와 Windows 자원의 최종 소유자.
mod cleanup;
mod initialize;
mod operations;
#[cfg(test)]
mod tests;
use super::super::evidence_stream::EvidenceStream;
use super::*;
use crate::{
    actions::{loot_pickup::LootScanner, session::ActionSession},
    common::{
        automap_vision::AutomapObserver,
        capture::{CaptureSettings, ScreenCapture},
        conditional_loot::ConditionalLootController,
        config::BotConfig,
        controller::InputGate,
        emergency_recovery::EmergencyResourceRecovery,
        error::ControlError,
        field_control::FieldInputSession,
        field_preparation::AutomaticBuffDriver,
        survival::CharacterSurvivalPolicy,
        survival_vision::{HudObserverOptions, SurvivalHudObserver},
        time::MonotonicClock,
        workers::{Cancellation, CooperativeStop},
    },
    platform::{
        CaptureBackend, InputSystem,
        windows::{HotkeyListener, Windows},
    },
    runs::{
        RunError,
        summoner::{
            camera_ground::{CalibratedCameraGroundObserver, CameraGroundHandle, CameraLimits},
            canyon_finish::NativeCanyonVision,
            continuous_navigation::NativeContinuousVision,
            encounter_execution::NativeEncounterVision,
            legacy_segment::NativeLegacyVision,
            navigation::ArcanePathObserver,
            navigation_execution::load_arcane_step_planner,
            objective_vision::ArcaneObjectiveObserver,
            options::{AutomapPreference, SelectedRun, SummonerOptions},
            scene_vision::{
                ArcaneHazardHandle, ArcaneHazardObserver, ArcaneHoverObserver, ArcaneHubObserver,
                ArcaneTargetObserver,
            },
            terminal_vision::ArcaneTerminalObserver,
            terrain_patterns::ArcaneTerrainPatterns,
        },
    },
};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

/// 시계와 전체 중단은 GUI/CLI 세션이 소유하고, 각 방은 별도의 local 중단을 가진다.
#[derive(Clone)]
pub struct RuntimeContext {
    pub root: PathBuf,
    pub config: BotConfig,
    pub clock: Arc<dyn MonotonicClock>,
    pub stop: Option<Cancellation>,
    pub emit: FieldEmitter,
    pub entry_directory: Option<PathBuf>,
}
struct FieldState<S: CaptureBackend + InputSystem> {
    core: Option<FieldCore<S>>,
    recovery: Option<EmergencyResourceRecovery>,
    scanner: Option<LootScanner>,
    loot: Option<ConditionalLootController>,
    camera: CameraGroundHandle,
    hazards: ArcaneHazardHandle,
    map: Option<AutomapObserver>,
    probe: Option<TargetProbe>,
    policy: Arc<CharacterSurvivalPolicy>,
    deadline: Option<f64>,
}
struct LiveObservers {
    terrain: Arc<ArcaneTerrainPatterns>,
    terminal: Option<ArcaneTerminalObserver>,
    objective: Arc<ArcaneObjectiveObserver>,
}
fn missing(name: &str) -> RunError {
    RunError::Runtime(format!("{name} unavailable"))
}
impl<S: CaptureBackend + InputSystem> FieldState<S> {
    fn core(&self) -> Result<&FieldCore<S>, RunError> {
        self.core.as_ref().ok_or_else(|| missing("field capture"))
    }
    fn core_mut(&mut self) -> Result<&mut FieldCore<S>, RunError> {
        self.core.as_mut().ok_or_else(|| missing("field capture"))
    }
    fn world(&mut self, context: LootContext) -> Result<FieldWorld<'_, S>, RunError> {
        Ok(FieldWorld {
            loot: FieldLoot {
                core: self.core.as_mut().ok_or_else(|| missing("field capture"))?,
                scanner: self
                    .scanner
                    .as_mut()
                    .ok_or_else(|| missing("loot scanner"))?,
                recovery: self
                    .recovery
                    .as_mut()
                    .ok_or_else(|| missing("resource recovery"))?,
                camera: self.camera.clone(),
                policy: self.policy.clone(),
                deadline: self.deadline,
            },
            controller: self
                .loot
                .as_mut()
                .ok_or_else(|| missing("loot controller"))?,
            hazards: self.hazards.clone(),
            map: self.map.as_mut(),
            probe: self.probe.as_mut(),
            context,
        })
    }
}
pub struct WindowsSummonerRuntime {
    context: RuntimeContext,
    settings: CaptureSettings,
    stop: CooperativeStop,
    gate: InputGate,
    field: FieldState<Windows>,
    pending_capture: Option<ScreenCapture<Windows>>,
    hud: Option<SurvivalHudObserver>,
    panels: PanelObservers,
    path: ArcanePathObserver,
    hover: Arc<ArcaneHoverObserver>,
    target: Option<Arc<ArcaneTargetObserver>>,
    hub: Option<ArcaneHubObserver>,
    live: Option<LiveObservers>,
    entry: ActionSession,
    hotkeys: Option<HotkeyListener>,
    backend_ready: bool,
    camera_started: bool,
    initialized: bool,
    recipe: BuffRecipe,
    area: Option<AutomapObserver>,
    driver: AutomaticBuffDriver,
    retained_inputs: Vec<FieldInputSession<RecordedInput<Windows>>>,
    continuous: Option<NativeContinuousVision>,
    legacy: Option<NativeLegacyVision>,
    encounter_vision: Option<NativeEncounterVision>,
    canyon: Option<NativeCanyonVision>,
    last_encounter: Value,
    room: String,
    navigation_reaction_radius: [f64; 2],
    evidence_stream: Option<EvidenceStream>,
}
impl WindowsSummonerRuntime {
    /// 자산과 설정만 읽는다. dry 실행에서는 Windows 캡처/포커스/단축키를 만들지 않는다.
    pub fn load(
        context: RuntimeContext,
        options: &SummonerOptions,
        selected: &SelectedRun,
    ) -> Result<Self, RunError> {
        let root = &context.root;
        let navigation_reaction_radius =
            crate::runs::summoner::options::navigation_reaction_radius(&context.config)?;
        let (ground, path) = load_arcane_step_planner(
            root,
            options.automap == AutomapPreference::On,
            &selected.character_class,
        )?;
        let hud = SurvivalHudObserver::load(
            root.join("assets/ui/survival"),
            HudObserverOptions {
                allow_high_orb_fallback: true,
                allow_partial_orb_fallback: true,
                observe_mercenary: selected.character_key == "flash" && !options.resume_arcane,
            },
        )?;
        let map = (options.automap == AutomapPreference::On)
            .then(|| AutomapObserver::load(root.join("assets/ui/automap")))
            .transpose()?;
        let panels = PanelObservers::load(root)?;
        let panel_callback = panels.clone();
        let camera = CameraGroundHandle::new(CalibratedCameraGroundObserver::new(
            ground,
            move |frame| panel_callback.check(frame),
            CameraLimits {
                persist_reviewed_layout: true,
                ..Default::default()
            },
        )?);
        let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
        let hazards =
            ArcaneHazardHandle::new(ArcaneHazardObserver::load(owner.join("manifest.json"))?);
        let hover = Arc::new(ArcaneHoverObserver::load(root.join("assets"))?);
        let target = options
            .continuous
            .then(|| ArcaneTargetObserver::load(&owner).map(Arc::new))
            .transpose()?;
        let hub = options
            .continuous
            .then(|| ArcaneHubObserver::load(owner.join("hub-patterns.json")))
            .transpose()?;
        let settings = CaptureSettings::from_config(&context.config)?;
        let recipe = BuffRecipe::from_config(&context.config, selected, options.engage_monsters)?;
        let room = format!(
            "{}/{}",
            options
                .output
                .parent()
                .and_then(|p| p.file_name())
                .unwrap_or_default()
                .to_string_lossy(),
            options
                .output
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        let stop = CooperativeStop::new(context.stop.clone());
        Ok(Self {
            context,
            settings,
            stop,
            gate: InputGate::default(),
            field: FieldState {
                core: None,
                recovery: None,
                scanner: None,
                loot: None,
                camera,
                hazards,
                map,
                probe: None,
                policy: selected.policy.clone(),
                deadline: None,
            },
            pending_capture: None,
            hud: Some(hud),
            panels,
            path,
            hover,
            target,
            hub,
            live: None,
            entry: ActionSession::new("summoner-entry"),
            hotkeys: None,
            backend_ready: false,
            camera_started: false,
            initialized: false,
            recipe,
            area: None,
            driver: AutomaticBuffDriver::default(),
            retained_inputs: vec![],
            continuous: None,
            legacy: None,
            encounter_vision: None,
            canyon: None,
            last_encounter: Value::Null,
            room,
            navigation_reaction_radius,
            evidence_stream: None,
        })
    }
    /// 출력을 만든 뒤 입장하기 전에 연속 경로 자산을 확인한다. 작업 pool은 이동 시점에 만든다.
    pub fn prepare_live(&mut self) -> Result<(), RunError> {
        if self.target.is_some() && self.live.is_none() {
            let owner = self
                .context
                .root
                .join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
            self.live = Some(LiveObservers {
                terrain: Arc::new(ArcaneTerrainPatterns::load(
                    owner.join("navigation-patterns.json"),
                )?),
                terminal: Some(ArcaneTerminalObserver::load(
                    owner.join("terminal-patterns.json"),
                )?),
                objective: Arc::new(ArcaneObjectiveObserver::load(owner)?),
            });
        }
        Ok(())
    }
    fn check_inputs(&self) -> Result<(), RunError> {
        if self.retained_inputs.is_empty() {
            Ok(())
        } else {
            Err(missing("previous field input session cleanup"))
        }
    }
    fn finish_inputs<T>(
        &mut self,
        mut inputs: FieldInputSession<RecordedInput<Windows>>,
        result: Result<T, RunError>,
    ) -> Result<T, RunError> {
        let cleanup = inputs
            .finish(std::time::Duration::from_secs(1))
            .map_err(RunError::from);
        if cleanup.is_err() {
            self.retained_inputs.push(inputs);
        }
        combine(result, cleanup)
    }
}
fn combine<T>(result: Result<T, RunError>, cleanup: Result<(), RunError>) -> Result<T, RunError> {
    match (result, cleanup) {
        (Ok(v), Ok(())) => Ok(v),
        (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(c)) => Err(e.with_cleanup(c)),
    }
}
