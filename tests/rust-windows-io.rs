use d2bot::common::{
    capture::{CaptureRegion, CaptureSettings, ScreenCapture, find_window},
    controller::{ControllerFieldInput, InputGate, normalize_pointer},
    error::ControlError,
    field_control::{Enemy, FieldInputBackend, FieldObservation, NavigationEvidence},
    frame::{BgrFrame, PixelRect, Size},
    frame_geometry::{PhysicalTarget, ReferenceLayout},
    hotkeys::{HotkeyAction, HotkeyBinding, HotkeyTracker},
    movement::{MovementExecutionState, MovementIntent, MovementMode},
    survival::{Location, ObservationStamp, SafetyObservation},
    time::MonotonicClock,
};
use d2bot::platform::{CaptureBackend, InputEvent, InputSystem, WindowId, WindowInfo};
use std::{
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
mod support;

#[test]
fn all_installed_keyboard_payloads_match_fake_python_dispatch() -> Result<(), Box<dyn Error>> {
    for numlock in [false, true] {
        let expected = support::legacy_expectation(
            "windows-io-oracle",
            &serde_json::json!({"numlock":numlock}),
        )?;
        let (system, backend) = input()?;
        system.numlock.store(numlock, Ordering::SeqCst);
        for (key, events) in expected.as_object().ok_or("oracle object missing")? {
            system.events.lock().map_err(|e| e.to_string())?.clear();
            backend.key_down(key)?;
            backend.key_up(key)?;
            let actual: Vec<_> = system
                .events
                .lock()
                .map_err(|e| e.to_string())?
                .iter()
                .map(|event| match event {
                    InputEvent::Keyboard { scan, up, extended } => {
                        serde_json::json!({"scan":scan,"up":up,"extended":extended})
                    }
                    _ => serde_json::Value::Null,
                })
                .collect();
            assert_eq!(
                *events,
                serde_json::json!(actual),
                "{key}, numlock={numlock}"
            );
        }
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn action_adapter_uses_raw_payloads_current_geometry_and_town_identity_guard()
-> Result<(), Box<dyn Error>> {
    use d2bot::actions::{
        desktop::DesktopActionBackend,
        runtime::{ActionBackend, FocusOutcome},
    };
    let window = info(7, "Diablo", target(-100, 20, 64, 48)?)?;
    let capture_state = Arc::new(Mutex::new(CaptureState {
        windows: vec![window.clone()],
        ..CaptureState::default()
    }));
    let capture = ScreenCapture::new(
        FakeCapture(capture_state.clone()),
        CaptureSettings {
            window_title: Some("Diablo".into()),
            ..CaptureSettings::default()
        },
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
    )?;
    let system = Arc::new(FakeInput::new(InputGate::default()));
    let focused = Arc::new(AtomicUsize::new(0));
    let count = focused.clone();
    let mut backend = DesktopActionBackend::new(capture, system.clone(), move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    })?;
    assert!(system.events.lock().map_err(|e| e.to_string())?.is_empty());
    assert_eq!(focused.load(Ordering::SeqCst), 0);
    assert!(matches!(backend.focus_configured()?,FocusOutcome::Focused(t)if t==window.target));
    backend.start_town_guard()?;
    backend.town_guard()?;
    backend.capture()?;
    assert_eq!(backend.frame_to_desktop([2.5, 3.5])?, [-98.0, 24.0]);
    assert_eq!(backend.desktop_to_frame([-98.0, 24.0])?, [2, 4]);
    backend.move_desktop([-50, 40])?;
    backend.click("right")?;
    backend.mouse_button("left", true)?;
    assert_eq!(system.pointer()?, [-50, 40]);
    let events = system.events.lock().map_err(|e| e.to_string())?.clone();
    assert!(matches!(events[0], InputEvent::Mouse { flags: 0xc001, .. }));
    assert_eq!(
        events[1],
        InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags: 24
        }
    );
    assert_eq!(
        events[2],
        InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags: 4
        }
    );
    system.numlock.store(true, Ordering::SeqCst);
    system.fail_up.store(true, Ordering::SeqCst);
    system.events.lock().map_err(|e| e.to_string())?.clear();
    assert!(backend.key("right", true).is_err());
    assert_eq!(system.events.lock().map_err(|e| e.to_string())?.len(), 2);
    system.focused.store(false, Ordering::SeqCst);
    assert!(
        backend
            .town_guard()
            .err()
            .ok_or("focus guard")?
            .to_string()
            .contains("focus was lost")
    );
    system.focused.store(true, Ordering::SeqCst);
    capture_state.lock().map_err(|e| e.to_string())?.windows[0].id = WindowId::new(8)?;
    assert!(backend.town_guard().is_err());
    capture_state.lock().map_err(|e| e.to_string())?.windows[0].target = target(-101, 20, 64, 48)?;
    assert!(!backend.geometry_is_current()?);
    assert!(backend.frame_to_desktop([2.0, 3.0]).is_err());
    backend.close_capture()?;
    Ok(())
}

