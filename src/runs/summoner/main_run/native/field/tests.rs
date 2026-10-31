use super::*;
use crate::{
    common::{
        capture::CaptureSettings,
        controller::{ControllerFieldInput, InputGate},
        frame::PixelRect,
        frame_geometry::PhysicalTarget,
        survival_vision::HudObserverOptions,
    },
    platform::{InputEvent, WindowId, WindowInfo},
    runs::summoner::scene_vision::{ArcaneHazardHandle, ArcaneHazardObserver},
};
use std::{
    error::Error,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
type TestResult = Result<(), Box<dyn Error>>;
struct FixedClock;
impl MonotonicClock for FixedClock {
    fn now(&self) -> f64 {
        10.0
    }
}
struct AdvancingClock(std::sync::atomic::AtomicU64);
impl MonotonicClock for AdvancingClock {
    fn now(&self) -> f64 {
        self.0.fetch_add(10, Ordering::SeqCst) as f64 / 1000.0
    }
}
#[derive(Clone)]
struct Fake {
    window: WindowInfo,
    closed: Arc<AtomicBool>,
    source: Option<Arc<BgrFrame>>,
    reject_send: Arc<AtomicBool>,
    sent: Arc<Mutex<Vec<InputEvent>>>,
}
impl CaptureBackend for Fake {
    fn windows(&self) -> Result<Vec<WindowInfo>, ControlError> {
        Ok(vec![self.window.clone()])
    }
    fn monitors(&self) -> Result<Vec<PhysicalTarget>, ControlError> {
        Ok(vec![self.window.target])
    }
    fn client_box(&self, _: &WindowInfo) -> Result<PixelRect, ControlError> {
        Ok(PixelRect::full(self.window.target.size()))
    }
    fn capture_window(&mut self, _: &WindowInfo) -> Result<Option<BgrFrame>, ControlError> {
        if let Some(source) = &self.source {
            return Ok(Some(source.as_ref().clone()));
        }
        Ok(Some(
            BgrFrame::zeros(self.window.target.size())
                .map_err(|e| ControlError::Runtime(e.to_string()))?,
        ))
    }
    fn capture_screen(&mut self, _: PhysicalTarget) -> Result<BgrFrame, ControlError> {
        Err(ControlError::Runtime("unexpected screen capture".into()))
    }
    fn close(&mut self) -> Result<(), ControlError> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}
impl InputSystem for Fake {
    fn focused(&self, id: WindowId) -> Result<bool, ControlError> {
        Ok(id == self.window.id)
    }
    fn bounds(&self, _: WindowId) -> Result<PhysicalTarget, ControlError> {
        Ok(self.window.target)
    }
    fn desktop(&self) -> Result<PhysicalTarget, ControlError> {
        Ok(self.window.target)
    }
    fn pointer(&self) -> Result<[i32; 2], ControlError> {
        Ok([100, 100])
    }
    fn numlock_state(&self) -> Result<bool, ControlError> {
        Ok(false)
    }
    fn arrow_scan(&self, _: u32) -> Result<u16, ControlError> {
        Ok(0)
    }
    fn send(&self, event: InputEvent) -> Result<u32, ControlError> {
        self.sent
            .lock()
            .map_err(|_| ControlError::Runtime("test events poisoned".into()))?
            .push(event);
        if self.reject_send.load(Ordering::SeqCst) {
            Err(ControlError::Runtime("unexpected native dispatch".into()))
        } else {
            Ok(1)
        }
    }
}
type Fixture = (FieldCore<Fake>, Arc<AtomicBool>);
fn core_fixture(source: Option<Arc<BgrFrame>>) -> Result<Fixture, Box<dyn Error>> {
    core_fixture_with_clock(source, Arc::new(FixedClock))
}
fn core_fixture_with_clock(
    source: Option<Arc<BgrFrame>>,
    clock: Arc<dyn MonotonicClock>,
) -> Result<Fixture, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let closed = Arc::new(AtomicBool::new(false));
    let system = Fake {
        window: WindowInfo {
            id: WindowId::new(7)?,
            title: "Diablo".into(),
            target: PhysicalTarget::new(10, 20, 1922, 1140)?,
        },
        closed: closed.clone(),
        source,
        reject_send: Arc::new(AtomicBool::new(true)),
        sent: Arc::new(Mutex::new(vec![])),
    };
    let settings = CaptureSettings {
        window_title: Some("Diablo".into()),
        ..Default::default()
    };
    let capture = ScreenCapture::new(system.clone(), settings.clone(), root)?;
    let window = Arc::new(WindowGuard::new(
        Arc::new(system.clone()),
        settings,
        system.window.clone(),
    ));
    let input = Arc::new(RecordedInput::new(
        ControllerFieldInput::new(
            Arc::new(system),
            InputGate::default(),
            window.window().id,
            clock.clone(),
        ),
        clock.clone(),
        Arc::new(FieldCore::<Fake>::wall_now),
    ));
    let panels = PanelObservers::load(root)?;
    let core = FieldCore::new(
        capture,
        FieldServices {
            hud: SurvivalHudObserver::load(
                root.join("assets/ui/survival"),
                HudObserverOptions {
                    allow_high_orb_fallback: true,
                    allow_partial_orb_fallback: true,
                    observe_mercenary: false,
                },
            )?,
            guards: FieldGuards {
                clock,
                stop: CooperativeStop::default(),
                window,
            },
            input: input.clone(),
            panels: panels.clone(),
            emit: Arc::new(|_| ()),
        },
    );
    Ok((core, closed))
}
#[test]
fn shared_current_frame_panels_and_hud_release_pixels_before_capture_close() -> TestResult {
    let (mut core, closed) = core_fixture(None)?;
    let input = core.input();
    let panels = core.panels();
    let frame = core.grab()?.frame;
    let weak = Arc::downgrade(&frame);
    assert_eq!(core.to_desktop([2.5, 3.5])?, [12, 24]);
    let hud = core.read_hud(&frame)?;
    assert!(hud.life.ratio().is_none());
    assert!(hud.mana.ratio().is_none());
    assert!(!panels.check(&frame)?);
    let failed: Result<(), RunError> = panels.with_town(|town| {
        town.set_frame(frame.clone());
        Err(RunError::Runtime("test town failure".into()))
    });
    assert!(failed.is_err());
    drop(frame);
    assert!(weak.upgrade().is_none());
    assert_eq!(input.events()?, serde_json::json!([]));
    core.close_capture()?;
    input.close()?;
    assert!(closed.load(Ordering::SeqCst));
    Ok(())
}
#[test]
fn native_runtime_safety_and_paused_renewal_share_fake_effects_and_current_observers() -> TestResult
{
    use super::super::{FieldLoot, FieldWorld, LootContext, observe_runtime};
    use crate::{
        actions::loot_pickup::LootScanner,
        common::{
            conditional_loot::{ConditionalLootController, LootLimits},
            config::BotConfig,
            emergency_recovery::EmergencyResourceRecovery,
            survival::CharacterSurvivalPolicy,
            vision_ops::codec,
        },
        runs::summoner::camera_ground::{
            CalibratedCameraGroundObserver, CameraGroundHandle, CameraLimits, CameraOnlyGround,
            ReviewedCameraLayout,
        },
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let owner = root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140");
    let mut source =
        codec::load_color(owner.join("references/navigation/area-title-stone-underlay.png"))?;
    let band = codec::load_color(root.join("assets/ui/survival/references/closed.png"))?;
    for y in 0..band.size().height() {
        let from = y * band.size().width() * 3;
        let to = ((y + 880) * 1922 + 250) * 3;
        source.pixels_mut()[to..to + band.size().width() * 3]
            .copy_from_slice(&band.pixels()[from..from + band.size().width() * 3]);
    }
    let clock = Arc::new(AdvancingClock(std::sync::atomic::AtomicU64::new(10000)));
    let (mut core, _) = core_fixture_with_clock(Some(Arc::new(source)), clock)?;
    let system = core.guards().window.system();
    let panels = core.panels();
    let camera = CameraGroundHandle::new(CalibratedCameraGroundObserver::new(
        Box::new(CameraOnlyGround),
        move |f| panels.check(f),
        CameraLimits {
            persist_reviewed_layout: true,
            ..Default::default()
        },
    )?);
    let acquired = PreparationEnvironment::clock(&core);
    let frame = core.grab()?.frame;
    let weak = Arc::downgrade(&frame);
    let hud = core.read_hud(&frame)?;
    let layout = ReviewedCameraLayout::load(
        root.join("assets/ui/layouts/remastered-1922x1140/camera-ground.json"),
    )?;
    camera.with(|c| {
        c.confirm_runtime_viewport(
            &frame,
            &hud,
            true,
            acquired,
            PreparationEnvironment::clock(&core),
            &layout,
        )
    })?;
    let config = BotConfig::load(root.join("config"))?;
    let profile = config
        .section("characters")
        .and_then(|v| v.get("flash"))
        .ok_or("flash")?;
    let policy = Arc::new(CharacterSurvivalPolicy::from_value(&profile["survival"])?);
    let mut scanner = LootScanner::new(&config, root)?;
    let mut recovery = EmergencyResourceRecovery::default();
    let mut controller = ConditionalLootController::new(
        policy.clone(),
        "f2".into(),
        [],
        LootLimits::new(3.0, 12, 2, 0.32)?,
    );
    let mut loot = FieldLoot {
        core: &mut core,
        scanner: &mut scanner,
        recovery: &mut recovery,
        camera,
        policy,
        deadline: None,
    };
    controller.prepare(&mut loot, false, Some(false))?;
    let hazards = ArcaneHazardHandle::new(ArcaneHazardObserver::load(owner.join("manifest.json"))?);
    let mut map =
        crate::common::automap_vision::AutomapObserver::load(root.join("assets/ui/automap"))?;
    let mut area =
        crate::common::automap_vision::AutomapObserver::load(root.join("assets/ui/automap"))?;
    let mut world = FieldWorld {
        loot,
        controller: &mut controller,
        hazards: hazards.clone(),
        map: Some(&mut map),
        probe: None,
        context: LootContext::Navigation,
    };
    let mut evidence = vec![];
    let proof = observe_runtime(&mut world, &mut area, "test-character", &mut evidence)?;
    assert_eq!(proof["source"], "runtime_screen");
    assert_eq!(proof["character_id"], "test-character");
    assert_eq!(proof["life_ratio"], 1.0);
    assert_eq!(proof["mana_ratio"], 1.0);
    assert_eq!(proof["nearby_candidates"], 0);
    assert_eq!(proof["item_labels_on"], false);
    assert!(evidence.is_empty());
    assert_eq!(hazards.sequence()?, 1);
    assert!(world.controller.movement_allowed());
    drop(frame);
    assert!(weak.upgrade().is_none());
    assert_eq!(world.loot.core.input().events()?, serde_json::json!([]));
    system.reject_send.store(false, Ordering::SeqCst);
    let order: Vec<String> =
        serde_json::from_value(profile["actions"]["pre_run_buff_order"].clone())?;
    let recipe = super::super::BuffRecipe {
        order: order.clone(),
        pauses: crate::common::buffing::PauseSchedule::new(Default::default(), 0.0)?,
        movement_key: profile["actions"]["movement_skill_key"]
            .as_str()
            .ok_or("movement key")?
            .into(),
        attack_key: profile["actions"]["primary_attack_skill_key"]
            .as_str()
            .ok_or("attack key")?
            .into(),
        reference_seconds: 0.32,
        character_key: "test-character".into(),
        engage_monsters: true,
    };
    let mut driver = crate::common::field_preparation::AutomaticBuffDriver::default();
    let mut retained = vec![];
    let mut paused = super::super::PausedField {
        world,
        driver: &mut driver,
        area: &mut area,
        recipe: &recipe,
        retained_inputs: &mut retained,
        encounter_recheck: None,
    };
    let mut report = serde_json::json!({"buff_renewals":[]});
    let deadline = crate::runs::summoner::field_defense::PausedEnvironment::renew_buffs(
        &mut paused,
        &mut report,
        &mut evidence,
    )?;
    assert_eq!(paused.world.loot.deadline, Some(deadline));
    assert_eq!(
        report["buff_renewals"].as_array().ok_or("renewals")?.len(),
        1
    );
    assert_eq!(
        report["buff_renewals"][0]["buff"]["status"],
        "dispatched_unconfirmed"
    );
    assert_eq!(
        report["buff_renewals"][0]["effect_status"],
        "not_independently_verified"
    );
    assert_eq!(evidence.len(), order.len());
    drop(paused);
    assert!(retained.is_empty());
    let rows = core.input().events()?;
    let rows = rows.as_array().ok_or("input events")?;
    assert_eq!(rows.len(), order.len());
    for (row, key) in rows.iter().zip(order) {
        assert_eq!(row["action"], "press");
        assert_eq!(row["args"], serde_json::json!([key]));
        assert_eq!(row["sent"], true);
    }
    core.input().close()?;
    core.close_capture()?;
    Ok(())
}
#[test]
fn shared_hazard_sequence_is_one_owner_across_field_consumers() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let observer = ArcaneHazardHandle::new(ArcaneHazardObserver::load(
        root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/manifest.json"),
    )?);
    let another = observer.clone();
    let frame = Arc::new(BgrFrame::zeros(crate::common::frame::Size::new(
        1922, 1140,
    )?)?);
    let weak = Arc::downgrade(&frame);
    assert!(
        observer
            .observe(frame.view(), [962, 578], 10.0, 10.0, None)?
            .is_empty()
    );
    assert!(
        another
            .observe(frame.view(), [962, 578], 10.1, 10.1, None)?
            .is_empty()
    );
    assert_eq!(observer.sequence()?, 2);
    assert_eq!(another.sequence()?, 2);
    drop(frame);
    assert!(weak.upgrade().is_none());
    Ok(())
}
#[cfg(windows)]
#[test]
fn entry_worker_factory_failure_is_failed_and_joined_without_capture_or_input() -> TestResult {
    use crate::{
        actions::{
            desktop::DesktopActionBackend,
            session::{ActionSession, SessionStatus},
            town_loop::Difficulty,
        },
        runs::summoner::main_run::entry::{ActionEntry, EntrySession, enter_arcane},
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut session = ActionSession::new("Run lifecycle");
    let mut entry = ActionEntry::<DesktopActionBackend<Fake, Fake>>::new(
        &mut session,
        crate::runs::summoner::town_vision(root)?,
        root.join("target/tmp/entry-factory-uncreated"),
        |_| {
            Err(ActionError::runtime(
                "forced factory failure before capture",
            ))
        },
    );
    let mut emitted = vec![];
    let result = enter_arcane(
        &mut entry,
        Difficulty::Hell,
        false,
        &CooperativeStop::default(),
        |s| emitted.push(s.to_owned()),
    );
    assert_eq!(
        result.err().ok_or("entry failure")?.to_string(),
        "Arcane entry did not pass: failed"
    );
    assert_eq!(
        entry.last_result()?,
        Some(crate::actions::run_lifecycle::TownOutcome::Failed)
    );
    drop(entry);
    assert_eq!(
        session.status()?,
        SessionStatus::Finished(Err("forced factory failure before capture".into()))
    );
    assert!(
        emitted
            .iter()
            .any(|e| e == "error: Run lifecycle failed: forced factory failure before capture")
    );
    assert!(!root.join("target/tmp/entry-factory-uncreated").exists());
    Ok(())
}
#[test]
fn borrowed_town_actions_preserve_field_capture_and_unrecorded_action_payloads() -> TestResult {
    use crate::{
        actions::runtime::{ActionBackend, ActionContext, PointerStyle},
        common::workers::Cancellation,
        runs::summoner::{
            main_run::native::BorrowedTownBackend,
            room_finish::{NativeTownFinish, TownFinishRuntime},
        },
    };
    let (mut core, closed) = core_fixture(None)?;
    let system = core.guards().window.system();
    system.reject_send.store(false, Ordering::SeqCst);
    let frame = core.grab()?.frame;
    let input = core.input();
    let stop = core.guards().stop;
    {
        let backend = BorrowedTownBackend::new(&mut core)?;
        let mut context =
            ActionContext::new(backend, Cancellation::default(), PointerStyle::Lifecycle);
        let mut vision = crate::runs::summoner::town_vision(env!("CARGO_MANIFEST_DIR"))?;
        let mut town = NativeTownFinish::new(
            &mut context,
            &mut vision,
            crate::actions::town_loop::Destination::Arcane,
        )?;
        town.begin()?;
        town.restore_map(&frame)?;
        town.release_mouse()?;
        drop(town);
        context.close()?;
    }
    assert!(!closed.load(Ordering::SeqCst));
    assert_eq!(input.events()?, serde_json::json!([]));
    let keys = system
        .sent
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|e| matches!(e, InputEvent::Keyboard { .. }))
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec![
            InputEvent::Keyboard {
                scan: 15,
                up: false,
                extended: false
            },
            InputEvent::Keyboard {
                scan: 15,
                up: true,
                extended: false
            }
        ]
    );
    {
        let mut backend = BorrowedTownBackend::new(&mut core)?;
        system.reject_send.store(true, Ordering::SeqCst);
        assert!(backend.click("left").is_err());
        system.reject_send.store(false, Ordering::SeqCst);
        stop.set();
        assert!(backend.key("f2", false).is_err());
        backend.key("f2", true)?;
    }
    input.close()?;
    assert!(
        system
            .sent
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .any(|e| matches!(e, InputEvent::Mouse { flags: 4, .. }))
    );
    assert_eq!(input.events()?, serde_json::json!([]));
    core.close_capture()?;
    assert!(closed.load(Ordering::SeqCst));
    Ok(())
}

