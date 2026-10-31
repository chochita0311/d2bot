use super::*;
use crate::{
    common::{
        controller::InputGate,
        frame::{BgrFrame, PixelRect},
        time::SystemClock,
    },
    platform::{InputEvent, WindowId},
};
use std::{
    error::Error,
    sync::atomic::{AtomicBool, Ordering},
};
type TestResult = Result<(), Box<dyn Error>>;
struct Fake {
    window: Mutex<WindowInfo>,
    client: Mutex<PixelRect>,
    pointer: Mutex<[i32; 2]>,
    focused: AtomicBool,
    minimized: AtomicBool,
    fail_up: AtomicBool,
    events: Mutex<Vec<InputEvent>>,
}
impl CaptureBackend for Fake {
    fn windows(&self) -> Result<Vec<WindowInfo>, ControlError> {
        Ok(vec![
            self.window
                .lock()
                .map_err(|_| ControlError::Runtime("test window".into()))?
                .clone(),
        ])
    }
    fn monitors(&self) -> Result<Vec<PhysicalTarget>, ControlError> {
        Ok(vec![self.desktop()?])
    }
    fn client_box(&self, _: &WindowInfo) -> Result<PixelRect, ControlError> {
        Ok(*self
            .client
            .lock()
            .map_err(|_| ControlError::Runtime("test client".into()))?)
    }
    fn capture_window(&mut self, _: &WindowInfo) -> Result<Option<BgrFrame>, ControlError> {
        Err(ControlError::Runtime("unexpected capture".into()))
    }
    fn capture_screen(&mut self, _: PhysicalTarget) -> Result<BgrFrame, ControlError> {
        Err(ControlError::Runtime("unexpected capture".into()))
    }
}
impl InputSystem for Fake {
    fn focused(&self, _: WindowId) -> Result<bool, ControlError> {
        Ok(self.focused.load(Ordering::SeqCst))
    }
    fn minimized(&self, _: WindowId) -> Result<bool, ControlError> {
        Ok(self.minimized.load(Ordering::SeqCst))
    }
    fn bounds(&self, _: WindowId) -> Result<PhysicalTarget, ControlError> {
        Ok(self
            .window
            .lock()
            .map_err(|_| ControlError::Runtime("test window".into()))?
            .target)
    }
    fn desktop(&self) -> Result<PhysicalTarget, ControlError> {
        PhysicalTarget::new(-100, 0, 2000, 1200).map_err(|e| ControlError::Runtime(e.to_string()))
    }
    fn pointer(&self) -> Result<[i32; 2], ControlError> {
        Ok(*self
            .pointer
            .lock()
            .map_err(|_| ControlError::Runtime("test pointer".into()))?)
    }
    fn numlock_state(&self) -> Result<bool, ControlError> {
        Ok(false)
    }
    fn arrow_scan(&self, _: u32) -> Result<u16, ControlError> {
        Ok(0)
    }
    fn send(&self, e: InputEvent) -> Result<u32, ControlError> {
        self.events
            .lock()
            .map_err(|_| ControlError::Runtime("test events".into()))?
            .push(e);
        if matches!(e, InputEvent::Keyboard { up: true, .. }) && self.fail_up.load(Ordering::SeqCst)
        {
            return Ok(0);
        }
        Ok(1)
    }
}
type Fixture = (Arc<Fake>, Arc<WindowGuard<Fake>>);
fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let window = WindowInfo {
        id: WindowId::new(7)?,
        title: "Diablo II".into(),
        target: PhysicalTarget::new(10, 20, 200, 140)?,
    };
    let system = Arc::new(Fake {
        window: Mutex::new(window.clone()),
        client: Mutex::new(PixelRect::full(window.target.size())),
        pointer: Mutex::new([50, 60]),
        focused: AtomicBool::new(true),
        minimized: AtomicBool::new(false),
        fail_up: AtomicBool::new(false),
        events: Mutex::new(vec![]),
    });
    let guard = Arc::new(WindowGuard::new(
        system.clone(),
        CaptureSettings {
            window_title: Some("Diablo".into()),
            reference_layout: Some("reviewed.json".into()),
            ..Default::default()
        },
        window.clone(),
    ));
    guard.admit(&CapturedFrame {
        frame: Arc::new(BgrFrame::zeros(window.target.size())?),
        target: window.target,
        geometry: Some(Arc::new(FrameGeometry::create(window.target, None, None)?)),
        wall_captured_at: Some(1000.0),
    })?;
    Ok((system, guard))
}
#[test]
fn current_window_client_focus_and_minimization_qualify_coordinate_mapping() -> TestResult {
    let (s, g) = fixture()?;
    assert!(g.focused()?);
    assert_eq!(g.to_desktop([2.5, 3.5])?, [12, 24]);
    assert_eq!(g.from_desktop([12, 24])?, [2, 4]);
    s.minimized.store(true, Ordering::SeqCst);
    assert!(!g.focused()?);
    s.minimized.store(false, Ordering::SeqCst);
    s.focused.store(false, Ordering::SeqCst);
    assert!(g.to_desktop([1.0, 1.0]).is_err());
    s.focused.store(true, Ordering::SeqCst);
    *s.client.lock().map_err(|e| e.to_string())? = PixelRect::new(1, 0, 200, 140)?;
    assert!(!g.focused()?);
    *s.client.lock().map_err(|e| e.to_string())? = PixelRect::new(0, 0, 200, 140)?;
    s.window.lock().map_err(|e| e.to_string())?.target = PhysicalTarget::new(11, 20, 200, 140)?;
    assert!(!g.focused()?);
    assert!(s.events.lock().map_err(|e| e.to_string())?.is_empty());
    Ok(())
}
#[test]
fn recorded_requests_keep_failed_dispatch_and_allow_cleanup_after_guard_refusal() -> TestResult {
    let (s, g) = fixture()?;
    let gate = InputGate::default();
    let clock = Arc::new(SystemClock::default());
    let guard = g.clone();
    let native = ControllerFieldInput::new(s.clone(), gate.clone(), g.window().id, clock.clone())
        .with_guard(move || guard.focused());
    let recorded = RecordedInput::new(native, clock, Arc::new(|| 1000.0));
    recorded.move_pointer([50, 60])?;
    recorded.key_down_observed("f2", Some(InputObservation::new(7, 0.0, 0.0)?))?;
    s.client
        .lock()
        .map_err(|e| e.to_string())?
        .clone_from(&PixelRect::new(1, 0, 200, 140)?);
    assert!(recorded.key_down("f3").is_err());
    s.fail_up.store(true, Ordering::SeqCst);
    assert!(recorded.key_up("f2").is_err());
    s.fail_up.store(false, Ordering::SeqCst);
    gate.stop();
    recorded.key_up("f2")?;
    recorded.close()?;
    let rows = recorded.events()?;
    assert_eq!(
        rows[0]["pointer_evidence"]["desktop"],
        json!([-100, 0, 2000, 1200])
    );
    assert_eq!(rows[0]["pointer_evidence"]["observed"], json!([50, 60]));
    assert_eq!(rows[0]["sent"], true);
    assert_eq!(rows[1]["sent"], true);
    assert_eq!(rows[1]["observation"]["sequence"], 7);
    assert!(
        rows[1]["backend_returned_at"]
            .as_f64()
            .ok_or("return time missing")?
            >= rows[1]["at"].as_f64().ok_or("request time missing")?
    );
    assert_eq!(rows[2]["sent"], false);
    assert_eq!(rows[3]["sent"], false);
    assert_eq!(rows[4]["sent"], true);
    assert_eq!(rows[2]["args"], json!(["f3"]));
    assert_eq!(rows[4]["action"], "key_up");
    let events = s.events.lock().map_err(|e| e.to_string())?;
    assert_eq!(events.len(), 4);
    assert!(matches!(events[3], InputEvent::Keyboard { up: true, .. }));
    Ok(())
}
#[test]
fn extra_guard_errors_preserve_the_cause_and_never_dispatch() -> TestResult {
    let (s, g) = fixture()?;
    let native = ControllerFieldInput::new(
        s.clone(),
        InputGate::default(),
        g.window().id,
        Arc::new(SystemClock::default()),
    )
    .with_guard(|| Err(ControlError::Backend("current client query failed".into())));
    assert_eq!(
        native.key_down("f2").err().ok_or("error")?.to_string(),
        "current client query failed"
    );
    assert!(s.events.lock().map_err(|e| e.to_string())?.is_empty());
    native.key_up("f2")?;
    Ok(())
}