fn target(x: i32, y: i32, w: usize, h: usize) -> Result<PhysicalTarget, Box<dyn Error>> {
    Ok(PhysicalTarget::new(x, y, w, h)?)
}
fn info(id: usize, title: &str, target: PhysicalTarget) -> Result<WindowInfo, Box<dyn Error>> {
    Ok(WindowInfo {
        id: WindowId::new(id)?,
        title: title.into(),
        target,
    })
}
#[derive(Default)]
struct CaptureState {
    windows: Vec<WindowInfo>,
    monitors: Vec<PhysicalTarget>,
    client: Option<PixelRect>,
    window_frame: bool,
    events: Vec<String>,
    after_capture: Option<PhysicalTarget>,
    close_fail: bool,
}
#[derive(Clone)]
struct FakeCapture(Arc<Mutex<CaptureState>>);
impl CaptureBackend for FakeCapture {
    fn windows(&self) -> Result<Vec<WindowInfo>, ControlError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))?
            .windows
            .clone())
    }
    fn monitors(&self) -> Result<Vec<PhysicalTarget>, ControlError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))?
            .monitors
            .clone())
    }
    fn client_box(&self, _: &WindowInfo) -> Result<PixelRect, ControlError> {
        self.0
            .lock()
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))?
            .client
            .ok_or_else(|| ControlError::Backend("client unavailable".into()))
    }
    fn capture_window(&mut self, window: &WindowInfo) -> Result<Option<BgrFrame>, ControlError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))?;
        state.events.push("window".into());
        if let Some(target) = state.after_capture {
            state.windows[0].target = target;
        }
        if state.window_frame {
            Ok(Some(
                BgrFrame::from_vec(
                    window.target.size(),
                    vec![
                        7;
                        window
                            .target
                            .size()
                            .byte_len()
                            .map_err(|e| ControlError::Backend(e.to_string()))?
                    ],
                )
                .map_err(|e| ControlError::Backend(e.to_string()))?,
            ))
        } else {
            Ok(None)
        }
    }
    fn capture_screen(&mut self, target: PhysicalTarget) -> Result<BgrFrame, ControlError> {
        self.0
            .lock()
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))?
            .events
            .push("screen".into());
        BgrFrame::zeros(target.size()).map_err(|e| ControlError::Backend(e.to_string()))
    }
    fn close(&mut self) -> Result<(), ControlError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))?;
        state.events.push("close".into());
        if state.close_fail {
            Err(ControlError::Backend("fake close failure".into()))
        } else {
            Ok(())
        }
    }
}
fn fake_capture() -> Result<FakeCapture, Box<dyn Error>> {
    Ok(FakeCapture(Arc::new(Mutex::new(CaptureState {
        windows: vec![info(1, "Diablo", target(-100, 20, 100, 80)?)?],
        monitors: vec![target(-1920, 0, 3840, 1080)?, target(0, 0, 1920, 1080)?],
        ..Default::default()
    }))))
}
#[test]
fn focused_control_auto_uses_screen_but_explicit_window_remains() -> Result<(), Box<dyn Error>> {
    for (mode, expected) in [
        ("auto", "screen"),
        ("window", "window"),
        ("screen", "screen"),
    ] {
        let backend = fake_capture()?;
        let state = backend.0.clone();
        state.lock().map_err(|e| e.to_string())?.window_frame = true;
        let mut capture = ScreenCapture::with_layout(
            backend,
            CaptureSettings {
                window_title: Some("Diablo".into()),
                capture_backend: mode.into(),
                ..Default::default()
            },
            None,
        )?;
        capture.prefer_focused_screen();
        let frame = capture.grab()?;
        assert_eq!(frame.target, target(-100, 20, 100, 80)?);
        assert_eq!(
            d2bot::common::realtime::ScreenSource::capture_method(&capture),
            Some(expected)
        );
        assert_eq!(state.lock().map_err(|e| e.to_string())?.events, [expected]);
        capture.close()?;
    }
    Ok(())
}
#[test]
fn capture_precedence_modes_fallback_close_and_negative_monitor_index() -> Result<(), Box<dyn Error>>
{
    for (mode, available, expected, error) in [
        ("auto", true, vec!["window"], false),
        ("auto", false, vec!["window", "screen"], false),
        ("window", false, vec!["window"], true),
        ("screen", true, vec!["screen"], false),
        ("other", true, vec!["screen"], false),
    ] {
        let backend = fake_capture()?;
        backend
            .0
            .lock()
            .map_err(|error| error.to_string())?
            .window_frame = available;
        let state = backend.0.clone();
        let mut capture = ScreenCapture::with_layout(
            backend,
            CaptureSettings {
                window_title: Some("diablo".into()),
                capture_backend: mode.into(),
                region: Some(CaptureRegion {
                    left: 0,
                    top: 0,
                    width: 1,
                    height: 1,
                }),
                ..Default::default()
            },
            None,
        )?;
        assert_eq!(target(-100, 20, 100, 80)?, capture.target());
        let result = capture.grab();
        assert_eq!(error, result.is_err());
        if let Ok(packet) = result {
            assert_eq!(packet.target.size(), packet.frame.size());
            assert!(
                packet
                    .wall_captured_at
                    .is_some_and(|time| time > 1_000_000_000.0)
            );
        }
        assert_eq!(
            expected,
            state.lock().map_err(|error| error.to_string())?.events
        );
        capture.close()?;
        capture.close()?;
        assert!(capture.grab().is_err());
        assert_eq!(
            1,
            state
                .lock()
                .map_err(|error| error.to_string())?
                .events
                .iter()
                .filter(|event| *event == "close")
                .count()
        );
    }
    let backend = fake_capture()?;
    let mut capture = ScreenCapture::with_layout(
        backend,
        CaptureSettings {
            monitor_index: -1,
            capture_backend: "window".into(),
            ..Default::default()
        },
        None,
    )?;
    assert_eq!(target(0, 0, 1920, 1080)?, capture.target());
    capture.close()?;
    let backend = fake_capture()?;
    let state = backend.0.clone();
    assert!(
        ScreenCapture::with_layout(
            backend,
            CaptureSettings {
                monitor_index: 99,
                ..Default::default()
            },
            None
        )
        .is_err()
    );
    assert_eq!(
        vec!["close"],
        state.lock().map_err(|error| error.to_string())?.events
    );
    Ok(())
}
#[test]
fn capture_reuses_geometry_rejects_changed_window_and_retries_explicit_close()
-> Result<(), Box<dyn Error>> {
    let backend = fake_capture()?;
    let state = backend.0.clone();
    state.lock().map_err(|error| error.to_string())?.client = Some(PixelRect::new(0, 0, 100, 80)?);
    let layout = ReferenceLayout::new(Size::new(50, 40)?, PixelRect::new(0, 0, 50, 40)?, 0.01)?;
    let mut capture = ScreenCapture::with_layout(
        backend,
        CaptureSettings {
            window_title: Some("Diablo".into()),
            ..Default::default()
        },
        Some(layout),
    )?;
    let a = capture.grab()?;
    let b = capture.grab()?;
    assert_eq!(Size::new(50, 40)?, a.frame.size());
    assert!(matches!((&a.geometry,&b.geometry),(Some(a),Some(b)) if Arc::ptr_eq(a,b)));
    state
        .lock()
        .map_err(|error| error.to_string())?
        .after_capture = Some(target(-99, 20, 100, 80)?);
    let old = capture.geometry().cloned();
    assert!(capture.grab().is_err());
    assert!(matches!((old,capture.geometry()),(Some(a),Some(b)) if Arc::ptr_eq(&a,b)));
    state.lock().map_err(|error| error.to_string())?.close_fail = true;
    assert!(capture.close().is_err());
    state.lock().map_err(|error| error.to_string())?.close_fail = false;
    capture.close()?;
    Ok(())
}
#[test]
fn title_matching_uses_frozen_full_casefold_and_first_match() -> Result<(), Box<dyn Error>> {
    let bounds = target(0, 0, 1, 1)?;
    let windows = vec![
        info(1, "Straße Σς İ 한국어", bounds)?,
        info(2, "STRASSE σσ i\u{307} 한국어", bounds)?,
    ];
    assert_eq!(
        Some(WindowId::new(1)?),
        find_window(windows.clone(), "STRASSE σσ i\u{307} 한국어", "exact").map(|w| w.id)
    );
    assert_eq!(
        Some(WindowId::new(1)?),
        find_window(windows.clone(), "STRASSE", "non-exact").map(|w| w.id)
    );
    assert!(find_window(windows, "STRASSE", "exact").is_none());
    Ok(())
}

