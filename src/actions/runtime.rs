//! Owned action effects with injectable desktop/capture/time/random boundaries.
//! Native dispatch is implemented separately; this module is safe Rust.
use super::{ActionError, ActionEvent, EventLevel};
use crate::common::{
    frame_geometry::PhysicalTarget, realtime::CapturedFrame, workers::Cancellation,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
struct EventBuffer {
    queue: VecDeque<ActionEvent>,
    dropped: usize,
}
#[derive(Clone, Default)]
pub struct ActionEvents(Arc<Mutex<EventBuffer>>);
impl ActionEvents {
    pub fn push(&self, event: ActionEvent) {
        let mut buffer = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if buffer.queue.len() == 256 {
            buffer.queue.pop_front();
            buffer.dropped = buffer.dropped.saturating_add(1);
        }
        buffer.queue.push_back(event);
    }
    pub fn drain(&self) -> Vec<ActionEvent> {
        let mut buffer = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let mut events = Vec::new();
        if buffer.dropped > 0 {
            events.push(ActionEvent {
                level: EventLevel::Warning,
                message: format!(
                    "Action log omitted {} older event(s); the latest 256 were retained.",
                    buffer.dropped
                ),
            });
            buffer.dropped = 0;
        }
        events.extend(buffer.queue.drain(..));
        events
    }
}

pub enum FocusOutcome {
    Unavailable,
    Focused(PhysicalTarget),
    Failed,
}
/// Primitive calls have no implicit PyDirectInput pause. ActionContext applies
/// the installed reference's 100ms after successful click/key calls explicitly.
pub trait ActionBackend: Send {
    fn capture(&mut self) -> Result<CapturedFrame, ActionError>;
    fn target(&self) -> PhysicalTarget;
    fn geometry_is_current(&self) -> Result<bool, ActionError>;
    fn frame_to_desktop(&self, point: [f64; 2]) -> Result<[f64; 2], ActionError>;
    fn desktop_to_frame(&self, point: [f64; 2]) -> Result<[i64; 2], ActionError>;
    fn focus_configured(&mut self) -> Result<FocusOutcome, ActionError>;
    fn pointer(&self) -> Result<[i64; 2], ActionError>;
    fn move_desktop(&mut self, point: [i64; 2]) -> Result<(), ActionError>;
    fn key(&mut self, key: &str, up: bool) -> Result<(), ActionError>;
    fn click(&mut self, button: &str) -> Result<(), ActionError>;
    fn mouse_button(&mut self, button: &str, up: bool) -> Result<(), ActionError>;
    fn wall_now(&self) -> f64;
    fn monotonic_now(&self) -> f64;
    fn sleep(&mut self, duration: Duration) -> Result<(), ActionError>;
    fn wait(&mut self, duration: Duration) -> Result<(), ActionError> {
        self.sleep(duration)
    }
    fn start_town_guard(&mut self) -> Result<(), ActionError> {
        Err(ActionError::runtime(
            "Town loop requires a named Diablo window.",
        ))
    }
    fn town_guard(&self) -> Result<(), ActionError> {
        Ok(())
    }
    fn town_geometry(&self) -> Result<(usize, PhysicalTarget), ActionError> {
        Err(ActionError::runtime("town geometry is unavailable"))
    }
    fn random_integer(&mut self, low: i64, high: i64) -> Result<i64, ActionError>;
    fn random_float(&mut self, low: f64, high: f64) -> Result<f64, ActionError>;
    fn bind_stop(&mut self, cancellation: Cancellation) -> Result<(), ActionError>;
    fn unbind_stop(&mut self) -> Result<(), ActionError>;
    fn close_capture(&mut self) -> Result<(), ActionError>;
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PointerStyle {
    Gems,
    Loot,
    Lifecycle,
}
impl PointerStyle {
    fn steps(self) -> [i64; 2] {
        match self {
            Self::Gems => [2, 3],
            _ => [2, 4],
        }
    }
    fn sleep(self) -> [f64; 2] {
        match self {
            Self::Gems => [0.016, 0.038],
            _ => [0.015, 0.035],
        }
    }
}
pub struct ActionContext<B: ActionBackend> {
    backend: B,
    cancellation: Cancellation,
    style: PointerStyle,
    last_pointer: Option<[i64; 2]>,
    keys: Vec<String>,
    buttons: Vec<String>,
    hotkey_owned: bool,
    closing: bool,
    closed: bool,
    events: ActionEvents,
    stop_reason: Option<String>,
}
impl<B: ActionBackend> ActionContext<B> {
    pub fn new(backend: B, cancellation: Cancellation, style: PointerStyle) -> Self {
        Self {
            backend,
            cancellation,
            style,
            last_pointer: None,
            keys: Vec::new(),
            buttons: Vec::new(),
            hotkey_owned: false,
            closing: false,
            closed: false,
            events: ActionEvents::default(),
            stop_reason: None,
        }
    }
    pub fn capture(&mut self) -> Result<CapturedFrame, ActionError> {
        self.ensure_open()?;
        self.backend.capture()
    }
    pub fn cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
    pub fn emit(&mut self, level: EventLevel, message: impl Into<String>) {
        self.events.push(ActionEvent {
            level,
            message: message.into(),
        });
    }
    pub fn take_events(&mut self) -> Vec<ActionEvent> {
        self.events.drain()
    }
    pub fn events_handle(&self) -> ActionEvents {
        self.events.clone()
    }
    pub fn with_events(mut self, events: ActionEvents) -> Self {
        self.events = events;
        self
    }
    pub fn now(&self) -> Result<f64, ActionError> {
        let now = self.backend.wall_now();
        if !now.is_finite() {
            Err(ActionError::runtime("finite action clock required"))
        } else {
            Ok(now)
        }
    }
    pub fn monotonic_now(&self) -> Result<f64, ActionError> {
        let now = self.backend.monotonic_now();
        if now.is_finite() {
            Ok(now)
        } else {
            Err(ActionError::runtime(
                "finite monotonic action clock required",
            ))
        }
    }
    pub fn ensure_input_allowed(&mut self) -> Result<(), ActionError> {
        self.ensure_open()?;
        if self.user_interrupted()? {
            return Err(ActionError::runtime("stopped by user interference"));
        }
        self.backend.town_guard()
    }
    pub fn start_town_guard(&mut self) -> Result<(), ActionError> {
        self.backend.start_town_guard()
    }
    pub fn town_guard(&self) -> Result<(), ActionError> {
        self.backend.town_guard()
    }
    pub fn town_geometry(&self) -> Result<(usize, PhysicalTarget), ActionError> {
        self.backend.town_geometry()
    }
    pub fn stop_reason(&self) -> Option<String> {
        self.stop_reason.clone().or_else(|| {
            self.cancelled()
                .then(|| "stop requested (F10 or session request)".into())
        })
    }
    pub fn geometry_is_current(&self) -> Result<bool, ActionError> {
        self.backend.geometry_is_current()
    }
    pub fn pointer(&self) -> Result<[i64; 2], ActionError> {
        self.backend.pointer()
    }
    pub fn last_pointer(&self) -> Option<[i64; 2]> {
        self.last_pointer
    }
    pub fn accept_pointer(&mut self, point: [i64; 2]) {
        self.last_pointer = Some(point);
    }
    /// 같은 실행 주체를 새 마을 복귀 단계로 넘길 때 이전 커서 기준을 지운다.
    pub fn reset_pointer(&mut self) {
        self.last_pointer = None;
    }
    pub fn wait(&mut self, seconds: f64) -> Result<(), ActionError> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| ActionError::runtime("action wait outside duration range"))?;
        self.backend.wait(duration)
    }
    pub fn release_town_mouse(&mut self) -> Result<(), ActionError> {
        if self.buttons.iter().any(|b| b == "left") {
            self.mouse_button_with_pause("left", true, false)?;
        }
        Ok(())
    }
    pub fn lifecycle_click_frame(&mut self, point: [f64; 2]) -> Result<(), ActionError> {
        self.ensure_input_allowed()?;
        let point = self.frame_point(point)?;
        let point = self.jitter(point)?;
        self.move_absolute(point)?;
        self.sleep_range([0.05, 0.09])?;
        self.ensure_input_allowed()?;
        self.click("left")?;
        self.last_pointer = Some(point.map(|n| n.trunc() as i64));
        self.sleep_range([0.16, 0.30])
    }
    pub fn lifecycle_press(&mut self, key: &str) -> Result<(), ActionError> {
        self.ensure_input_allowed()?;
        self.press(key)?;
        self.sleep_range([0.16, 0.30])
    }
    pub fn move_town_absolute(&mut self, point: [f64; 2]) -> Result<(), ActionError> {
        self.ensure_open()?;
        if !self.backend.geometry_is_current()? {
            return Err(ActionError::runtime(
                "window geometry changed after capture",
            ));
        }
        let target = self.backend.target();
        if target.size().width() < 4
            || target.size().height() < 4
            || point.iter().any(|n| !n.is_finite())
        {
            return Err(ActionError::runtime("invalid town pointer target"));
        }
        let point = [
            point[0]
                .clamp(
                    f64::from(target.left) + 2.0,
                    f64::from(target.left) + target.size().width() as f64 - 2.0,
                )
                .trunc() as i64,
            point[1]
                .clamp(
                    f64::from(target.top) + 2.0,
                    f64::from(target.top) + target.size().height() as f64 - 2.0,
                )
                .trunc() as i64,
        ];
        if self.cancelled() {
            return Ok(());
        }
        self.backend.move_desktop(point)?;
        let deadline = self.monotonic_now()? + 0.08;
        loop {
            if self.cancelled() {
                return Err(ActionError::runtime(
                    "stopped before pointer movement completed",
                ));
            }
            self.backend.town_guard()?;
            let actual = self.backend.pointer()?;
            if actual[0].abs_diff(point[0]) <= 3 && actual[1].abs_diff(point[1]) <= 3 {
                self.last_pointer = Some(actual);
                return Ok(());
            }
            let remaining = deadline - self.monotonic_now()?;
            if remaining <= 0.0 {
                return Err(ActionError::runtime(format!(
                    "Pointer mapping mismatch: requested ({}, {}), observed ({}, {})",
                    point[0], point[1], actual[0], actual[1]
                )));
            }
            self.wait(remaining.min(0.005))?;
        }
    }
    fn random_float(&mut self, range: [f64; 2]) -> Result<f64, ActionError> {
        let value = self.backend.random_float(range[0], range[1])?;
        if !value.is_finite() || value < range[0] || value > range[1] {
            return Err(ActionError::runtime("action random value outside range"));
        }
        Ok(value)
    }
    fn random_integer(&mut self, range: [i64; 2]) -> Result<i64, ActionError> {
        let value = self.backend.random_integer(range[0], range[1])?;
        if value < range[0] || value > range[1] {
            return Err(ActionError::runtime("action random integer outside range"));
        }
        Ok(value)
    }
    pub fn raw_sleep(&mut self, seconds: f64) -> Result<(), ActionError> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|_| ActionError::runtime("action delay outside duration range"))?;
        self.backend.sleep(duration)
    }
    pub fn sleep_range(&mut self, range: [f64; 2]) -> Result<(), ActionError> {
        let duration = self.random_float(range)?;
        let end = self.now()? + duration;
        if !end.is_finite() {
            return Err(ActionError::runtime("action deadline overflow"));
        }
        while self.now()? < end {
            if self.cancelled() {
                return Ok(());
            }
            self.raw_sleep(0.01)?;
        }
        Ok(())
    }
    pub fn prepare(&mut self) -> Result<(), ActionError> {
        self.ensure_open()?;
        self.hotkey_owned = true;
        self.backend.bind_stop(self.cancellation.clone())?;
        self.focus_configured()
    }
    pub fn focus_configured(&mut self) -> Result<(), ActionError> {
        self.ensure_open()?;
        match self.backend.focus_configured()? {
            FocusOutcome::Unavailable => {}
            FocusOutcome::Failed => {
                return Err(ActionError::runtime(
                    "Could not bring the Diablo window to the foreground from the control panel.",
                ));
            }
            FocusOutcome::Focused(window) => {
                if self.style == PointerStyle::Lifecycle {
                    self.last_pointer = Some(self.backend.pointer()?);
                    return Ok(());
                }
                let center = [
                    i64::from(window.left) + (window.size().width() / 2) as i64,
                    i64::from(window.top) + (window.size().height() / 2) as i64,
                ];
                self.move_absolute(center.map(|n| n as f64))?;
                self.last_pointer = Some(center);
                self.sleep_range([0.12, 0.20])?;
            }
        }
        Ok(())
    }
    pub fn frame_point(&self, point: [f64; 2]) -> Result<[f64; 2], ActionError> {
        self.backend.frame_to_desktop(point)
    }
    pub fn desktop_point_to_frame(&self, point: [f64; 2]) -> Result<[i64; 2], ActionError> {
        self.backend.desktop_to_frame(point)
    }
    pub fn user_interrupted(&mut self) -> Result<bool, ActionError> {
        if self.cancelled() {
            return Ok(true);
        }
        if let Some(last) = self.last_pointer {
            let current = self.backend.pointer()?;
            if current[0].abs_diff(last[0]) > 80 || current[1].abs_diff(last[1]) > 80 {
                if self.style == PointerStyle::Lifecycle {
                    let reason = format!(
                        "pointer changed outside planned movement: expected ({}, {}), observed ({}, {})",
                        last[0], last[1], current[0], current[1]
                    );
                    self.emit(EventLevel::Info, &reason);
                    self.stop_reason = Some(reason);
                }
                self.cancellation.cancel();
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn before_down(&self) -> Result<(), ActionError> {
        self.ensure_open()?;
        if self.cancelled() {
            return Err(ActionError::runtime("stopped by user interference"));
        }
        if self.backend.pointer()? == [0, 0] {
            return Err(ActionError::runtime(
                "PyDirectInput fail-safe triggered from mouse moving to a corner of the screen.",
            ));
        }
        Ok(())
    }
    pub fn move_absolute(&mut self, point: [f64; 2]) -> Result<(), ActionError> {
        self.ensure_open()?;
        if point.iter().any(|n| !n.is_finite()) {
            return Err(ActionError::runtime("finite action target required"));
        }
        if !self.backend.geometry_is_current()? {
            return Err(ActionError::runtime(
                "window geometry changed after capture",
            ));
        }
        let target = self.backend.target();
        if target.size().width() < 4 || target.size().height() < 4 {
            return Err(ActionError::runtime(
                "action target is too small for pointer inset",
            ));
        }
        let x = point[0].clamp(
            f64::from(target.left) + 2.0,
            f64::from(target.left) + target.size().width() as f64 - 2.0,
        );
        let y = point[1].clamp(
            f64::from(target.top) + 2.0,
            f64::from(target.top) + target.size().height() as f64 - 2.0,
        );
        let current = self.backend.pointer()?;
        let steps = self.random_integer(self.style.steps())?;
        for step in 1..=steps {
            if self.cancelled() {
                return Ok(());
            }
            let ratio = step as f64 / steps as f64;
            let bend = self.random_integer([-8, 8])? as f64;
            let next = [
                (current[0] as f64 + (x - current[0] as f64) * ratio).trunc() as i64,
                (current[1] as f64
                    + (y - current[1] as f64) * ratio
                    + bend * (1.0 - (0.5 - ratio).abs() * 2.0))
                    .trunc() as i64,
            ];
            self.backend.move_desktop(next)?;
            self.last_pointer = Some(next);
            let delay = self.random_float(self.style.sleep())?;
            self.raw_sleep(delay)?;
        }
        Ok(())
    }
    fn jitter(&mut self, point: [f64; 2]) -> Result<[f64; 2], ActionError> {
        Ok([
            point[0] + self.random_integer([-3, 3])? as f64,
            point[1] + self.random_integer([-3, 3])? as f64,
        ])
    }
    pub fn key_down(&mut self, key: &str) -> Result<(), ActionError> {
        self.before_down()?;
        if !self.keys.iter().any(|held| held == key) {
            self.keys.push(key.to_owned());
        }
        self.backend.key(key, false)?;
        self.raw_sleep(0.1)
    }
    pub fn key_up(&mut self, key: &str) -> Result<(), ActionError> {
        // Cleanup bypasses stop/failsafe and retains failed releases for retry.
        self.backend.key(key, true)?;
        self.keys.retain(|held| held != key);
        self.raw_sleep(0.1)
    }
    pub fn press(&mut self, key: &str) -> Result<(), ActionError> {
        self.press_with_pause(key, true)
    }
    pub fn press_with_pause(&mut self, key: &str, pause: bool) -> Result<(), ActionError> {
        let result = self.key_down(key);
        let released = if self.keys.iter().any(|held| held == key) {
            self.key_up(key)
        } else {
            Ok(())
        };
        combine(result, released)?;
        // Installed press() calls decorated keyDown/keyUp internally, then its
        // own wrapper adds one more PAUSE unless _pause=False was requested.
        if pause {
            self.raw_sleep(0.1)?;
        }
        Ok(())
    }
    pub fn click_absolute(
        &mut self,
        point: [f64; 2],
        button: &str,
        modifiers: bool,
    ) -> Result<(), ActionError> {
        let point = self.jitter(point)?;
        self.move_absolute(point)?;
        self.sleep_range([0.05, 0.09])?;
        if modifiers {
            let result = self
                .key_down("ctrl")
                .and_then(|_| self.key_down("shift"))
                .and_then(|_| self.click(button));
            let shift = if self.keys.iter().any(|key| key == "shift") {
                self.key_up("shift")
            } else {
                Ok(())
            };
            let ctrl = if self.keys.iter().any(|key| key == "ctrl") {
                self.key_up("ctrl")
            } else {
                Ok(())
            };
            combine(combine(result, shift), ctrl)?;
        } else {
            self.click(button)?;
        }
        self.last_pointer = Some(point.map(|n| n.trunc() as i64));
        Ok(())
    }
    pub fn click_frame(
        &mut self,
        point: [f64; 2],
        button: &str,
        modifiers: bool,
    ) -> Result<(), ActionError> {
        let target = self.frame_point(point)?;
        self.click_absolute(target, button, modifiers)
    }
    pub fn click(&mut self, button: &str) -> Result<(), ActionError> {
        self.click_with_pause(button, true)
    }
    pub fn click_with_pause(&mut self, button: &str, pause: bool) -> Result<(), ActionError> {
        self.before_down()?;
        self.backend.click(button)?;
        if pause {
            self.raw_sleep(0.1)?;
        }
        Ok(())
    }
    pub fn mouse_button(&mut self, button: &str, up: bool) -> Result<(), ActionError> {
        self.mouse_button_with_pause(button, up, true)
    }
    pub fn mouse_button_with_pause(
        &mut self,
        button: &str,
        up: bool,
        pause: bool,
    ) -> Result<(), ActionError> {
        if !up {
            self.before_down()?;
            if !self.buttons.iter().any(|held| held == button) {
                self.buttons.push(button.to_owned());
            }
        }
        self.backend.mouse_button(button, up)?;
        if up {
            self.buttons.retain(|held| held != button);
        }
        if pause {
            self.raw_sleep(0.1)?;
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<(), ActionError> {
        if self.closed {
            return Ok(());
        }
        self.closing = true;
        let mut errors = Vec::new();
        for key in self.keys.clone().into_iter().rev() {
            if let Err(error) = cleanup_attempt(|| self.key_up(&key)) {
                errors.push(error.to_string());
            }
        }
        for button in self.buttons.clone().into_iter().rev() {
            if let Err(error) =
                cleanup_attempt(|| self.mouse_button_with_pause(&button, true, false))
            {
                errors.push(error.to_string());
            }
        }
        if self.hotkey_owned {
            match cleanup_attempt(|| self.backend.unbind_stop()) {
                Ok(()) => self.hotkey_owned = false,
                Err(error) => errors.push(error.to_string()),
            }
        }
        if let Err(error) = cleanup_attempt(|| self.backend.close_capture()) {
            errors.push(error.to_string());
        }
        self.closed = errors.is_empty();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ActionError::runtime(format!(
                "action cleanup failed: {}",
                errors.join("; ")
            )))
        }
    }
    pub fn finish<T>(&mut self, result: Result<T, ActionError>) -> Result<T, ActionError> {
        let cleanup = self.close();
        merge(result, cleanup)
    }
    fn ensure_open(&self) -> Result<(), ActionError> {
        if self.closing || self.closed {
            Err(ActionError::runtime("action context is closed or stopping"))
        } else {
            Ok(())
        }
    }
    pub fn finish_logged<T>(
        &mut self,
        result: Result<T, ActionError>,
        name: &str,
    ) -> Result<T, ActionError> {
        if let Err(error) = &result {
            self.emit(EventLevel::Error, format!("{name} failed: {error}"));
        }
        let cleanup = self.close();
        if let Err(error) = &cleanup {
            self.emit(EventLevel::Error, format!("{name} cleanup failed: {error}"));
        }
        merge(result, cleanup)
    }
}
fn merge<T>(
    result: Result<T, ActionError>,
    cleanup: Result<(), ActionError>,
) -> Result<T, ActionError> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(ActionError::runtime(format!("{error}; {cleanup}"))),
    }
}
fn cleanup_attempt(action: impl FnOnce() -> Result<(), ActionError>) -> Result<(), ActionError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(action))
        .unwrap_or_else(|_| Err(ActionError::runtime("action cleanup panicked")))
}
impl<B: ActionBackend> Drop for ActionContext<B> {
    fn drop(&mut self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.close()));
    }
}
fn combine(
    first: Result<(), ActionError>,
    second: Result<(), ActionError>,
) -> Result<(), ActionError> {
    match (first, second) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(first), Err(second)) => Err(ActionError::runtime(format!(
            "{first}; input release failed: {second}"
        ))),
    }
}
