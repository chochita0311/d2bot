use crate::{
    common::{
        capture::{CaptureSettings, ScreenCapture},
        config::BotConfig,
        controller::{ControllerFieldInput, InputGate, contains},
        error::ControlError,
        field_control::{DesktopPoint, FieldInputBackend},
        field_preparation::PreparationEnvironment,
        frame::BgrFrame,
        survival_vision::{HudObserverOptions, HudReading, SurvivalHudObserver},
        time::MonotonicClock,
        workers::Cancellation,
    },
    platform::windows::Windows,
    runs::{
        RunError,
        summoner::{
            main_run::native::{RecordedInput, WindowGuard},
            navigation_execution::{LandingEnvironment, StepEnvironment},
        },
    },
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub fn wall_now() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(v) => v.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    }
}
pub enum ProbeInput {
    Live(RecordedInput<Windows>),
    Dry(DryProbeInput),
}
pub struct DryProbeInput {
    window: Arc<WindowGuard<Windows>>,
    gate: InputGate,
    clock: Arc<dyn MonotonicClock>,
    events: Mutex<Vec<Value>>,
}
impl DryProbeInput {
    fn ready(&self) -> Result<(), ControlError> {
        if self.gate.interrupted() || !self.window.focused()? {
            Err(ControlError::Runtime(
                "field input interrupted or game unfocused".into(),
            ))
        } else {
            Ok(())
        }
    }
    fn send(
        &self,
        action: &str,
        args: Value,
        check: impl FnOnce() -> Result<(), ControlError>,
    ) -> Result<(), ControlError> {
        let index = {
            let mut events = self
                .events
                .lock()
                .map_err(|_| ControlError::Runtime("probe input events poisoned".into()))?;
            let n = events.len();
            events.push(json!({"action":action,"args":args,"at":self.clock.now(),"wall_at":wall_now(),"sent":false}));
            n
        };
        check()?;
        self.events
            .lock()
            .map_err(|_| ControlError::Runtime("probe input events poisoned".into()))?[index]["sent"] =
            json!(true);
        Ok(())
    }
}
impl ProbeInput {
    pub fn events(&self) -> Result<Value, ControlError> {
        match self {
            Self::Live(v) => v.events(),
            Self::Dry(v) => v
                .events
                .lock()
                .map(|v| json!(*v))
                .map_err(|_| ControlError::Runtime("probe input events poisoned".into())),
        }
    }
    pub fn close(&self) -> Result<(), ControlError> {
        match self {
            Self::Live(v) => v.close(),
            Self::Dry(v) => {
                v.gate.stop();
                Ok(())
            }
        }
    }
}
impl FieldInputBackend for ProbeInput {
    fn is_dry_run(&self) -> bool {
        matches!(self, Self::Dry(_))
    }
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        match self {
            Self::Live(v) => v.key_down(key),
            Self::Dry(v) => v.send("key_down", json!([key]), || v.ready()),
        }
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        match self {
            Self::Live(v) => v.key_up(key),
            Self::Dry(v) => v.send("key_up", json!([key]), || Ok(())),
        }
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        match self {
            Self::Live(v) => v.press(key),
            Self::Dry(v) => v.send("press", json!([key]), || v.ready()),
        }
    }
    fn move_pointer(&self, p: DesktopPoint) -> Result<(), ControlError> {
        match self {
            Self::Live(v) => v.move_pointer(p),
            Self::Dry(v) => v.send("move", json!(p), || {
                v.ready()?;
                let p = [i32::try_from(p[0]), i32::try_from(p[1])];
                match p {
                    [Ok(x), Ok(y)] if contains(v.window.target(), [x, y]) => Ok(()),
                    _ => Err(ControlError::Runtime(
                        "field aim outside current game window".into(),
                    )),
                }
            }),
        }
    }
    fn supports_pointer_confirmation(&self) -> bool {
        true
    }
    fn pointer_matches(&self, p: DesktopPoint) -> Result<bool, ControlError> {
        match self {
            Self::Live(v) => v.pointer_matches(p),
            Self::Dry(v) => {
                v.ready()?;
                Ok(true)
            }
        }
    }
}
pub struct NativeProbe {
    pub capture: ScreenCapture<Windows>,
    pub window: Arc<WindowGuard<Windows>>,
    pub input: Arc<ProbeInput>,
    pub hud: SurvivalHudObserver,
    pub clock: Arc<dyn MonotonicClock>,
    pub stop: Cancellation,
    pub gate: InputGate,
}
impl NativeProbe {
    pub fn load(
        root: &Path,
        config: &BotConfig,
        live: bool,
        clock: Arc<dyn MonotonicClock>,
        stop: Cancellation,
        gate: InputGate,
    ) -> Result<Self, RunError> {
        let hud = SurvivalHudObserver::load(
            root.join("assets/ui/survival"),
            HudObserverOptions::default(),
        )?;
        let mut settings = CaptureSettings::from_config(config)?;
        settings.window_title = Some("Diablo II: Resurrected".into());
        settings.window_title_mode = "exact".into();
        settings.capture_backend = "window".into();
        let system = Arc::new(Windows::new()?);
        let mut capture = ScreenCapture::new(*system, settings.clone(), root)?;
        let window = capture.configured_window().and_then(|v| {
            v.ok_or_else(|| ControlError::Runtime("visible game window required".into()))
        });
        let window = match window {
            Ok(v) => v,
            Err(e) => {
                return match capture.close() {
                    Ok(()) => Err(e.into()),
                    Err(c) => Err(RunError::from(e).with_cleanup(c.into())),
                };
            }
        };
        let guard = Arc::new(WindowGuard::new(system.clone(), settings, window));
        let input = if live {
            let current = guard.clone();
            let controller =
                ControllerFieldInput::new(system, gate.clone(), guard.window().id, clock.clone())
                    .with_guard(move || current.focused());
            ProbeInput::Live(RecordedInput::new(
                controller,
                clock.clone(),
                Arc::new(wall_now),
            ))
        } else {
            ProbeInput::Dry(DryProbeInput {
                window: guard.clone(),
                gate: gate.clone(),
                clock: clock.clone(),
                events: Mutex::new(vec![]),
            })
        };
        Ok(Self {
            capture,
            window: guard,
            input: Arc::new(input),
            hud,
            clock,
            stop,
            gate,
        })
    }
    pub fn grab(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        let frame = self.capture.grab()?;
        self.window.admit(&frame)?;
        Ok(frame.frame)
    }
    pub fn to_desktop(&self, point: [f64; 2]) -> Result<DesktopPoint, RunError> {
        let geometry = self
            .capture
            .geometry()
            .ok_or_else(|| RunError::Runtime("current frame geometry required".into()))?;
        if self.capture.current_target()? != geometry.target()
            || self.capture.current_client_box()? != geometry.source_client()
        {
            return Err(RunError::Runtime(
                "window geometry changed after capture".into(),
            ));
        }
        geometry
            .to_desktop(point)
            .map_err(|e| RunError::Runtime(e.to_string()))
    }
    pub fn close(&mut self) -> Result<(), RunError> {
        self.stop.cancel();
        self.gate.stop();
        let input = self.input.close().map_err(RunError::from);
        let capture = self.capture.close().map_err(RunError::from);
        match (input, capture) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
            (Err(e), Err(c)) => Err(e.with_cleanup(c)),
        }
    }
}
impl PreparationEnvironment for NativeProbe {
    type Error = RunError;
    fn clock(&self) -> f64 {
        self.clock.now()
    }
    fn wall_clock(&self) -> f64 {
        wall_now()
    }
    fn stopped(&self) -> bool {
        self.stop.is_cancelled()
    }
    fn focused(&self) -> Result<bool, RunError> {
        Ok(self.window.focused()?)
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.grab()
    }
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        Ok(self.hud.observe(frame.view())?)
    }
    fn press(&mut self, key: &str) -> Result<(), RunError> {
        Ok(self.input.press(key)?)
    }
    fn wait(&mut self, seconds: f64) -> Result<(), RunError> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| RunError::Runtime("probe wait exceeds runtime range".into()))?;
        self.stop.wait(duration)?;
        Ok(())
    }
}
impl StepEnvironment for NativeProbe {
    fn clock(&self) -> f64 {
        self.clock.now()
    }
    fn stopped(&self) -> bool {
        self.stop.is_cancelled()
    }
    fn focused(&self) -> Result<bool, ControlError> {
        self.window.focused()
    }
    fn wait(&self, seconds: f64) -> Result<(), ControlError> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| ControlError::Runtime("probe wait exceeds runtime range".into()))?;
        self.stop.wait(duration)?;
        Ok(())
    }
}
impl LandingEnvironment for NativeProbe {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.grab()
    }
    fn hud(&mut self, frame: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        Ok(self.hud.observe(frame.view())?)
    }
}
impl Drop for NativeProbe {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.close()));
    }
}
