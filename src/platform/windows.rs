//! All native pointers and unsafe operations stay inside this module.

mod capture;
mod console;
mod hotkeys;
mod input;
pub mod resources;
mod stdin;
mod window;
pub use console::ConsoleInterrupt;
pub use hotkeys::HotkeyListener;
pub use stdin::StdinLines;
pub use window::{DpiAwareness, enable_dpi_awareness, focus_window};
#[cfg(test)]
pub(crate) static CONSOLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 기존 로컬 증거 디렉터리의 날짜/시각 이름에 사용한다.
pub fn local_timestamp() -> String {
    let time = local_time();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}
fn local_time() -> windows_sys::Win32::Foundation::SYSTEMTIME {
    let mut time = windows_sys::Win32::Foundation::SYSTEMTIME::default();
    // SAFETY: 동기 호출의 출력 버퍼는 올바른 크기의 초기화된 지역 SYSTEMTIME이다.
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time) };
    time
}
pub fn local_log_timestamp() -> String {
    let t = local_time();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02},{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

#[derive(Debug, Clone, Copy)]
pub struct Windows {
    dpi: DpiAwareness,
}
impl Windows {
    /// Initialize before querying coordinates or creating a native control surface.
    pub fn new() -> Result<Self, crate::common::error::ControlError> {
        Ok(Self {
            dpi: enable_dpi_awareness()?,
        })
    }
    pub fn dpi_awareness(self) -> DpiAwareness {
        self.dpi
    }
}

fn failure(operation: &str) -> crate::common::error::ControlError {
    crate::common::error::ControlError::Backend(format!(
        "{operation} failed: {}",
        std::io::Error::last_os_error()
    ))
}