struct TestClock(AtomicUsize);
impl MonotonicClock for TestClock {
    fn now(&self) -> f64 {
        self.0.fetch_add(1, Ordering::SeqCst) as f64 * 0.01
    }
}
struct FakeInput {
    events: Mutex<Vec<InputEvent>>,
    focused: AtomicBool,
    pointer: Mutex<[i32; 2]>,
    follow: AtomicBool,
    fail_down: AtomicBool,
    fail_up: AtomicBool,
    panic_down: AtomicBool,
    numlock: AtomicBool,
    gate: InputGate,
    interrupt_move: AtomicBool,
    bounds_gate: Mutex<
        Option<(
            std::sync::mpsc::SyncSender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
}
impl FakeInput {
    fn new(gate: InputGate) -> Self {
        Self {
            events: Mutex::new(Vec::new()),
            focused: AtomicBool::new(true),
            pointer: Mutex::new([50, 50]),
            follow: AtomicBool::new(true),
            fail_down: AtomicBool::new(false),
            fail_up: AtomicBool::new(false),
            panic_down: AtomicBool::new(false),
            numlock: AtomicBool::new(false),
            gate,
            interrupt_move: AtomicBool::new(false),
            bounds_gate: Mutex::new(None),
        }
    }
}
impl InputSystem for FakeInput {
    fn focused(&self, _: WindowId) -> Result<bool, ControlError> {
        Ok(self.focused.load(Ordering::SeqCst))
    }
    fn bounds(&self, _: WindowId) -> Result<PhysicalTarget, ControlError> {
        let gate = self
            .bounds_gate
            .lock()
            .map_err(|_| ControlError::Runtime("fake bounds gate poisoned".into()))?
            .take();
        if let Some((started, resume)) = gate {
            started
                .send(())
                .map_err(|e| ControlError::Runtime(e.to_string()))?;
            resume
                .recv_timeout(Duration::from_secs(2))
                .map_err(|e| ControlError::Runtime(e.to_string()))?;
        }
        PhysicalTarget::new(-1920, -1080, 3840, 2160)
            .map_err(|e| ControlError::Backend(e.to_string()))
    }
    fn desktop(&self) -> Result<PhysicalTarget, ControlError> {
        self.bounds(WindowId::new(1)?)
    }
    fn pointer(&self) -> Result<[i32; 2], ControlError> {
        self.pointer
            .lock()
            .map(|p| *p)
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))
    }
    fn numlock_state(&self) -> Result<bool, ControlError> {
        Ok(self.numlock.load(Ordering::SeqCst))
    }
    fn arrow_scan(&self, vk: u32) -> Result<u16, ControlError> {
        Ok((vk + 0x100) as u16)
    }
    fn send(&self, event: InputEvent) -> Result<u32, ControlError> {
        self.events
            .lock()
            .map_err(|_| ControlError::Runtime("fake poisoned".into()))?
            .push(event);
        if matches!(event, InputEvent::Keyboard { up: false, .. })
            && self.panic_down.load(Ordering::SeqCst)
        {
            panic!("fake panic after key-down attempt");
        }
        if let InputEvent::Mouse {
            dx,
            dy,
            flags: 0xc001,
        } = event
        {
            if self.interrupt_move.load(Ordering::SeqCst) {
                self.gate.stop();
            }
            if self.follow.load(Ordering::SeqCst) {
                *self
                    .pointer
                    .lock()
                    .map_err(|_| ControlError::Runtime("fake poisoned".into()))? = [
                    (f64::from(dx) * 3839.0 / 65535.0 - 1920.0).round_ties_even() as i32,
                    (f64::from(dy) * 2159.0 / 65535.0 - 1080.0).round_ties_even() as i32,
                ];
            }
        }
        if (matches!(event, InputEvent::Keyboard { up: true, .. })
            && self.fail_up.load(Ordering::SeqCst))
            || (matches!(event, InputEvent::Keyboard { up: false, .. })
                && self.fail_down.load(Ordering::SeqCst))
        {
            Ok(0)
        } else {
            Ok(1)
        }
    }
}
fn input() -> Result<(Arc<FakeInput>, ControllerFieldInput<FakeInput>), Box<dyn Error>> {
    let gate = InputGate::default();
    let system = Arc::new(FakeInput::new(gate.clone()));
    let backend = ControllerFieldInput::new(
        system.clone(),
        gate,
        WindowId::new(1)?,
        Arc::new(TestClock(AtomicUsize::new(0))),
    );
    Ok((system, backend))
}
#[test]
fn keyboard_payloads_partial_down_cleanup_and_release_after_focus_pause_stop()
-> Result<(), Box<dyn Error>> {
    let (system, backend) = input()?;
    backend.key_down("f2")?;
    backend.key_up("f2")?;
    assert_eq!(
        vec![
            InputEvent::Keyboard {
                scan: 0x3c,
                up: false,
                extended: false
            },
            InputEvent::Keyboard {
                scan: 0x3c,
                up: true,
                extended: false
            }
        ],
        *system.events.lock().map_err(|error| error.to_string())?
    );
    system
        .events
        .lock()
        .map_err(|error| error.to_string())?
        .clear();
    system.fail_down.store(true, Ordering::SeqCst);
    assert!(backend.press("f4").is_err());
    assert_eq!(
        2,
        system
            .events
            .lock()
            .map_err(|error| error.to_string())?
            .len()
    );
    assert!(matches!(
        system.events.lock().map_err(|error| error.to_string())?[1],
        InputEvent::Keyboard { up: true, .. }
    ));
    system.fail_down.store(false, Ordering::SeqCst);
    system.focused.store(false, Ordering::SeqCst);
    backend.gate().pause(true);
    backend.gate().stop();
    assert!(backend.key_down("f2").is_err());
    backend.key_up("f2")?;
    system.numlock.store(true, Ordering::SeqCst);
    system
        .events
        .lock()
        .map_err(|error| error.to_string())?
        .clear();
    backend.key_up("up")?;
    assert_eq!(
        vec![
            InputEvent::Keyboard {
                scan: 0x126,
                up: true,
                extended: true
            },
            InputEvent::Keyboard {
                scan: 0xe0,
                up: true,
                extended: false
            }
        ],
        *system.events.lock().map_err(|error| error.to_string())?
    );
    assert!(backend.key_up("unsupported").is_err());
    Ok(())
}

