use d2bot::{
    actions::{
        ActionError, EventLevel,
        runtime::{ActionBackend, ActionContext, ActionEvents, FocusOutcome, PointerStyle},
    },
    common::{
        frame::{BgrFrame, Size},
        frame_geometry::PhysicalTarget,
        realtime::CapturedFrame,
        workers::Cancellation,
    },
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};
#[path = "support/directory.rs"]
mod directory;
mod support;

#[test]
fn current_town_finish_adapter_restores_map_without_a_new_planner_command()
-> Result<(), Box<dyn Error>> {
    use d2bot::{
        actions::town_loop::{Destination, TownVision},
        runs::summoner::room_finish::{NativeTownFinish, TownFinishRuntime},
    };
    for fail in [false, true] {
        let (state, mut context) = fixture();
        context.accept_pointer([900, 800]);
        lock(&state).guard_failed = fail;
        let mut vision = TownVision::load(env!("CARGO_MANIFEST_DIR"))?;
        let mut runtime = NativeTownFinish::new(&mut context, &mut vision, Destination::Arcane)?;
        runtime.begin()?;
        let before = runtime.snapshot()?;
        let frame = Arc::new(BgrFrame::zeros(Size::new(1922, 1140)?)?);
        let result = runtime.restore_map(&frame);
        assert_eq!(result.is_err(), fail);
        assert_eq!(runtime.snapshot()?, before);
        runtime.release_mouse()?;
        drop(runtime);
        assert_eq!(context.last_pointer(), None);
        let trace = lock(&state).trace.clone();
        let keys = trace
            .iter()
            .filter(|e| e[0] == "key")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            if fail {
                vec![]
            } else {
                vec![json!(["key", "tab", false]), json!(["key", "tab", true])]
            }
        );
        assert!(
            !trace
                .iter()
                .any(|e| e[0] == "capture" || e[0] == "click" || e[0] == "bind" || e[0] == "close")
        );
        context.close()?;
    }
    Ok(())
}

