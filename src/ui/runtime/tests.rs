use super::*;
use crate::{actions::runtime::ActionEvents, ui::native::NativeActionBackend};
use std::{
    error::Error,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};
type TestResult = Result<(), Box<dyn Error>>;
#[derive(Clone, Default)]
struct Recorder {
    running: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    configured: Arc<AtomicUsize>,
    joined: Arc<AtomicUsize>,
    events: ActionEvents,
}
impl RecordingPort for Recorder {
    fn configure(&mut self, _: &BotConfig) -> Result<(), RunError> {
        self.configured.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn start(&mut self) -> Result<PathBuf, RunError> {
        self.running.store(true, Ordering::SeqCst);
        Ok("recordings/fake.avi".into())
    }
    fn request_stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    fn drain_events(&self) -> Vec<ActionEvent> {
        self.events.drain()
    }
    fn finish(&mut self, _: Duration) -> Result<(), RunError> {
        assert!(!self.is_running());
        self.joined.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
fn panel(
    recorder: Recorder,
    effects: PanelEffects<NativeActionBackend>,
) -> Result<PanelRuntime<NativeActionBackend, Recorder>, RunError> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    PanelRuntime::new(
        BotConfig::load(root.join("config"))?,
        root,
        recorder,
        effects,
    )
}
fn effects() -> PanelEffects<NativeActionBackend> {
    PanelEffects {
        backend: Arc::new(|_, _| Err(ActionError::runtime("fake capture failure"))),
        summoner: Box::new(|session, _, request| {
            session.start_with(request, || Ok("recordings/fake-summoner".into()), |_| Ok(0))
        }),
        snapshot: Arc::new(|_, _, _| Ok("assets/private/gui_snapshot.png".into())),
        windows: Box::new(|| {
            Ok(vec![
                "Diablo2 Helper".into(),
                "Diablo II: Resurrected".into(),
            ])
        }),
    }
}
fn until(mut condition: impl FnMut() -> Result<bool, RunError>) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition()? {
        if Instant::now() >= deadline {
            return Err("fake worker did not finish".into());
        }
        std::thread::yield_now();
    }
    Ok(())
}
#[test]
fn real_worker_keeps_exclusivity_through_stop_and_close_without_blocking_ui() -> TestResult {
    let recorder = Recorder::default();
    let mut fx = effects();
    let (entered, started) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    fx.backend = Arc::new(move |_, _| {
        entered
            .send(())
            .map_err(|e| ActionError::runtime(e.to_string()))?;
        gate.lock()
            .map_err(|_| ActionError::runtime("fake gate poisoned"))?
            .recv()
            .map_err(|e| ActionError::runtime(e.to_string()))?;
        Err(ActionError::runtime("fake capture failure"))
    });
    let mut panel = panel(recorder.clone(), fx)?;
    panel.intent(Intent::Gems).map_err(|e| e.message)?;
    started.recv_timeout(Duration::from_secs(3))?;
    assert!(panel.state.activity.gem);
    assert!(!panel.state.controls().actions);
    assert_eq!(recorder.configured.load(Ordering::SeqCst), 1);
    let error = panel
        .intent(Intent::Loot)
        .err()
        .ok_or("expected busy error")?;
    assert_eq!(error.title, "Item Looting Error");
    assert!(error.message.starts_with("Another game action is active."));
    panel.intent(Intent::Record).map_err(|e| e.message)?;
    assert!(recorder.is_running());
    assert_eq!(recorder.configured.load(Ordering::SeqCst), 1);
    let before = Instant::now();
    panel.intent(Intent::StopAction).map_err(|e| e.message)?;
    panel.request_close();
    assert!(before.elapsed() < Duration::from_millis(100));
    assert!(panel.gems.is_running());
    assert!(recorder.stopped.load(Ordering::SeqCst));
    assert_eq!(panel.state.status, "Stopping");
    assert!(!panel.ready_to_close());
    panel.request_close();
    assert_eq!(recorder.joined.load(Ordering::SeqCst), 0);
    release.send(())?;
    until(|| Ok(!panel.gems.is_running()))?;
    panel.poll()?;
    assert!(!panel.ready_to_close());
    recorder.running.store(false, Ordering::SeqCst);
    panel.poll()?;
    assert!(panel.ready_to_close());
    assert_eq!(panel.state.status, "Stopping");
    assert_eq!(recorder.joined.load(Ordering::SeqCst), 1);
    assert!(panel.state.log().contains("fake capture failure"));
    Ok(())
}
#[test]
fn source_validation_errors_precede_any_backend_or_output_effect() -> TestResult {
    let mut panel = panel(Recorder::default(), effects())?;
    for (town, count, difficulty, expected) in [
        (
            true,
            "",
            "hell",
            "Supervised town loop requires an explicit repeat count from 1 to 3.",
        ),
        (
            true,
            "4",
            "bad",
            "Supervised town loop requires an explicit repeat count from 1 to 3.",
        ),
        (true, "1", "bad", "Unknown difficulty."),
    ] {
        panel.state.town_loop = town;
        panel.state.repeat_count = count.into();
        panel.state.difficulty = difficulty.into();
        let error = panel
            .intent(Intent::Lifecycle)
            .err()
            .ok_or("expected validation error")?;
        assert_eq!(error.message, expected);
        assert_eq!(panel.state.status, "Idle");
        assert!(panel.joined.iter().all(|v| *v));
    }
    panel.state.repeat_count = "6".into();
    panel.state.difficulty = "bad".into();
    let error = panel
        .intent(Intent::Summoner)
        .err()
        .ok_or("expected repeat error")?;
    assert!(
        error
            .message
            .starts_with("Summoner Repeat Count must be from 1 to 5")
    );
    panel.state.repeat_count = "1".into();
    let error = panel
        .intent(Intent::Summoner)
        .err()
        .ok_or("expected difficulty error")?;
    assert_eq!(
        error.message,
        "Select a valid Summoner difficulty: normal, nightmare, or hell."
    );
    Ok(())
}
#[test]
fn lifecycle_preserves_signed_large_repeat_text_and_default_difficulty() -> TestResult {
    for count in ["-2", "0", "18446744073709551616"] {
        let mut panel = panel(Recorder::default(), effects())?;
        panel.state.repeat_count = count.into();
        panel.state.difficulty = "unrecognized".into();
        panel.intent(Intent::Lifecycle).map_err(|e| e.message)?;
        until(|| Ok(!panel.lifecycle.is_running()))?;
        panel.poll()?;
        assert!(panel.state.log().contains(&format!(
            "Run lifecycle started for {count} run(s) on Hell."
        )));
        assert!(panel.state.log().contains("fake capture failure"));
        assert_eq!(panel.state.status, "Error");
    }
    Ok(())
}
#[test]
fn snapshot_and_recording_finalize_outside_ui_and_config_stays_in_memory() -> TestResult {
    let recorder = Recorder::default();
    let mut fx = effects();
    let (entered, started) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(std::sync::Mutex::new(gate));
    fx.snapshot = Arc::new(move |settings, _, stop| {
        assert!(settings.reference_layout.is_none());
        assert_eq!(settings.window_title.as_deref(), Some("changed title"));
        entered
            .send(())
            .map_err(|e| RunError::Runtime(e.to_string()))?;
        gate.lock()
            .map_err(|_| RunError::Runtime("fake gate".into()))?
            .recv()
            .map_err(|e| RunError::Runtime(e.to_string()))?;
        assert!(stop.is_cancelled());
        Ok("assets/private/gui_snapshot.png".into())
    });
    let mut panel = panel(recorder.clone(), fx)?;
    panel.state.status = "Summoner Run".into();
    panel.state.window_title = " changed title ".into();
    panel.intent(Intent::WindowChanged).map_err(|e| e.message)?;
    assert_eq!(panel.state.status, "Summoner Run");
    panel.intent(Intent::Snapshot).map_err(|e| e.message)?;
    started.recv_timeout(Duration::from_secs(3))?;
    panel.request_close();
    assert!(!panel.ready_to_close());
    release.send(())?;
    until(|| Ok(!panel.snapshot.is_running()))?;
    // 완료와 다음 UI poll 사이에 닫기 검사가 들어와도 결과를 건너뛰지 않는다.
    assert!(!panel.ready_to_close());
    panel.poll()?;
    assert!(panel.ready_to_close());
    assert_eq!(panel.state.output, "assets/private/gui_snapshot.png");
    assert!(
        panel
            .state
            .log()
            .contains("Snapshot saved: gui_snapshot.png")
    );
    assert_eq!(
        panel
            .config()
            .section("capture")
            .and_then(|v| v["window_title"].as_str()),
        Some("changed title")
    );
    Ok(())
}

#[test]
fn completed_snapshot_error_is_consumed_before_close_or_another_snapshot() -> TestResult {
    let mut fx = effects();
    fx.snapshot = Arc::new(|_, _, _| Err(RunError::Runtime("fake snapshot failure".into())));
    let mut panel = panel(Recorder::default(), fx)?;
    panel.intent(Intent::Snapshot).map_err(|e| e.message)?;
    until(|| Ok(!panel.snapshot.is_running()))?;
    assert!(panel.intent(Intent::Snapshot).is_err());
    panel.request_close();
    assert!(!panel.ready_to_close());
    let faults = panel.poll()?;
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].title, "Snapshot Error");
    assert!(faults[0].message.contains("fake snapshot failure"));
    assert!(panel.state.log().contains("fake snapshot failure"));
    assert!(panel.ready_to_close());
    assert!(panel.poll()?.is_empty());
    Ok(())
}
