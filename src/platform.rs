//! Safe OS boundaries. Only the Windows implementation may contain FFI.

use crate::common::{
    error::ControlError,
    frame::{BgrFrame, PixelRect},
    frame_geometry::PhysicalTarget,
};

#[cfg(windows)]
#[allow(unsafe_code)]
pub mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowId(usize);
impl WindowId {
    pub fn new(value: usize) -> Result<Self, ControlError> {
        if value == 0 {
            return Err(ControlError::Backend("null window identity".into()));
        }
        Ok(Self(value))
    }
    pub fn value(self) -> usize {
        self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: WindowId,
    pub title: String,
    pub target: PhysicalTarget,
}
/// Calls return owned pixels, never borrowed GDI buffers or owned window handles.
pub trait CaptureBackend: Send + 'static {
    fn windows(&self) -> Result<Vec<WindowInfo>, ControlError>;
    /// Index0 is the virtual desktop; subsequent entries preserve OS enumeration order.
    fn monitors(&self) -> Result<Vec<PhysicalTarget>, ControlError>;
    fn client_box(&self, window: &WindowInfo) -> Result<PixelRect, ControlError>;
    fn capture_window(&mut self, window: &WindowInfo) -> Result<Option<BgrFrame>, ControlError>;
    fn capture_screen(&mut self, target: PhysicalTarget) -> Result<BgrFrame, ControlError>;
    fn close(&mut self) -> Result<(), ControlError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Keyboard { scan: u16, up: bool, extended: bool },
    Mouse { dx: i32, dy: i32, flags: u32 },
}
pub trait InputSystem: Send + Sync + 'static {
    fn focused(&self, window: WindowId) -> Result<bool, ControlError>;
    /// 창을 최소화하지 않는 가상 입력 구현은 기본값을 사용한다.
    fn minimized(&self, _window: WindowId) -> Result<bool, ControlError> {
        Ok(false)
    }
    fn bounds(&self, window: WindowId) -> Result<PhysicalTarget, ControlError>;
    fn desktop(&self) -> Result<PhysicalTarget, ControlError>;
    fn pointer(&self) -> Result<[i32; 2], ControlError>;
    fn numlock_state(&self) -> Result<bool, ControlError>;
    fn arrow_scan(&self, virtual_key: u32) -> Result<u16, ControlError>;
    /// Return the inserted count. Callers must check each event; never silently retry.
    fn send(&self, event: InputEvent) -> Result<u32, ControlError>;
}