#[test]
fn gem_success_and_uncertain_result_paths_match_source_effects_and_events()
-> Result<(), Box<dyn Error>> {
    use d2bot::{
        actions::{
            MatchResult,
            gem_summing::{self, FrameCounts, GemOutcome, GemRecognition},
        },
        common::frame::BgrView,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Script {
        scenario: &'static str,
        occupied: AtomicUsize,
    }
    impl GemRecognition for Script {
        fn locate_stash_panel(&self, _: BgrView<'_>) -> Result<Option<MatchResult>, ActionError> {
            Ok(Some(MatchResult {
                top_left: [20, 30],
                width: 576,
                height: 757,
                score: 1.0,
            }))
        }
        fn locate_cube_button(&self, _: BgrView<'_>) -> Result<Option<MatchResult>, ActionError> {
            Ok(Some(MatchResult {
                top_left: [500, 600],
                width: 40,
                height: 20,
                score: 1.0,
            }))
        }
        fn read_frame_counts(
            &self,
            _: BgrView<'_>,
            _: MatchResult,
        ) -> Result<FrameCounts, ActionError> {
            let mut counts = [[0; 7]; 5];
            counts[0][0] = 10;
            Ok(FrameCounts {
                counts,
                confidences: [[1.0; 7]; 5],
                icon_scores: [[1.0; 7]; 5],
            })
        }
        fn occupied_cube_cells(
            &self,
            _: BgrView<'_>,
            _: MatchResult,
        ) -> Result<Vec<([usize; 2], f64)>, ActionError> {
            let call = self.occupied.fetch_add(1, Ordering::Relaxed);
            if call != 1 {
                return Ok(Vec::new());
            }
            Ok(if self.scenario == "bad_cells" {
                vec![([259, 448], 1.0), ([308, 448], 1.0)]
            } else {
                vec![([259, 448], 1.0)]
            })
        }
        fn cube_result_icon(
            &self,
            _: BgrView<'_>,
            _: MatchResult,
            _: [i64; 2],
        ) -> Result<BgrFrame, ActionError> {
            Ok(BgrFrame::zeros(Size::new(1, 1)?)?)
        }
        fn expected_result(&self, _: usize, _: usize, _: BgrView<'_>) -> Result<bool, ActionError> {
            Ok(self.scenario != "bad_icon")
        }
    }
    for scenario in ["success", "bad_cells", "bad_icon"] {
        let expected = support::legacy_expectation(
            "action-oracle",
            &json!({"kind":"gem_session","scenario":scenario}),
        )?;
        let (state, mut ctx) = fixture();
        lock(&state).target = Some(PhysicalTarget::new(100, 200, 700, 900)?);
        let script = Script {
            scenario,
            occupied: AtomicUsize::new(0),
        };
        assert_eq!(
            gem_summing::run_session(&script, &mut ctx)?,
            GemOutcome::Completed
        );
        assert_eq!(
            json!(ctx.take_events()),
            expected["events"],
            "{scenario}/events"
        );
        let state = lock(&state);
        let trace = state
            .trace
            .iter()
            .filter(|e| e[0] != "close")
            .cloned()
            .collect::<Vec<_>>();
        compare(&json!(trace), &expected["trace"])?;
        assert!(
            (state.now - expected["elapsed"].as_f64().ok_or("elapsed")?).abs() < 1e-12,
            "{scenario}/elapsed"
        );
        assert_eq!(state.trace.last(), Some(&json!(["close"])));
    }
    Ok(())
}
#[derive(Default)]
struct State {
    trace: Vec<Value>,
    pointer: [i64; 2],
    now: f64,
    fail_down: Option<String>,
    fail_up: usize,
    fail_close: usize,
    panic_unbind: bool,
    frames: VecDeque<Arc<BgrFrame>>,
    target: Option<PhysicalTarget>,
    pointer_stalled: bool,
    guard_failed: bool,
    cancel_on_wait: Option<Cancellation>,
    capture_failed: bool,
    block_result_path: Option<std::path::PathBuf>,
}
struct FakeBackend(Arc<Mutex<State>>);
fn lock(state: &Arc<Mutex<State>>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|p| p.into_inner())
}
impl ActionBackend for FakeBackend {
    fn start_town_guard(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
    fn town_geometry(&self) -> Result<(usize, PhysicalTarget), ActionError> {
        Ok((7, self.target()))
    }
    fn town_guard(&self) -> Result<(), ActionError> {
        if lock(&self.0).guard_failed {
            Err(ActionError::runtime("focus lost"))
        } else {
            Ok(())
        }
    }
    fn wait(&mut self, duration: Duration) -> Result<(), ActionError> {
        self.sleep(duration)?;
        if let Some(token) = &lock(&self.0).cancel_on_wait {
            token.cancel();
        }
        Ok(())
    }
    fn monotonic_now(&self) -> f64 {
        self.wall_now()
    }
    fn capture(&mut self) -> Result<CapturedFrame, ActionError> {
        let frame = {
            let mut state = lock(&self.0);
            state.trace.push(json!(["capture"]));
            if state.capture_failed {
                return Err(ActionError::runtime("capture failed"));
            }
            if let Some(path) = state.block_result_path.take() {
                std::fs::create_dir(path)?;
            }
            state.frames.pop_front()
        };
        Ok(CapturedFrame {
            frame: if let Some(frame) = frame {
                frame
            } else {
                Arc::new(BgrFrame::zeros(self.target().size())?)
            },
            target: self.target(),
            wall_captured_at: None,
            geometry: None,
        })
    }
    fn target(&self) -> PhysicalTarget {
        lock(&self.0).target.unwrap_or_else(|| {
            PhysicalTarget::new(100, 200, 640, 480).unwrap_or_else(|_| unreachable!())
        })
    }
    fn geometry_is_current(&self) -> Result<bool, ActionError> {
        Ok(true)
    }
    fn frame_to_desktop(&self, p: [f64; 2]) -> Result<[f64; 2], ActionError> {
        Ok([p[0] + 100.0, p[1] + 200.0])
    }
    fn desktop_to_frame(&self, p: [f64; 2]) -> Result<[i64; 2], ActionError> {
        Ok([
            (p[0] - 100.0).round_ties_even() as i64,
            (p[1] - 200.0).round_ties_even() as i64,
        ])
    }
    fn focus_configured(&mut self) -> Result<FocusOutcome, ActionError> {
        Ok(FocusOutcome::Unavailable)
    }
    fn pointer(&self) -> Result<[i64; 2], ActionError> {
        Ok(lock(&self.0).pointer)
    }
    fn move_desktop(&mut self, p: [i64; 2]) -> Result<(), ActionError> {
        let mut state = lock(&self.0);
        if !state.pointer_stalled {
            state.pointer = p;
        }
        state.trace.push(json!(["move", p[0], p[1]]));
        Ok(())
    }
    fn key(&mut self, key: &str, up: bool) -> Result<(), ActionError> {
        let mut state = lock(&self.0);
        state.trace.push(json!(["key", key, up]));
        if !up && state.fail_down.as_deref() == Some(key) {
            return Err(ActionError::runtime("partial down"));
        }
        if up && key == "shift" && state.fail_up > 0 {
            state.fail_up -= 1;
            return Err(ActionError::runtime("release failed"));
        }
        Ok(())
    }
    fn click(&mut self, button: &str) -> Result<(), ActionError> {
        lock(&self.0).trace.push(json!(["click", button]));
        Ok(())
    }
    fn mouse_button(&mut self, button: &str, up: bool) -> Result<(), ActionError> {
        lock(&self.0).trace.push(json!(["mouse", button, up]));
        Ok(())
    }
    fn wall_now(&self) -> f64 {
        lock(&self.0).now
    }
    fn sleep(&mut self, d: Duration) -> Result<(), ActionError> {
        let mut state = lock(&self.0);
        state.now += d.as_secs_f64();
        if !d.is_zero() {
            state.trace.push(json!(["sleep", d.as_secs_f64()]));
        }
        Ok(())
    }
    fn random_integer(&mut self, low: i64, _high: i64) -> Result<i64, ActionError> {
        Ok(if low < 0 { 0 } else { low })
    }
    fn random_float(&mut self, low: f64, _high: f64) -> Result<f64, ActionError> {
        Ok(low)
    }
    fn bind_stop(&mut self, _: Cancellation) -> Result<(), ActionError> {
        lock(&self.0).trace.push(json!(["bind"]));
        Ok(())
    }
    fn unbind_stop(&mut self) -> Result<(), ActionError> {
        let mut state = lock(&self.0);
        state.trace.push(json!(["unbind"]));
        if state.panic_unbind {
            state.panic_unbind = false;
            drop(state);
            panic!("fake unbind panic");
        }
        Ok(())
    }
    fn close_capture(&mut self) -> Result<(), ActionError> {
        let mut state = lock(&self.0);
        state.trace.push(json!(["close"]));
        if state.fail_close > 0 {
            state.fail_close -= 1;
            Err(ActionError::runtime("close failed"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn town_dispatch_releases_after_stop_refuses_hover_race_and_never_retries_pointer()
-> Result<(), Box<dyn Error>> {
    use d2bot::actions::{
        run_lifecycle::dispatch_town_action,
        town_loop::{TownAction, TownActionKind, TownPhase},
    };
    let action = |kind, point| TownAction {
        kind,
        reason: "fixture".into(),
        phase: TownPhase::Act1Waypoint,
        number: 1,
        key: None,
        point,
    };
    let frame = Size::new(1267, 753)?;
    let (state, mut ctx) = fixture();
    dispatch_town_action(
        &mut ctx,
        frame,
        &action(TownActionKind::Hold, Some([300, 200])),
    )?;
    ctx.cancellation().cancel();
    lock(&state).guard_failed = true;
    dispatch_town_action(&mut ctx, frame, &action(TownActionKind::Release, None))?;
    assert_eq!(
        lock(&state).trace,
        [
            json!(["move", 400, 400]),
            json!(["mouse", "left", false]),
            json!(["mouse", "left", true])
        ]
    );
    let (state, mut ctx) = fixture();
    lock(&state).cancel_on_wait = Some(ctx.cancellation());
    assert!(
        dispatch_town_action(
            &mut ctx,
            frame,
            &action(TownActionKind::Click, Some([300, 200]))
        )
        .is_err()
    );
    assert!(!lock(&state).trace.iter().any(|e| e[0] == "click"));
    let (state, mut ctx) = fixture();
    lock(&state).pointer_stalled = true;
    let error = dispatch_town_action(
        &mut ctx,
        frame,
        &action(TownActionKind::Click, Some([300, 200])),
    )
    .err()
    .ok_or("pointer timeout")?;
    assert!(error.to_string().contains("Pointer mapping mismatch"));
    assert_eq!(
        lock(&state).trace.iter().filter(|e| e[0] == "move").count(),
        1
    );
    assert!(lock(&state).now >= 0.08);
    assert!(!lock(&state).trace.iter().any(|e| e[0] == "click"));
    Ok(())
}
fn fixture() -> (Arc<Mutex<State>>, ActionContext<FakeBackend>) {
    let state = Arc::new(Mutex::new(State {
        pointer: [150, 250],
        ..State::default()
    }));
    let ctx = ActionContext::new(
        FakeBackend(state.clone()),
        Cancellation::default(),
        PointerStyle::Gems,
    );
    (state, ctx)
}
fn compare(a: &Value, b: &Value) -> Result<(), Box<dyn Error>> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert!(
                (a.as_f64().ok_or("number")? - b.as_f64().ok_or("number")?).abs() < 1e-12,
                "{a} != {b}"
            )
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{a:?} != {b:?}");
            for (a, b) in a.iter().zip(b) {
                compare(a, b)?;
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in a {
                compare(value, b.get(key).ok_or("missing key")?)?;
            }
        }
        _ => assert_eq!(a, b),
    }
    Ok(())
}

#[test]
fn town_stop_and_capture_failure_preserve_evidence_and_cleanup() -> Result<(), Box<dyn Error>> {
    use d2bot::actions::{
        run_lifecycle::{TownOutcome, run_town},
        town_loop::{TownOptions, TownVision},
    };
    use sha2::{Digest, Sha256};
    for scenario in ["stopped", "capture_failure"] {
        let owned = directory::TestDirectory::new()?;
        let python_directory = owned.path().join("python/20000101-000000-town-loop");
        let rust_directory = owned.path().join("rust/20000101-000000-town-loop");
        let expected = support::legacy_expectation(
            "action-oracle",
            &json!({"kind":"town_evidence","directory":python_directory,"scenario":scenario}),
        )?;
        let state = Arc::new(Mutex::new(State {
            pointer: [150, 250],
            ..State::default()
        }));
        lock(&state).target = Some(PhysicalTarget::new(100, 200, 1922, 1140)?);
        lock(&state).capture_failed = scenario == "capture_failure";
        let mut context = ActionContext::new(
            FakeBackend(state.clone()),
            Cancellation::default(),
            PointerStyle::Lifecycle,
        );
        lock(&state).cancel_on_wait = Some(context.cancellation());
        let mut vision = TownVision::load(env!("CARGO_MANIFEST_DIR"))?;
        let result = run_town(
            &mut context,
            &mut vision,
            TownOptions::default(),
            false,
            &rust_directory,
        );
        if scenario == "stopped" {
            assert_eq!(result?.result, TownOutcome::Stopped);
        } else {
            assert!(result.is_err());
        }
        compare(&json!(context.take_events()), &expected["events"])?;
        compare(&json!(lock(&state).trace), &expected["trace"])?;
        let mut artifacts = serde_json::Map::new();
        for entry in std::fs::read_dir(&rust_directory)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("filename")?
                .to_owned();
            let value = match path.extension().and_then(|s| s.to_str()) {
                Some("json") => serde_json::from_slice(&std::fs::read(&path)?)?,
                Some("jsonl") => json!(
                    std::fs::read_to_string(&path)?
                        .lines()
                        .map(serde_json::from_str::<Value>)
                        .collect::<Result<Vec<_>, _>>()?
                ),
                Some("png") => json!(format!(
                    "{:x}",
                    Sha256::digest(d2bot::common::vision_ops::codec::load_color(&path)?.pixels())
                )),
                _ => return Err("unexpected evidence file".into()),
            };
            artifacts.insert(name, value);
        }
        compare(&Value::Object(artifacts), &expected["artifacts"])?;
    }
    Ok(())
}

#[test]
fn town_result_write_failure_still_writes_timings_and_closes_owned_resources()
-> Result<(), Box<dyn Error>> {
    use d2bot::actions::{
        run_lifecycle::run_town,
        town_loop::{TownOptions, TownVision},
    };
    let owned = directory::TestDirectory::new()?;
    let output = owned.path().join("evidence");
    let state = Arc::new(Mutex::new(State {
        pointer: [150, 250],
        target: Some(PhysicalTarget::new(100, 200, 1922, 1140)?),
        block_result_path: Some(output.join("result.json")),
        ..State::default()
    }));
    let mut context = ActionContext::new(
        FakeBackend(state.clone()),
        Cancellation::default(),
        PointerStyle::Lifecycle,
    );
    lock(&state).cancel_on_wait = Some(context.cancellation());
    let mut vision = TownVision::load(env!("CARGO_MANIFEST_DIR"))?;
    let error = run_town(
        &mut context,
        &mut vision,
        TownOptions::default(),
        false,
        &output,
    )
    .err()
    .ok_or("write failure")?;
    assert!(error.to_string().contains("town finalization failed"));
    assert!(output.join("timings.json").is_file());
    assert!(output.join("stopped-frame.png").is_file());
    {
        let state = lock(&state);
        assert_eq!(
            &state.trace[state.trace.len() - 2..],
            &[json!(["unbind"]), json!(["close"])]
        );
    }
    assert!(
        context
            .take_events()
            .iter()
            .any(|e| e.level == EventLevel::Error)
    );
    Ok(())
}

#[test]
fn launched_gem_worker_connects_shared_events_and_closes_capture_on_failure()
-> Result<(), Box<dyn Error>> {
    use d2bot::actions::{
        gem_summing::GemVision,
        session::{ActionSession, SessionStatus},
    };
    let vision = Arc::new(GemVision::load(env!("CARGO_MANIFEST_DIR"))?);
    let state = Arc::new(Mutex::new(State {
        pointer: [150, 250],
        ..State::default()
    }));
    let backend_state = state.clone();
    let mut session = ActionSession::new("Gem summing");
    session.start_gems(vision.clone(), move |_| Ok(FakeBackend(backend_state)))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !matches!(session.status()?, SessionStatus::Finished(_)) {
        if std::time::Instant::now() > deadline {
            return Err("gem worker did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    session.stop(Duration::from_secs(2))?;
    assert!(
        matches!(session.status()?,SessionStatus::Finished(Err(error))if error.contains("opened gem stash panel"))
    );
    let events = session.events().drain();
    assert!(
        events
            .iter()
            .any(|e| e.message == "Gem summing started. Press F10 to stop.")
    );
    assert!(events.iter().any(|e| e.level == EventLevel::Error));
    assert_eq!(lock(&state).trace.last(), Some(&json!(["close"])));
    let mut failed = ActionSession::new("Gem summing");
    failed.start_gems(vision, |_| -> Result<FakeBackend, ActionError> {
        Err(ActionError::runtime("capture factory failed"))
    })?;
    failed.stop(Duration::from_secs(2))?;
    assert!(
        matches!(failed.status()?,SessionStatus::Finished(Err(error))if error=="capture factory failed")
    );
    assert!(
        failed
            .events()
            .drain()
            .iter()
            .any(|e| e.message == "Gem summing failed: capture factory failed")
    );
    Ok(())
}
#[test]
fn modified_click_moves_waits_and_pdi_default_pauses_match_installed_python()
-> Result<(), Box<dyn Error>> {
    let expected = support::legacy_expectation("action-oracle", &json!({"kind":"gem_pointer"}))?;
    let (state, mut ctx) = fixture();
    ctx.click_absolute([300.0, 400.0], "right", true)?;
    {
        let state = lock(&state);
        compare(&json!(state.trace), &expected["trace"])?;
        assert!((state.now - expected["elapsed"].as_f64().ok_or("elapsed")?).abs() < 1e-12);
    }
    ctx.close()?;
    Ok(())
}
#[test]
fn partial_modifier_failure_releases_every_owned_key_and_retries_failed_cleanup()
-> Result<(), Box<dyn Error>> {
    let (state, mut ctx) = fixture();
    ctx.prepare()?;
    {
        let mut state = lock(&state);
        state.fail_down = Some("shift".into());
        state.fail_up = 1;
        state.fail_close = 1;
    }
    let result = ctx.click_absolute([300.0, 400.0], "right", true);
    assert!(result.is_err());
    assert!(ctx.finish(result).is_err());
    ctx.close()?;
    let trace = &lock(&state).trace;
    assert_eq!(
        trace
            .iter()
            .filter(|v| **v == json!(["key", "shift", true]))
            .count(),
        2
    );
    assert_eq!(
        trace
            .iter()
            .filter(|v| **v == json!(["key", "ctrl", true]))
            .count(),
        1
    );
    assert!(!trace.iter().any(|v| v[0] == "click"));
    assert_eq!(trace.iter().filter(|v| **v == json!(["close"])).count(), 2);
    Ok(())
}
#[test]
fn stop_blocks_new_downs_but_allows_release_and_cleanup_continues_after_panic()
-> Result<(), Box<dyn Error>> {
    let (state, mut ctx) = fixture();
    ctx.prepare()?;
    ctx.key_down("ctrl")?;
    ctx.cancellation().cancel();
    assert!(ctx.key_down("shift").is_err());
    lock(&state).panic_unbind = true;
    assert!(ctx.close().is_err());
    assert!(lock(&state).trace.iter().any(|v| *v == json!(["close"])));
    ctx.close()?;
    let trace = &lock(&state).trace;
    assert!(trace.iter().any(|v| *v == json!(["key", "ctrl", true])));
    assert!(!trace.iter().any(|v| *v == json!(["key", "shift", false])));
    Ok(())
}
#[test]
fn diagnostic_event_backlog_is_bounded_and_reports_omission() {
    let events = ActionEvents::default();
    for index in 0..300 {
        events.push(d2bot::actions::ActionEvent {
            level: EventLevel::Info,
            message: index.to_string(),
        });
    }
    let drained = events.drain();
    assert_eq!(drained.len(), 257);
    assert_eq!(drained[0].level, EventLevel::Warning);
    assert!(drained[0].message.contains("44 older"));
    assert_eq!(drained[1].message, "44");
    assert_eq!(drained[256].message, "299");
    assert!(events.drain().is_empty());
}

#[test]
fn loot_worker_capture_click_quiet_deadline_and_events_match_python() -> Result<(), Box<dyn Error>>
{
    use d2bot::{
        actions::loot_pickup::{self, LootScanner},
        common::{config::BotConfig, vision_ops::codec},
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = "assets/items/keys/key-of-hate-on-the-ground.png";
    let template = codec::load_color(root.join(path))?;
    let mut first = BgrFrame::zeros(Size::new(640, 480)?)?;
    for y in 0..template.size().height() {
        let from = y * template.size().width() * 3;
        let to = ((y + 30) * 640 + 20) * 3;
        first.pixels_mut()[to..to + template.size().width() * 3]
            .copy_from_slice(&template.pixels()[from..from + template.size().width() * 3]);
    }
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"loot_session","template_path":path}),
    )?;
    let state = Arc::new(Mutex::new(State {
        pointer: [150, 250],
        ..State::default()
    }));
    lock(&state).frames.push_back(Arc::new(first));
    let mut ctx = ActionContext::new(
        FakeBackend(state.clone()),
        Cancellation::default(),
        PointerStyle::Loot,
    );
    let config = BotConfig::from_value(
        json!({"shared_loot":{"fixed_items":[{"label":"fixture","ground_template":path}]}}),
    )?;
    let mut scanner = LootScanner::new(&config, root)?;
    assert_eq!(loot_pickup::run_session(&mut scanner, &mut ctx)?, 1);
    let events = ctx.take_events();
    assert_eq!(json!(events), expected["events"]);
    let state = lock(&state);
    // Explicit capture closure is the approved ownership correction; all
    // other source effects and positive-duration waits remain in the trace.
    let trace = state
        .trace
        .iter()
        .filter(|e| e[0] != "close")
        .cloned()
        .collect::<Vec<_>>();
    compare(&json!(trace), &expected["trace"])?;
    assert!((state.now - expected["elapsed"].as_f64().ok_or("elapsed")?).abs() < 1e-12);
    assert_eq!(state.trace.last(), Some(&json!(["close"])));
    Ok(())
}

#[test]
fn gem_initial_missing_panel_reports_failure_unbinds_and_closes_capture()
-> Result<(), Box<dyn Error>> {
    use d2bot::actions::gem_summing::{self, GemVision};
    let vision = GemVision::load(env!("CARGO_MANIFEST_DIR"))?;
    let (state, mut ctx) = fixture();
    assert!(gem_summing::run_session(&vision, &mut ctx).is_err());
    let events = ctx.take_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, EventLevel::Error);
    assert!(
        events[0]
            .message
            .contains("opened gem stash panel in the starting screen")
    );
    assert_eq!(
        lock(&state).trace,
        [
            json!(["bind"]),
            json!(["capture"]),
            json!(["unbind"]),
            json!(["close"])
        ]
    );
    assert!(ctx.capture().is_err());
    Ok(())
}

#[test]
fn complete_room_create_exit_effects_waits_and_events_match_source() -> Result<(), Box<dyn Error>> {
    use d2bot::actions::{
        run_lifecycle::{self, LifecycleSight, RoomTemplates},
        session::SessionOutcome,
        town_loop::Difficulty,
    };
    struct Menus;
    impl LifecycleSight for Menus {
        fn menu_label(
            &mut self,
            _: Arc<BgrFrame>,
            label: &str,
            exclude: bool,
        ) -> Result<bool, ActionError> {
            assert_eq!(exclude, label == "play");
            Ok(true)
        }
    }
    let pattern = |start: u16| -> Result<BgrFrame, ActionError> {
        Ok(BgrFrame::from_vec(
            Size::new(3, 3)?,
            (0..27).map(|n| ((n * 37 + start) % 256) as u8).collect(),
        )?)
    };
    let templates = RoomTemplates {
        difficulty: [("hell".into(), pattern(11)?)].into_iter().collect(),
        loading: pattern(73)?,
        exit: pattern(151)?,
    };
    let scene = |template: Option<&BgrFrame>| -> Result<Arc<BgrFrame>, ActionError> {
        let mut frame = BgrFrame::zeros(Size::new(1922, 1140)?)?;
        if let Some(template) = template {
            for y in 0..3 {
                frame.pixels_mut()[((y + 40) * 1922 + 30) * 3..((y + 40) * 1922 + 33) * 3]
                    .copy_from_slice(
                        template
                            .view()
                            .row(y)
                            .ok_or_else(|| ActionError::runtime("row"))?,
                    );
            }
        }
        Ok(Arc::new(frame))
    };
    let state = Arc::new(Mutex::new(State {
        pointer: [150, 250],
        target: Some(PhysicalTarget::new(100, 200, 1922, 1140)?),
        frames: [
            scene(None)?,
            scene(None)?,
            scene(templates.difficulty.get("hell"))?,
            scene(Some(&templates.loading))?,
            scene(None)?,
            scene(Some(&templates.exit))?,
            scene(None)?,
        ]
        .into_iter()
        .collect(),
        ..State::default()
    }));
    let mut context = ActionContext::new(
        FakeBackend(state.clone()),
        Cancellation::default(),
        PointerStyle::Lifecycle,
    );
    let expected = support::legacy_expectation("action-oracle", &json!({"kind":"lifecycle_flow"}))?;
    assert_eq!(
        run_lifecycle::run_rooms(
            &mut context,
            &mut Menus,
            &templates,
            Some(1),
            Difficulty::Hell
        )?,
        SessionOutcome::Completed
    );
    assert_eq!(json!(context.take_events()), expected["events"]);
    compare(&json!(lock(&state).trace), &expected["trace"])?;
    assert!((lock(&state).now - expected["elapsed"].as_f64().ok_or("elapsed")?).abs() < 1e-12);
    assert!(
        run_lifecycle::locate_template(
            BgrFrame::zeros(Size::new(1, 1)?)?.view(),
            templates.loading.view(),
            0.0
        )?
        .is_none()
    );
    Ok(())
}
