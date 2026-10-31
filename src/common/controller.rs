//! Guarded input with a small shared gate, independent of any actor/controller cycle.

use super::{
    error::ControlError,
    field_control::{DesktopPoint, FieldInputBackend},
    frame_geometry::PhysicalTarget,
    time::MonotonicClock,
    workers::Cancellation,
};
use crate::platform::{InputEvent, InputSystem, WindowId};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub struct InputGate(Arc<GateState>);
#[derive(Default)]
struct GateState {
    stop: Cancellation,
    paused: AtomicBool,
}
impl InputGate {
    pub fn stop(&self) {
        self.0.stop.cancel();
    }
    pub fn pause(&self, paused: bool) {
        self.0.paused.store(paused, Ordering::Release);
    }
    pub fn paused(&self) -> bool {
        self.0.paused.load(Ordering::Acquire)
    }
    pub fn stopped(&self) -> bool {
        self.0.stop.is_cancelled()
    }
    pub fn interrupted(&self) -> bool {
        self.stopped() || self.paused()
    }
    pub fn wait(&self, duration: Duration) -> Result<bool, ControlError> {
        let start = std::time::Instant::now();
        while !self.interrupted() {
            let Some(remaining) = duration.checked_sub(start.elapsed()) else {
                return Ok(false);
            };
            if remaining.is_zero() {
                return Ok(false);
            }
            self.0.stop.wait(remaining.min(Duration::from_millis(5)))?;
        }
        Ok(true)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct PointerEvidence {
    pub requested: DesktopPoint,
    pub observed: [i32; 2],
    pub desktop: PhysicalTarget,
    pub settle_seconds: f64,
}

pub fn contains(target: PhysicalTarget, point: [i32; 2]) -> bool {
    let [x, y] = point.map(i64::from);
    let left = i64::from(target.left);
    let top = i64::from(target.top);
    left <= x
        && x < left + target.size().width() as i64
        && top <= y
        && y < top + target.size().height() as i64
}
pub fn normalize_pointer(
    target: PhysicalTarget,
    point: [i32; 2],
) -> Result<[i32; 2], ControlError> {
    let size = target.size();
    if size.width() <= 1 || size.height() <= 1 || !contains(target, point) {
        return Err(ControlError::Backend(
            "field aim outside virtual desktop".into(),
        ));
    }
    Ok([
        ((f64::from(point[0]) - f64::from(target.left)) * 65535.0 / (size.width() - 1) as f64)
            .round_ties_even() as i32,
        ((f64::from(point[1]) - f64::from(target.top)) * 65535.0 / (size.height() - 1) as f64)
            .round_ties_even() as i32,
    ])
}
/// Preserve the installed pydirectinput1.0.4 scan-code payloads, including its
/// legacy extended-key numbers. Arrow codes are queried from the active OS map.
pub(crate) fn keyboard_events<S: InputSystem>(
    system: &S,
    key: &str,
    up: bool,
) -> Result<Vec<InputEvent>, ControlError> {
    let arrow = match key {
        "left" => Some(0x25),
        "up" => Some(0x26),
        "right" => Some(0x27),
        "down" => Some(0x28),
        _ => None,
    };
    let scan = if let Some(virtual_key) = arrow {
        system.arrow_scan(virtual_key)?
    } else {
        key_scan(key)
            .ok_or_else(|| ControlError::Backend(format!("unsupported input key '{key}'")))?
    };
    let event = InputEvent::Keyboard {
        scan,
        up,
        extended: arrow.is_some(),
    };
    let mut events = Vec::with_capacity(2);
    if arrow.is_some() && system.numlock_state()? {
        let prefix = InputEvent::Keyboard {
            scan: 0xe0,
            up,
            extended: false,
        };
        if !up {
            events.push(prefix);
        }
        events.push(event);
        if up {
            events.push(prefix);
        }
    } else {
        events.push(event);
    }
    Ok(events)
}
fn key_scan(key: &str) -> Option<u16> {
    if key.len() == 1 {
        for (letters, start) in [
            ("1234567890-=", 0x02),
            ("qwertyuiop[]", 0x10),
            ("asdfghjkl;'", 0x1e),
            ("zxcvbnm,./", 0x2c),
        ] {
            if let Some(index) = letters.find(key) {
                return Some(start + index as u16);
            }
        }
    }
    Some(match key {
        "escape" | "esc" => 0x01,
        "backspace" => 0x0e,
        "tab" => 0x0f,
        "enter" | "return" => 0x1c,
        "ctrl" | "ctrlleft" => 0x1d,
        "shift" | "shiftleft" => 0x2a,
        "\\" => 0x2b,
        "shiftright" => 0x36,
        "multiply" => 0x37,
        "alt" | "altleft" => 0x38,
        " " | "space" => 0x39,
        "capslock" => 0x3a,
        "f1" => 0x3b,
        "f2" => 0x3c,
        "f3" => 0x3d,
        "f4" => 0x3e,
        "f5" => 0x3f,
        "f6" => 0x40,
        "f7" => 0x41,
        "f8" => 0x42,
        "f9" => 0x43,
        "f10" => 0x44,
        "f11" => 0x57,
        "f12" => 0x58,
        "numlock" => 0x45,
        "scrolllock" => 0x46,
        "subtract" => 0x4a,
        "add" => 0x4e,
        "decimal" => 0x53,
        "`" => 0x29,
        "pause" => 0xc5,
        "printscreen" | "prntscrn" | "prtsc" | "prtscr" => 0xb7,
        "insert" => 0xd2 + 1024,
        "home" => 0xc7 + 1024,
        "pageup" => 0xc9 + 1024,
        "pagedown" => 0xd1 + 1024,
        "divide" => 0xb5 + 1024,
        "delete" | "del" => 0xd3 + 1024,
        "end" => 0xcf + 1024,
        "win" | "winleft" => 0xdb + 1024,
        "winright" => 0xdc + 1024,
        "apps" => 0xdd + 1024,
        "altright" => 0xb8 + 1024,
        "ctrlright" => 0x9d + 1024,
        _ => return None,
    })
}
type CurrentGuard = Arc<dyn Fn() -> Result<bool, ControlError> + Send + Sync>;
pub struct ControllerFieldInput<S: InputSystem> {
    system: Arc<S>,
    gate: InputGate,
    window: WindowId,
    clock: Arc<dyn MonotonicClock>,
    pointer_evidence: std::sync::Mutex<Option<PointerEvidence>>,
    ownership: std::sync::Mutex<NativeOwnership>,
    current_guard: Option<CurrentGuard>,
}
#[derive(Default)]
struct NativeOwnership {
    keys: BTreeSet<String>,
    buttons: BTreeSet<String>,
}
impl<S: InputSystem> ControllerFieldInput<S> {
    pub fn new(
        system: Arc<S>,
        gate: InputGate,
        window: WindowId,
        clock: Arc<dyn MonotonicClock>,
    ) -> Self {
        Self {
            system,
            gate,
            window,
            clock,
            pointer_evidence: std::sync::Mutex::new(None),
            ownership: std::sync::Mutex::new(NativeOwnership::default()),
            current_guard: None,
        }
    }
    /// 실행 소유자의 현재 창·좌표·중단 검사를 각 입력 직전과 포인터 대기 중에도 적용한다.
    pub fn with_guard(
        mut self,
        guard: impl Fn() -> Result<bool, ControlError> + Send + Sync + 'static,
    ) -> Self {
        self.current_guard = Some(Arc::new(guard));
        self
    }
    pub fn gate(&self) -> &InputGate {
        &self.gate
    }
    pub fn guard(&self) -> Result<(), ControlError> {
        self.base_guard()?;
        if let Some(guard) = &self.current_guard
            && !guard()?
        {
            return Err(ControlError::Runtime(
                "field input interrupted or game unfocused".into(),
            ));
        }
        Ok(())
    }
    fn base_guard(&self) -> Result<(), ControlError> {
        if self.gate.interrupted() || !self.system.focused(self.window)? {
            return Err(ControlError::Runtime(
                "field input interrupted or game unfocused".into(),
            ));
        }
        Ok(())
    }
    fn failsafe(&self) -> Result<(), ControlError> {
        if self.system.pointer()? == [0, 0] {
            return Err(ControlError::Runtime(
                "PyDirectInput fail-safe triggered from mouse moving to a corner of the screen."
                    .into(),
            ));
        }
        Ok(())
    }
    fn dispatch_key(&self, key: &str, up: bool) -> Result<(), ControlError> {
        let mut ownership = match self.ownership.lock() {
            Ok(ownership) => ownership,
            Err(poisoned) if up => poisoned.into_inner(),
            Err(_) => {
                return Err(ControlError::Runtime(
                    "native input ownership poisoned".into(),
                ));
            }
        };
        self.key_event(key, up, &mut ownership)
    }
    fn key_event(
        &self,
        key: &str,
        up: bool,
        ownership: &mut NativeOwnership,
    ) -> Result<(), ControlError> {
        let events = keyboard_events(self.system.as_ref(), key, up)?;
        if !up {
            // 창 열거/좌표 검사는 바깥 guard에서 끝낸다. 정리 잠금 안에서는 짧은 재검사만 한다.
            self.base_guard()?;
            self.failsafe()?;
            ownership.keys.insert(key.to_owned());
        }
        let mut failed = false;
        let mut errors = Vec::new();
        // A failed prefix must not skip the remaining release events.
        for event in events {
            match self.system.send(event) {
                Ok(1) => {}
                Ok(_) => failed = true,
                Err(error) => errors.push(error.to_string()),
            }
        }
        if failed || !errors.is_empty() {
            return Err(ControlError::Backend(format!(
                "field key {} was not dispatched{}",
                if up { "release" } else { "down" },
                if errors.is_empty() {
                    String::new()
                } else {
                    format!(": {}", errors.join("; "))
                }
            )));
        }
        if up {
            ownership.keys.remove(key);
        }
        Ok(())
    }
    pub fn pointer_evidence(&self) -> Result<Option<PointerEvidence>, ControlError> {
        self.pointer_evidence
            .lock()
            .map(|value| value.clone())
            .map_err(|_| ControlError::Runtime("pointer evidence poisoned".into()))
    }
    pub fn mouse_button(&self, button: &str, up: bool) -> Result<(), ControlError> {
        if !up {
            self.guard()?;
        }
        let mut ownership = match self.ownership.lock() {
            Ok(ownership) => ownership,
            Err(poisoned) if up => poisoned.into_inner(),
            Err(_) => {
                return Err(ControlError::Runtime(
                    "native input ownership poisoned".into(),
                ));
            }
        };
        self.button_event(button, up, &mut ownership)
    }
    fn button_event(
        &self,
        button: &str,
        up: bool,
        ownership: &mut NativeOwnership,
    ) -> Result<(), ControlError> {
        if !up {
            self.base_guard()?;
            self.failsafe()?;
        }
        let flags = match (button, up) {
            ("left", false) => 2,
            ("left", true) => 4,
            ("right", false) => 8,
            ("right", true) => 16,
            ("middle", false) => 32,
            ("middle", true) => 64,
            _ => return Err(ControlError::Backend("invalid mouse button".into())),
        };
        if !up {
            ownership.buttons.insert(button.to_owned());
        }
        if self.system.send(InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags,
        })? != 1
        {
            return Err(ControlError::Backend(
                "field mouse input was not dispatched".into(),
            ));
        }
        if up {
            ownership.buttons.remove(button);
        }
        Ok(())
    }
    /// Physical dispatch ownership also covers a one-shot press whose final up failed.
    /// Pointer waits never take this short dispatch lock. Failed releases remain for retry.
    pub fn release_outstanding(&self) -> Result<(), ControlError> {
        let (mut ownership, poisoned) = match self.ownership.lock() {
            Ok(ownership) => (ownership, false),
            Err(poisoned) => (poisoned.into_inner(), true),
        };
        let mut errors = Vec::new();
        if poisoned {
            errors.push("native input ownership poisoned; cleanup attempted".into());
        }
        for key in ownership.keys.clone() {
            if let Err(error) = release_attempt(|| self.key_event(&key, true, &mut ownership)) {
                errors.push(error.to_string());
            }
        }
        for button in ownership.buttons.clone() {
            if let Err(error) = release_attempt(|| self.button_event(&button, true, &mut ownership))
            {
                errors.push(error.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ControlError::Backend(errors.join("; ")))
        }
    }
    pub fn close(&self) -> Result<(), ControlError> {
        self.gate.stop();
        self.release_outstanding()
    }
    pub fn click(&self) -> Result<(), ControlError> {
        self.guard()?;
        if !contains(self.system.bounds(self.window)?, self.system.pointer()?) {
            return Err(ControlError::Backend(
                "field click outside current game window".into(),
            ));
        }
        let result = self
            .mouse_button("left", false)
            .and_then(|_| self.gate.wait(Duration::from_millis(50)).map(|_| ()));
        combine(result, self.mouse_button("left", true))
    }
    /// 기존 action의 단일 down/up 묶음에는 50ms 필드 대기를 더하지 않는다.
    /// 실패한 전송은 정리 소유권에 남겨 상위 실행기의 해제 재시도를 허용한다.
    pub fn click_without_pause(&self, button: &str) -> Result<(), ControlError> {
        self.guard()?;
        self.failsafe()?;
        let flags = match button {
            "left" => 2 | 4,
            "right" => 8 | 16,
            "middle" => 32 | 64,
            _ => return Err(ControlError::Backend("invalid mouse button".into())),
        };
        let mut ownership = self
            .ownership
            .lock()
            .map_err(|_| ControlError::Runtime("native input ownership poisoned".into()))?;
        self.base_guard()?;
        ownership.buttons.insert(button.to_owned());
        if self.system.send(InputEvent::Mouse {
            dx: 0,
            dy: 0,
            flags,
        })? != 1
        {
            return Err(ControlError::Backend(
                "action mouse input was not dispatched".into(),
            ));
        }
        ownership.buttons.remove(button);
        Ok(())
    }
}
impl<S: InputSystem> Drop for ControllerFieldInput<S> {
    fn drop(&mut self) {
        super::error::best_effort_cleanup(|| self.close());
    }
}
fn combine(
    first: Result<(), ControlError>,
    cleanup: Result<(), ControlError>,
) -> Result<(), ControlError> {
    match (first, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
        (Err(e), Err(c)) => Err(ControlError::Runtime(format!(
            "{e}; input release failed: {c}"
        ))),
    }
}
fn release_attempt(action: impl FnOnce() -> Result<(), ControlError>) -> Result<(), ControlError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)).unwrap_or_else(|_| {
        Err(ControlError::Runtime(
            "native input release panicked".into(),
        ))
    })
}
impl<S: InputSystem> FieldInputBackend for ControllerFieldInput<S> {
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        self.guard()?;
        self.failsafe()?;
        self.dispatch_key(key, false)
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        self.dispatch_key(key, true)
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        self.guard()?;
        self.failsafe()?;
        let result = self
            .dispatch_key(key, false)
            .and_then(|_| self.gate.wait(Duration::from_millis(50)).map(|_| ()));
        combine(result, self.dispatch_key(key, true))
    }
    fn move_pointer(&self, point: DesktopPoint) -> Result<(), ControlError> {
        self.guard()?;
        let point = [i32::try_from(point[0]), i32::try_from(point[1])];
        let point = match point {
            [Ok(x), Ok(y)] => [x, y],
            _ => {
                return Err(ControlError::Backend(
                    "field aim outside current game window".into(),
                ));
            }
        };
        if !contains(self.system.bounds(self.window)?, point) {
            return Err(ControlError::Backend(
                "field aim outside current game window".into(),
            ));
        }
        *self
            .pointer_evidence
            .lock()
            .map_err(|_| ControlError::Runtime("pointer evidence poisoned".into()))? = None;
        let desktop = self.system.desktop()?;
        let [dx, dy] = normalize_pointer(desktop, point)?;
        let inserted = {
            // Queries may be slow. Serialize only the final short dispatch with close,
            // then recheck interruption so a stopped operation cannot move later.
            let _ownership = self
                .ownership
                .lock()
                .map_err(|_| ControlError::Runtime("native input ownership poisoned".into()))?;
            self.base_guard()?;
            self.system.send(InputEvent::Mouse {
                dx,
                dy,
                flags: 0x0001 | 0x8000 | 0x4000,
            })?
        };
        if inserted != 1 {
            return Err(ControlError::Backend(
                "field pointer movement was not dispatched".into(),
            ));
        }
        let started = super::time::nonnegative(self.clock.now(), "pointer clock")?;
        loop {
            self.guard()?;
            let actual = self.system.pointer()?;
            let elapsed = self.clock.now() - started;
            if !elapsed.is_finite() || elapsed < 0.0 {
                return Err(ControlError::Runtime(
                    "pointer monotonic clock moved backwards".into(),
                ));
            }
            if point.into_iter().zip(actual).all(|(requested, observed)| {
                (i64::from(requested) - i64::from(observed)).abs() <= 3
            }) {
                *self
                    .pointer_evidence
                    .lock()
                    .map_err(|_| ControlError::Runtime("pointer evidence poisoned".into()))? =
                    Some(PointerEvidence {
                        requested: [i64::from(point[0]), i64::from(point[1])],
                        observed: actual,
                        desktop,
                        settle_seconds: elapsed,
                    });
                return Ok(());
            }
            if elapsed >= 0.08 {
                return Err(ControlError::Runtime(format!(
                    "field pointer mapping mismatch: requested {point:?}, observed {actual:?}"
                )));
            }
            self.gate.wait(Duration::from_millis(5))?;
        }
    }
    fn supports_pointer_confirmation(&self) -> bool {
        true
    }
    fn pointer_matches(&self, point: DesktopPoint) -> Result<bool, ControlError> {
        self.guard()?;
        Ok(self
            .system
            .pointer()?
            .into_iter()
            .zip(point)
            .all(|(actual, expected)| (i128::from(actual) - i128::from(expected)).abs() <= 3))
    }
}
