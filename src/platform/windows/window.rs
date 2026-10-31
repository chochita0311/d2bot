use super::{Windows, failure};
use crate::{
    common::{error::ControlError, frame::PixelRect, frame_geometry::PhysicalTarget},
    platform::{WindowId, WindowInfo},
};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    Graphics::Gdi::{ClientToScreen, EnumDisplayMonitors, HDC, HMONITOR},
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
    UI::{
        HiDpi::{GetDpiForSystem, PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness},
        Input::KeyboardAndMouse::{
            KEYEVENTF_KEYUP, SetActiveWindow, SetFocus, VK_MENU, keybd_event,
        },
        WindowsAndMessaging::*,
    },
};

pub(super) fn hwnd(id: WindowId) -> HWND {
    id.value() as HWND
}
pub(super) fn rectangle(rect: RECT) -> Result<PhysicalTarget, ControlError> {
    let width = rect.right.checked_sub(rect.left).filter(|n| *n > 0);
    let height = rect.bottom.checked_sub(rect.top).filter(|n| *n > 0);
    match (width, height) {
        (Some(w), Some(h)) => PhysicalTarget::new(rect.left, rect.top, w as usize, h as usize)
            .map_err(|error| ControlError::Backend(error.to_string())),
        _ => Err(ControlError::Backend("invalid native rectangle".into())),
    }
}
pub(super) fn bounds(id: WindowId) -> Result<PhysicalTarget, ControlError> {
    let mut rect = RECT::default();
    // SAFETY: stack output lives through the synchronous call; HWND is non-owning.
    if unsafe { GetWindowRect(hwnd(id), &mut rect) } == 0 {
        return Err(failure("GetWindowRect"));
    }
    rectangle(rect)
}
pub(super) fn desktop() -> Result<PhysicalTarget, ControlError> {
    // SAFETY: GetSystemMetrics has no pointer arguments.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if width <= 0 || height <= 0 {
        return Err(failure("virtual desktop bounds"));
    }
    PhysicalTarget::new(left, top, width as usize, height as usize)
        .map_err(|e| ControlError::Backend(e.to_string()))
}
struct Enumeration<T> {
    values: Vec<T>,
    error: Option<ControlError>,
}
unsafe extern "system" fn enum_window(window: HWND, data: LPARAM) -> i32 {
    // SAFETY: EnumWindows invokes synchronously with the exact live Enumeration pointer.
    let state = unsafe { &mut *(data as *mut Enumeration<WindowInfo>) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), ControlError> {
            // SAFETY: all outputs are local; invalid/disappeared windows are skipped.
            if unsafe { IsWindowVisible(window) } == 0 {
                return Ok(());
            }
            let length = unsafe { GetWindowTextLengthW(window) };
            if length <= 0 {
                return Ok(());
            }
            let mut title = vec![0; length as usize + 1];
            let count = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
            let title = String::from_utf16_lossy(&title[..count.max(0) as usize])
                .trim()
                .to_owned();
            if title.is_empty() {
                return Ok(());
            }
            let id = WindowId::new(window as usize)?;
            if let Ok(target) = bounds(id) {
                state.values.push(WindowInfo { id, title, target });
            }
            Ok(())
        },
    ));
    match result {
        Ok(Ok(())) => 1,
        Ok(Err(error)) => {
            state.error = Some(error);
            0
        }
        Err(_) => {
            state.error = Some(ControlError::Backend(
                "window enumeration callback panicked".into(),
            ));
            0
        }
    }
}
unsafe extern "system" fn enum_monitor(_: HMONITOR, _: HDC, rect: *mut RECT, data: LPARAM) -> i32 {
    // SAFETY: OS supplies RECT for this synchronous callback; data belongs to the caller.
    let state = unsafe { &mut *(data as *mut Enumeration<PhysicalTarget>) };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if rect.is_null() {
            return Err(ControlError::Backend("null monitor bounds".into()));
        }
        rectangle(unsafe { *rect }).map(|value| state.values.push(value))
    }));
    match result {
        Ok(Ok(())) => 1,
        Ok(Err(error)) => {
            state.error = Some(error);
            0
        }
        Err(_) => {
            state.error = Some(ControlError::Backend(
                "monitor enumeration callback panicked".into(),
            ));
            0
        }
    }
}
pub(super) fn windows() -> Result<Vec<WindowInfo>, ControlError> {
    let mut state = Enumeration {
        values: Vec::new(),
        error: None,
    };
    // SAFETY: callback/data stay live until EnumWindows returns; no pointer is retained.
    let success = unsafe { EnumWindows(Some(enum_window), &mut state as *mut _ as LPARAM) };
    if let Some(error) = state.error {
        return Err(error);
    }
    if success == 0 {
        return Err(failure("EnumWindows"));
    }
    Ok(state.values)
}
pub(super) fn monitors() -> Result<Vec<PhysicalTarget>, ControlError> {
    let mut state = Enumeration {
        values: vec![desktop()?],
        error: None,
    };
    // SAFETY: same synchronous callback ownership as windows().
    let success = unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(enum_monitor),
            &mut state as *mut _ as LPARAM,
        )
    };
    if let Some(error) = state.error {
        return Err(error);
    }
    if success == 0 {
        return Err(failure("EnumDisplayMonitors"));
    }
    Ok(state.values)
}
pub(super) fn client_box(window: &WindowInfo) -> Result<PixelRect, ControlError> {
    let mut rect = RECT::default();
    let mut origin = POINT::default();
    // SAFETY: both output pointers refer to live stack values.
    if unsafe { GetClientRect(hwnd(window.id), &mut rect) } == 0
        || unsafe { ClientToScreen(hwnd(window.id), &mut origin) } == 0
    {
        return Err(failure("current game client bounds"));
    }
    let left = i64::from(origin.x) - i64::from(window.target.left);
    let top = i64::from(origin.y) - i64::from(window.target.top);
    let convert = |n: i64| {
        usize::try_from(n)
            .map_err(|_| ControlError::Backend("client rectangle outside captured frame".into()))
    };
    PixelRect::new(
        convert(left)?,
        convert(top)?,
        convert(left + i64::from(rect.right))?,
        convert(top + i64::from(rect.bottom))?,
    )
    .and_then(|r| r.checked_in(window.target.size()))
    .map_err(|e| ControlError::Backend(e.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiAwareness {
    PerMonitor,
    Legacy,
    AlreadyConfigured,
}
pub fn enable_dpi_awareness() -> Result<DpiAwareness, ControlError> {
    // SAFETY: process configuration has no pointer arguments. Access denied means a
    // manifest/framework already configured awareness; do not override it.
    let result = unsafe { SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE) };
    if result >= 0 {
        return Ok(DpiAwareness::PerMonitor);
    }
    if result as u32 == 0x80070005 {
        return Ok(DpiAwareness::AlreadyConfigured);
    }
    if unsafe { SetProcessDPIAware() } != 0 {
        return Ok(DpiAwareness::Legacy);
    }
    Err(failure("DPI awareness"))
}
struct ThreadAttachment {
    current: u32,
    target: u32,
    attached: bool,
}
impl ThreadAttachment {
    fn new(target: u32) -> Self {
        // SAFETY: IDs are obtained from Windows; attachment belongs to this thread.
        let current = unsafe { GetCurrentThreadId() };
        let attached = target != 0
            && target != current
            && unsafe { AttachThreadInput(current, target, 1) } != 0;
        Self {
            current,
            target,
            attached,
        }
    }
}
impl Drop for ThreadAttachment {
    fn drop(&mut self) {
        if self.attached {
            unsafe {
                AttachThreadInput(self.current, self.target, 0);
            }
        }
    }
}
/// Explicit activation only. Read-only capture/query never calls this function.
pub fn focus_window(window: WindowId) -> Result<bool, ControlError> {
    let window = hwnd(window);
    // SAFETY: operations use a non-owning HWND; no target process memory is accessed.
    unsafe {
        if IsWindow(window) == 0 {
            return Err(failure("focus window unavailable"));
        }
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        let foreground = GetForegroundWindow();
        if foreground == window {
            return Ok(true);
        }
        let _foreground = ThreadAttachment::new(if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, null_mut())
        });
        let _target = ThreadAttachment::new(GetWindowThreadProcessId(window, null_mut()));
        let activate = || {
            BringWindowToTop(window);
            SetActiveWindow(window);
            SetFocus(window);
            SetForegroundWindow(window);
        };
        activate();
        if GetForegroundWindow() != window {
            keybd_event(VK_MENU as u8, 0, 0, 0);
            keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
            activate();
        }
        Ok(GetForegroundWindow() == window)
    }
}

impl Windows {
    /// DPI 인식 설정 뒤 창을 만들 때 원본의 물리 픽셀 크기를 논리 단위로 변환한다.
    pub fn system_ui_scale(self) -> Result<f32, ControlError> {
        // SAFETY: 포인터 인자가 없는 시스템 DPI 조회다. 값은 호출 시마다 읽는다.
        let dpi = unsafe { GetDpiForSystem() };
        if dpi == 0 {
            return Err(ControlError::Backend("System DPI is unavailable.".into()));
        }
        Ok(dpi as f32 / 96.0)
    }
    pub fn list_windows(self) -> Result<Vec<WindowInfo>, ControlError> {
        windows()
    }
}
