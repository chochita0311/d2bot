use crate::{
    common::{
        capture::{CaptureSettings, find_window},
        controller::ControllerFieldInput,
        error::ControlError,
        field_control::{DesktopPoint, FieldInputBackend, InputObservation},
        frame_geometry::{FrameGeometry, PhysicalTarget},
        realtime::CapturedFrame,
        time::MonotonicClock,
    },
    platform::{CaptureBackend, InputSystem, WindowInfo},
    runs::summoner::canyon_finish::CanyonBackend,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, RwLock};
#[cfg(test)]
mod tests;
/// 포커스와 현재 물리 창/클라이언트 범위를 다시 조회한다. 잠금 안에서는 OS 작업을 하지 않는다.
pub struct WindowGuard<S: CaptureBackend + InputSystem> {
    system: Arc<S>,
    settings: CaptureSettings,
    window: WindowInfo,
    geometry: RwLock<Option<Arc<FrameGeometry>>>,
}
impl<S: CaptureBackend + InputSystem> WindowGuard<S> {
    pub fn new(system: Arc<S>, settings: CaptureSettings, window: WindowInfo) -> Self {
        Self {
            system,
            settings,
            window,
            geometry: RwLock::new(None),
        }
    }
    pub fn window(&self) -> &WindowInfo {
        &self.window
    }
    pub(crate) fn system(&self) -> Arc<S> {
        self.system.clone()
    }
    pub fn target(&self) -> PhysicalTarget {
        self.window.target
    }
    pub fn geometry(&self) -> Result<Option<Arc<FrameGeometry>>, ControlError> {
        self.geometry
            .read()
            .map(|v| v.clone())
            .map_err(|_| ControlError::Runtime("field geometry poisoned".into()))
    }
    pub fn admit(&self, frame: &CapturedFrame) -> Result<(), ControlError> {
        if frame.target != self.window.target {
            return Err(ControlError::Runtime(
                "window geometry changed after capture".into(),
            ));
        }
        *self
            .geometry
            .write()
            .map_err(|_| ControlError::Runtime("field geometry poisoned".into()))? =
            frame.geometry.clone();
        Ok(())
    }
    pub fn focused(&self) -> Result<bool, ControlError> {
        if !self.system.focused(self.window.id)? || self.system.minimized(self.window.id)? {
            return Ok(false);
        }
        let Some(title) = self.settings.window_title.as_deref() else {
            return Ok(false);
        };
        let Some(window) = find_window(
            self.system.windows()?,
            title,
            &self.settings.window_title_mode,
        ) else {
            return Ok(false);
        };
        if window.target != self.window.target {
            return Ok(false);
        }
        let geometry = self.geometry()?;
        if let Some(g) = geometry {
            let client = if self
                .settings
                .reference_layout
                .as_deref()
                .is_some_and(|p| !p.is_empty())
            {
                self.system.client_box(&window)?
            } else {
                crate::common::frame::PixelRect::full(window.target.size())
            };
            if g.target() != window.target || g.source_client() != client {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn to_desktop(&self, p: [f64; 2]) -> Result<DesktopPoint, ControlError> {
        if !self.focused()? {
            return Err(ControlError::Runtime(
                "window geometry changed after capture".into(),
            ));
        }
        match self.geometry()? {
            Some(g) => g
                .to_desktop(p)
                .map_err(|e| ControlError::Runtime(e.to_string())),
            None => Err(ControlError::Runtime(
                "current frame geometry required".into(),
            )),
        }
    }
    pub fn from_desktop(&self, p: DesktopPoint) -> Result<[i32; 2], ControlError> {
        self.from_desktop_float(p.map(|v| v as f64))
    }
    pub fn from_desktop_float(&self, p: [f64; 2]) -> Result<[i32; 2], ControlError> {
        if !self.focused()? {
            return Err(ControlError::Runtime(
                "window geometry changed after capture".into(),
            ));
        }
        let g = self
            .geometry()?
            .ok_or_else(|| ControlError::Runtime("current frame geometry required".into()))?;
        let p = g
            .from_desktop(p)
            .map_err(|e| ControlError::Runtime(e.to_string()))?;
        let p = p.map(|n| n.round_ties_even());
        if p.iter()
            .any(|v| !v.is_finite() || *v < f64::from(i32::MIN) || *v > f64::from(i32::MAX))
        {
            return Err(ControlError::Runtime(
                "frame point outside coordinate range".into(),
            ));
        }
        Ok(p.map(|v| v as i32))
    }
}
type WallClock = Arc<dyn Fn() -> f64 + Send + Sync>;
/// 요청을 먼저 기록하고 OS 호출이 성공한 행만 sent로 바꾼다. 느린 입력 중 기록 잠금을 유지하지 않는다.
pub struct RecordedInput<S: InputSystem> {
    backend: ControllerFieldInput<S>,
    clock: Arc<dyn MonotonicClock>,
    wall: WallClock,
    events: Mutex<Vec<Value>>,
}
impl<S: InputSystem> RecordedInput<S> {
    pub fn new(
        backend: ControllerFieldInput<S>,
        clock: Arc<dyn MonotonicClock>,
        wall: WallClock,
    ) -> Self {
        Self {
            backend,
            clock,
            wall,
            events: Mutex::new(vec![]),
        }
    }
    pub fn events(&self) -> Result<Value, ControlError> {
        self.events
            .lock()
            .map(|v| json!(*v))
            .map_err(|_| ControlError::Runtime("input events poisoned".into()))
    }
    pub(crate) fn controller(&self) -> &ControllerFieldInput<S> {
        &self.backend
    }
    fn send(
        &self,
        action: &str,
        args: Value,
        call: impl FnOnce() -> Result<(), ControlError>,
    ) -> Result<(), ControlError> {
        self.send_observed(action, args, None, call)
    }
    fn send_observed(
        &self,
        action: &str,
        args: Value,
        observation: Option<InputObservation>,
        call: impl FnOnce() -> Result<(), ControlError>,
    ) -> Result<(), ControlError> {
        let started = self.clock.now();
        let mut row =
            json!({"action":action,"args":args,"at":started,"wall_at":(self.wall)(),"sent":false});
        if let Some(source) = observation {
            row["observation"] = json!(source);
        }
        let index = {
            let mut rows = self
                .events
                .lock()
                .map_err(|_| ControlError::Runtime("input events poisoned".into()))?;
            let index = rows.len();
            rows.push(row);
            index
        };
        call()?;
        let returned = self.clock.now();
        let pointer = if action == "move" {
            self.backend.pointer_evidence()?
        } else {
            None
        };
        let mut rows = self
            .events
            .lock()
            .map_err(|_| ControlError::Runtime("input events poisoned".into()))?;
        let row = &mut rows[index];
        // native method 완료는 게임 효과 확인 시각과 구분한다.
        row["backend_returned_at"] = json!(returned);
        row["backend_seconds"] = json!(returned - started);
        if let Some(p) = pointer {
            let d = p.desktop;
            row["pointer_evidence"] = json!({"requested":p.requested,"observed":p.observed,"desktop":[d.left as i64,d.top as i64,d.size().width() as i64,d.size().height() as i64],"settle_seconds":p.settle_seconds});
        }
        row["sent"] = json!(true);
        Ok(())
    }
    pub fn close(&self) -> Result<(), ControlError> {
        self.backend.close()
    }
    pub fn release_outstanding(&self) -> Result<(), ControlError> {
        self.backend.release_outstanding()
    }
}
impl<S: InputSystem> FieldInputBackend for RecordedInput<S> {
    fn key_down(&self, k: &str) -> Result<(), ControlError> {
        self.send("key_down", json!([k]), || self.backend.key_down(k))
    }
    fn key_down_observed(
        &self,
        k: &str,
        source: Option<InputObservation>,
    ) -> Result<(), ControlError> {
        self.send_observed("key_down", json!([k]), source, || self.backend.key_down(k))
    }
    fn key_up(&self, k: &str) -> Result<(), ControlError> {
        self.send("key_up", json!([k]), || self.backend.key_up(k))
    }
    fn press(&self, k: &str) -> Result<(), ControlError> {
        self.send("press", json!([k]), || self.backend.press(k))
    }
    fn move_pointer(&self, p: DesktopPoint) -> Result<(), ControlError> {
        self.send("move", json!(p), || self.backend.move_pointer(p))
    }
    fn move_pointer_observed(
        &self,
        p: DesktopPoint,
        source: Option<InputObservation>,
    ) -> Result<(), ControlError> {
        self.send_observed("move", json!(p), source, || self.backend.move_pointer(p))
    }
    fn supports_pointer_confirmation(&self) -> bool {
        self.backend.supports_pointer_confirmation()
    }
    fn pointer_matches(&self, p: DesktopPoint) -> Result<bool, ControlError> {
        self.backend.pointer_matches(p)
    }
}
impl<S: InputSystem> CanyonBackend for RecordedInput<S> {
    fn click(&self) -> Result<(), ControlError> {
        self.send("click", json!([]), || self.backend.click())
    }
}