#[test]
fn failed_one_shot_release_stays_owned_for_explicit_retry() -> Result<(), Box<dyn Error>> {
    let (system, backend) = input()?;
    system.fail_up.store(true, Ordering::SeqCst);
    assert!(backend.press("f4").is_err());
    assert!(backend.close().is_err());
    assert_eq!(3, system.events.lock().map_err(|e| e.to_string())?.len());
    system.fail_up.store(false, Ordering::SeqCst);
    backend.close()?;
    assert_eq!(4, system.events.lock().map_err(|e| e.to_string())?.len());
    backend.close()?;
    assert_eq!(4, system.events.lock().map_err(|e| e.to_string())?.len());
    assert!(backend.key_down("f4").is_err());
    Ok(())
}

#[test]
fn poisoned_dispatch_still_releases_owned_keys_and_reports_the_fault() -> Result<(), Box<dyn Error>>
{
    let (system, backend) = input()?;
    system.panic_down.store(true, Ordering::SeqCst);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.key_down("f2"))).is_err()
    );
    assert!(backend.close().is_err());
    assert_eq!(
        vec![
            InputEvent::Keyboard {
                scan: 0x3c,
                up: false,
                extended: false
            },
            InputEvent::Keyboard {
                scan: 0x3c,
                up: true,
                extended: false
            }
        ],
        *system.events.lock().map_err(|e| e.to_string())?
    );
    assert!(backend.key_down("f2").is_err());
    Ok(())
}

