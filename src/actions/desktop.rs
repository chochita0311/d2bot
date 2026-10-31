//! Action primitives over the existing safe Windows boundary. Timing and held
//! input ownership stay in ActionContext; this adapter adds no field-action waits.
use super::{
    ActionError,
    runtime::{ActionBackend, FocusOutcome},
};
use crate::{
    common::{
        capture::{CaptureSettings, ScreenCapture},
        controller::{keyboard_events, normalize_pointer},
        error::ControlError,
        frame_geometry::PhysicalTarget,
        hotkeys::{HotkeyAction, HotkeyBinding},
        realtime::CapturedFrame,
        time::{MonotonicClock, SystemClock},
        workers::Cancellation,
    },
    platform::{
        CaptureBackend, InputEvent, InputSystem, WindowInfo,
        windows::{HotkeyListener, Windows, focus_window},
    },
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
type Focus = Box<dyn FnMut(&WindowInfo) -> Result<bool, ControlError> + Send>;
pub struct DesktopActionBackend<C: CaptureBackend, I: InputSystem> {
    capture: ScreenCapture<C>,
    system: Arc<I>,
    focus: Focus,
    clock: Arc<dyn MonotonicClock>,
    random: StdRng,
    hotkey: Option<HotkeyListener>,
    cancellation: Cancellation,
    town_window: Option<WindowInfo>,
}
impl<C: CaptureBackend, I: InputSystem> DesktopActionBackend<C, I> {
    pub fn new(
        capture: ScreenCapture<C>,
        system: Arc<I>,
        focus: impl FnMut(&WindowInfo) -> Result<bool, ControlError> + Send + 'static,
    ) -> Result<Self, ActionError> {
        let random = StdRng::try_from_os_rng().map_err(|e| {
            ActionError::runtime(format!("action random initialization failed: {e}"))
        })?;
        Ok(Self {
            capture,
            system,
            focus: Box::new(focus),
            clock: Arc::new(SystemClock::default()),
            random,
            hotkey: None,
            cancellation: Cancellation::default(),
            town_window: None,
        })
    }
    /// 입장·필드·마감에서 실제 요청 시각의 기준을 하나로 유지한다.
    pub fn with_clock(mut self, clock: Arc<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }
    fn send(&self, event: InputEvent) -> Result<(), ActionError> {
        if self.system.send(event)? != 1 {
            return Err(ActionError::runtime(
                "SendInput did not insert the requested event",
            ));
        }
        Ok(())
    }
}
impl DesktopActionBackend<Windows, Windows> {
    pub fn windows(settings: CaptureSettings, root: &Path) -> Result<Self, ActionError> {
        let system = Windows::new()?;
        let capture = ScreenCapture::new(system, settings, root)?;
        Self::new(capture, Arc::new(system), |window| focus_window(window.id))
    }
}
impl<C: CaptureBackend, I: InputSystem> ActionBackend for DesktopActionBackend<C, I> {
    fn capture(&mut self) -> Result<CapturedFrame, ActionError> {
        Ok(self.capture.grab()?)
    }
    fn target(&self) -> PhysicalTarget {
        self.capture.target()
    }
    fn geometry_is_current(&self) -> Result<bool, ActionError> {
        if let Some(geometry) = self.capture.geometry() {
            Ok(self.capture.current_target()? == geometry.target()
                && self.capture.current_client_box()? == geometry.source_client())
        } else {
            Ok(true)
        }
    }
    fn frame_to_desktop(&self, point: [f64; 2]) -> Result<[f64; 2], ActionError> {
        if let Some(geometry) = self.capture.geometry() {
            if !self.geometry_is_current()? {
                return Err(ActionError::runtime(
                    "window geometry changed after capture",
                ));
            }
            Ok(geometry
                .to_desktop(point)
                .map_err(|e| ActionError::runtime(e.to_string()))?
                .map(|n| n as f64))
        } else {
            let t = self.capture.target();
            Ok([point[0] + f64::from(t.left), point[1] + f64::from(t.top)])
        }
    }
    fn desktop_to_frame(&self, point: [f64; 2]) -> Result<[i64; 2], ActionError> {
        let point = if let Some(geometry) = self.capture.geometry() {
            if !self.geometry_is_current()? {
                return Err(ActionError::runtime(
                    "window geometry changed after capture",
                ));
            }
            geometry
                .from_desktop(point)
                .map_err(|e| ActionError::runtime(e.to_string()))?
        } else {
            let t = self.capture.target();
            [point[0] - f64::from(t.left), point[1] - f64::from(t.top)]
        };
        Ok(point.map(|n| n.round_ties_even() as i64))
    }
    fn focus_configured(&mut self) -> Result<FocusOutcome, ActionError> {
        let Some(window) = self.capture.configured_window()? else {
            return Ok(FocusOutcome::Unavailable);
        };
        Ok(if (self.focus)(&window)? {
            FocusOutcome::Focused(window.target)
        } else {
            FocusOutcome::Failed
        })
    }
    fn pointer(&self) -> Result<[i64; 2], ActionError> {
        Ok(self.system.pointer()?.map(i64::from))
    }
    fn move_desktop(&mut self, point: [i64; 2]) -> Result<(), ActionError> {
        let point = point.map(i32::try_from);
        let point = [
            point[0]
                .as_ref()
                .copied()
                .map_err(|_| ActionError::runtime("pointer x exceeds desktop range"))?,
            point[1]
                .as_ref()
                .copied()
                .map_err(|_| ActionError::runtime("pointer y exceeds desktop range"))?,
        ];
        let [dx, dy] = normalize_pointer(self.system.desktop()?, point)?;
        self.send(InputEvent::Mouse {
            dx,
            dy,
            flags: 0x8000 | 0x4000 | 0x0001,
        })
    }
    fn key(&mut self, key: &str, up: bool) -> Result<(), ActionError> {
        let mut errors = Vec::new();
        for event in keyboard_events(self.system.as_ref(), key, up)? {
            if let Err(error) = self.send(event) {
                errors.push(error.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ActionError::runtime(errors.join("; ")))
        }
    }
    fn click(&mut self, button: &str) -> Result<(), ActionError> {
        let flags = match button {
            "left" => 6,
            "right" => 24,
            _ => return Err(ActionError::runtime("unsupported action mouse button")),
        };
        self.send(InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags,
        })
    }
    fn mouse_button(&mut self, button: &str, up: bool) -> Result<(), ActionError> {
        let flags = match (button, up) {
            ("left", false) => 2,
            ("left", true) => 4,
            ("right", false) => 8,
            ("right", true) => 16,
            _ => return Err(ActionError::runtime("unsupported action mouse button")),
        };
        self.send(InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags,
        })
    }
    fn wall_now(&self) -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(f64::NAN, |d| d.as_secs_f64())
    }
    fn monotonic_now(&self) -> f64 {
        self.clock.now()
    }
    fn sleep(&mut self, duration: Duration) -> Result<(), ActionError> {
        std::thread::sleep(duration);
        Ok(())
    }
    fn wait(&mut self, duration: Duration) -> Result<(), ActionError> {
        self.cancellation.wait(duration)?;
        Ok(())
    }
    fn random_integer(&mut self, low: i64, high: i64) -> Result<i64, ActionError> {
        if low > high {
            return Err(ActionError::runtime("empty action integer range"));
        }
        Ok(self.random.random_range(low..=high))
    }
    fn random_float(&mut self, low: f64, high: f64) -> Result<f64, ActionError> {
        if !low.is_finite() || !high.is_finite() || low > high || !(high - low).is_finite() {
            return Err(ActionError::runtime("invalid action random range"));
        }
        if low == high {
            return Ok(low);
        }
        Ok(self.random.random_range(low..=high))
    }
    fn bind_stop(&mut self, cancellation: Cancellation) -> Result<(), ActionError> {
        if self.hotkey.is_some() {
            return Err(ActionError::runtime(
                "action stop listener is already bound",
            ));
        }
        self.cancellation = cancellation.clone();
        self.hotkey = Some(HotkeyListener::start(
            vec![HotkeyBinding::parse("f10", HotkeyAction::Stop)?],
            Arc::new(move |_| {
                cancellation.cancel();
                Ok(())
            }),
        )?);
        Ok(())
    }
    fn unbind_stop(&mut self) -> Result<(), ActionError> {
        if let Some(listener) = &mut self.hotkey {
            listener.stop(Duration::from_secs(1))?;
        }
        self.hotkey = None;
        Ok(())
    }
    fn close_capture(&mut self) -> Result<(), ActionError> {
        Ok(self.capture.close()?)
    }
    fn start_town_guard(&mut self) -> Result<(), ActionError> {
        self.town_window =
            Some(self.capture.configured_window()?.ok_or_else(|| {
                ActionError::runtime("Town loop requires a named Diablo window.")
            })?);
        Ok(())
    }
    fn town_guard(&self) -> Result<(), ActionError> {
        if let Some(expected) = &self.town_window {
            let current = self.capture.configured_window()?;
            if current
                .as_ref()
                .is_none_or(|c| c.id != expected.id || c.target != expected.target)
            {
                return Err(ActionError::runtime(
                    "Game window moved or resized; town loop stopped before further input.",
                ));
            }
            if !self.system.focused(expected.id)? {
                return Err(ActionError::runtime(
                    "Game focus was lost; town loop stopped before further input.",
                ));
            }
        }
        Ok(())
    }
    fn town_geometry(&self) -> Result<(usize, PhysicalTarget), ActionError> {
        self.town_window
            .as_ref()
            .map(|w| (w.id.value(), w.target))
            .ok_or_else(|| ActionError::runtime("town geometry is unavailable"))
    }
}