#[test]
fn panel_result_tracks_only_the_same_owned_frame_without_retaining_pixels() -> TestResult {
    use crate::common::vision_ops::codec;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let panels = PanelObservers::load(root)?;
    let open = Arc::new(codec::load_color(root.join(
        "assets/ui/town-loop/remastered-ko-window/references/exit-menu-window.png",
    ))?);
    assert!(panels.check(&open)?);
    assert!(panels.check(&open.clone())?);
    assert_eq!(
        Arc::strong_count(&open),
        1,
        "cache must not retain full image pixels"
    );
    panels.0.with(|p| {
        assert!(
            p.last
                .as_ref()
                .is_some_and(|(f, _)| f.ptr_eq(&Arc::downgrade(&open)))
        );
        Ok(())
    })?;
    drop(open);
    panels.0.with(|p| {
        assert_eq!(p.last.as_ref().map(|(f, _)| f.strong_count()), Some(0));
        Ok(())
    })?;
    let closed=Arc::new(codec::load_color(root.join("assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-before.png"))?);
    assert!(
        !panels.check(&closed)?,
        "new capture must not reuse the open-panel result"
    );
    panels.with_town(|_| Ok(()))?;
    panels.0.with(|p| {
        assert!(p.last.is_none());
        Ok(())
    })?;
    assert!(!panels.check(&closed)?);
    Ok(())
}