#[test]
fn stop_during_bounds_query_releases_and_prevents_late_pointer_dispatch()
-> Result<(), Box<dyn Error>> {
    let (system, backend) = input()?;
    let backend = Arc::new(backend);
    backend.key_down("f2")?;
    let (started, observed) = std::sync::mpsc::sync_channel(1);
    let (resume, waiting) = std::sync::mpsc::sync_channel(1);
    *system.bounds_gate.lock().map_err(|e| e.to_string())? = Some((started, waiting));
    let moving = backend.clone();
    let worker = std::thread::spawn(move || moving.move_pointer([-1400, 60]));
    observed.recv_timeout(Duration::from_secs(1))?;
    let start = Instant::now();
    backend.close()?;
    assert!(start.elapsed() < Duration::from_millis(100));
    resume.send(())?;
    assert!(
        worker
            .join()
            .map_err(|_| "pointer worker panicked")?
            .is_err()
    );
    assert!(
        system
            .events
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .all(|event| matches!(event, InputEvent::Keyboard { .. }))
    );
    Ok(())
}

#[test]
fn invalid_layout_closes_the_supplied_backend_before_returning_error() -> Result<(), Box<dyn Error>>
{
    let backend = fake_capture()?;
    let state = backend.0.clone();
    assert!(
        ScreenCapture::new(
            backend,
            CaptureSettings {
                reference_layout: Some("missing.json".into()),
                ..Default::default()
            },
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        )
        .is_err()
    );
    assert_eq!(
        vec!["close"],
        state.lock().map_err(|e| e.to_string())?.events
    );
    Ok(())
}
#[test]
fn pointer_negative_origins_confirmation_timeout_and_interrupt_have_no_resend()
-> Result<(), Box<dyn Error>> {
    assert_eq!(
        [0, 0],
        normalize_pointer(target(-1920, -1080, 3840, 2160)?, [-1920, -1080])?
    );
    assert_eq!(
        [65535, 65535],
        normalize_pointer(target(-1920, -1080, 3840, 2160)?, [1919, 1079])?
    );
    assert!(normalize_pointer(target(0, 0, 1, 2)?, [0, 0]).is_err());
    let (system, backend) = input()?;
    backend.move_pointer([-1400, -100])?;
    assert!(backend.pointer_matches([-1400, -100])?);
    assert_eq!(
        Some([-1400, -100]),
        backend.pointer_evidence()?.map(|e| e.requested)
    );
    system.follow.store(false, Ordering::SeqCst);
    system
        .events
        .lock()
        .map_err(|error| error.to_string())?
        .clear();
    assert!(backend.move_pointer([100, 100]).is_err());
    assert_eq!(
        1,
        system
            .events
            .lock()
            .map_err(|error| error.to_string())?
            .len()
    );
    assert!(backend.pointer_evidence()?.is_none());
    system.interrupt_move.store(true, Ordering::SeqCst);
    system
        .events
        .lock()
        .map_err(|error| error.to_string())?
        .clear();
    assert!(backend.move_pointer([100, 100]).is_err());
    assert_eq!(
        1,
        system
            .events
            .lock()
            .map_err(|error| error.to_string())?
            .len()
    );
    Ok(())
}
#[test]
fn mouse_release_is_unconditional_and_gate_wait_cancels_promptly() -> Result<(), Box<dyn Error>> {
    let (system, backend) = input()?;
    backend.gate().stop();
    assert!(backend.mouse_button("left", false).is_err());
    backend.mouse_button("left", true)?;
    assert_eq!(
        vec![InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags: 4
        }],
        *system.events.lock().map_err(|error| error.to_string())?
    );
    let gate = InputGate::default();
    let worker_gate = gate.clone();
    let thread = std::thread::spawn(move || worker_gate.wait(Duration::from_secs(30)));
    let start = Instant::now();
    gate.pause(true);
    assert!(thread.join().map_err(|_| "wait thread panicked")??);
    assert!(start.elapsed() < Duration::from_millis(200));
    Ok(())
}
#[test]
fn movement_cleanup_and_hotkey_traces_keep_source_contract() -> Result<(), Box<dyn Error>> {
    let (system, backend) = input()?;
    let backend = Arc::new(backend);
    let mut movement = MovementExecutionState::new(backend.clone(), "f2".into());
    movement.apply(MovementMode::Hold)?;
    movement.apply(MovementMode::Hold)?;
    assert_eq!(
        1,
        system
            .events
            .lock()
            .map_err(|error| error.to_string())?
            .len()
    );
    assert!(movement.held());
    movement.release(true)?;
    assert!(movement.stopped());
    assert!(!movement.held());
    assert_eq!(
        "with movement stopped.",
        movement.apply(MovementMode::Press)?
    );
    assert_eq!(
        MovementMode::Hold,
        MovementMode::resolve("bad", MovementIntent::Travel)
    );
    assert_eq!(
        MovementMode::Press,
        MovementMode::resolve("bad", MovementIntent::Reposition)
    );
    let mut hotkeys = HotkeyTracker::new(vec![
        HotkeyBinding::parse("f8", HotkeyAction::TogglePause)?,
        HotkeyBinding::parse("ctrl+f9", HotkeyAction::Stop)?,
    ])?;
    assert_eq!(Some(HotkeyAction::TogglePause), hotkeys.event(0x77, true));
    assert_eq!(Some(HotkeyAction::TogglePause), hotkeys.event(0x77, true));
    assert_eq!(None, hotkeys.event(0x77, false));
    hotkeys.event(0x71, true);
    assert_eq!(None, hotkeys.event(0x77, true));
    hotkeys.event(0x77, false);
    hotkeys.event(0x71, false);
    hotkeys.event(0xa2, true);
    assert_eq!(None, hotkeys.event(0x77, true));
    hotkeys.event(0x77, false);
    assert_eq!(Some(HotkeyAction::Stop), hotkeys.event(0x78, true));
    assert!(HotkeyBinding::parse("ctrl+ctrl+f8", HotkeyAction::Stop).is_err());
    let safety = SafetyObservation::new(
        ObservationStamp::new("c", "r", 1, 1.0)?,
        Some(1.0),
        Some(1.0),
        Location::Field,
        Some(true),
    )?;
    assert!(
        FieldObservation::new(
            safety,
            Some(true),
            Enemy::Clear,
            NavigationEvidence {
                travel_aim: Some([-1, 10]),
                ..Default::default()
            }
        )
        .is_err()
    );
    Ok(())
}

#[cfg(windows)]
#[test]
#[ignore = "explicit read-only desktop capture; never sends input or saves pixels"]
fn native_capture_smoke() -> Result<(), Box<dyn Error>> {
    use d2bot::platform::windows::{Windows, enable_dpi_awareness};
    enable_dpi_awareness()?;
    let mut backend = Windows::new()?;
    let monitors = backend.monitors()?;
    let desktop = monitors.first().ok_or("no virtual desktop")?;
    let region = target(desktop.left, desktop.top, 8, 8)?;
    for _ in 0..32 {
        let frame = backend.capture_screen(region)?;
        assert_eq!(region.size(), frame.size());
    }
    backend.close()?;
    // No title or pixel values are printed or persisted.
    Ok(())
}
