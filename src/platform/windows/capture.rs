use super::{Windows, failure, window};
use crate::{
    common::{
        error::ControlError,
        frame::{BgrFrame, PixelRect},
        frame_geometry::PhysicalTarget,
    },
    platform::{CaptureBackend, WindowInfo},
};
use std::ptr::null_mut;
use windows_sys::Win32::{Foundation::HWND, Graphics::Gdi::*, Storage::Xps::PrintWindow};

// 모든 GDI 자원과 raw slice는 한 호출/한 스레드 안에서만 살아 있다.
struct Surface {
    window: HWND,
    source: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut std::ffi::c_void,
    byte_len: usize,
}
impl Surface {
    fn new(window: HWND, size: crate::common::frame::Size) -> Result<Option<Self>, ControlError> {
        let width = i32::try_from(size.width())
            .map_err(|_| ControlError::Backend("capture width exceeds native range".into()))?;
        let height = i32::try_from(size.height())
            .map_err(|_| ControlError::Backend("capture height exceeds native range".into()))?;
        let byte_len = size
            .width()
            .checked_mul(size.height())
            .and_then(|n| n.checked_mul(4))
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or_else(|| ControlError::Backend("native capture size overflow".into()))?;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut owner = Self {
            window,
            source: null_mut(),
            memory: null_mut(),
            bitmap: null_mut(),
            previous: null_mut(),
            bits: null_mut(),
            byte_len,
        };
        // SAFETY: owned resources are tracked before any following fallible operation.
        let available = (|| {
            owner.source = unsafe { GetWindowDC(window) };
            if owner.source.is_null() {
                return false;
            }
            owner.memory = unsafe { CreateCompatibleDC(owner.source) };
            if owner.memory.is_null() {
                return false;
            }
            owner.bitmap = unsafe {
                CreateDIBSection(
                    owner.source,
                    &info,
                    DIB_RGB_COLORS,
                    &mut owner.bits,
                    null_mut(),
                    0,
                )
            };
            if owner.bitmap.is_null() || owner.bits.is_null() {
                return false;
            }
            // SAFETY: OS가 소유한 top-down32bit DIB는 정확히 byte_len bytes다.
            // 부분 그리기/실패에서도 초기화하지 않은 byte를 읽지 않는다.
            unsafe {
                std::ptr::write_bytes(owner.bits.cast::<u8>(), 0, owner.byte_len);
            }
            owner.previous = unsafe { SelectObject(owner.memory, owner.bitmap) };
            if owner.previous.is_null() || owner.previous as isize == -1 {
                owner.previous = null_mut();
                return false;
            }
            true
        })();
        if available {
            Ok(Some(owner))
        } else {
            owner.close()?;
            Ok(None)
        }
    }
    fn unselect(&mut self) -> Result<(), ControlError> {
        if !self.previous.is_null() {
            // SAFETY: previous was returned from this DC's successful SelectObject.
            let result = unsafe { SelectObject(self.memory, self.previous) };
            if result.is_null() || result as isize == -1 {
                return Err(failure("restore selected bitmap"));
            }
            self.previous = null_mut();
        }
        Ok(())
    }
    fn pixels(&self) -> Result<&[u8], ControlError> {
        if self.bits.is_null() || self.bitmap.is_null() {
            return Err(ControlError::Backend("capture DIB is closed".into()));
        }
        // SAFETY: 호출자는 GdiFlush와 선택 해제 뒤 읽는다. 크기/stride는 생성 때
        // 확인한32bit DIB이고 borrow 동안 bitmap을 삭제하거나 다시 그리지 않는다.
        Ok(unsafe { std::slice::from_raw_parts(self.bits.cast::<u8>(), self.byte_len) })
    }
    fn close(&mut self) -> Result<(), ControlError> {
        let mut errors = Vec::new();
        if let Err(error) = self.unselect() {
            errors.push(error.to_string());
        }
        // Delete DC first if restoring selection failed; its bitmap then becomes unselected.
        if !self.memory.is_null() {
            if unsafe { DeleteDC(self.memory) } == 0 {
                errors.push(failure("DeleteDC").to_string());
            } else {
                self.memory = null_mut();
                self.previous = null_mut();
            }
        }
        if !self.bitmap.is_null() && self.previous.is_null() {
            if unsafe { DeleteObject(self.bitmap) } == 0 {
                errors.push(failure("DeleteObject").to_string());
            } else {
                self.bitmap = null_mut();
                self.bits = null_mut();
            }
        }
        if !self.source.is_null() {
            if unsafe { ReleaseDC(self.window, self.source) } == 0 {
                errors.push(failure("ReleaseDC").to_string());
            } else {
                self.source = null_mut();
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ControlError::Backend(errors.join("; ")))
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| self.close());
    }
}
fn capture(
    id: HWND,
    target: PhysicalTarget,
    named: bool,
) -> Result<Option<BgrFrame>, ControlError> {
    let size = target.size();
    let Some(mut surface) = Surface::new(id, size)? else {
        return if named {
            Ok(None)
        } else {
            Err(failure("screen capture resources"))
        };
    };
    let result = (|| {
        // SAFETY: handles are owned by surface and dimensions are checked native sizes.
        let success = if named && unsafe { PrintWindow(id, surface.memory, 2) } != 0 {
            true
        } else {
            (unsafe {
                BitBlt(
                    surface.memory,
                    0,
                    0,
                    size.width() as i32,
                    size.height() as i32,
                    surface.source,
                    if named { 0 } else { target.left },
                    if named { 0 } else { target.top },
                    if named { SRCCOPY } else { SRCCOPY | CAPTUREBLT },
                )
            }) != 0
        };
        if !success {
            return if named {
                Ok(None)
            } else {
                Err(failure("BitBlt"))
            };
        }
        // SAFETY: DIB byte를 CPU가 읽기 전에 이 스레드의 GDI 그리기를 완료한다.
        if unsafe { GdiFlush() } == 0 {
            return if named {
                Ok(None)
            } else {
                Err(failure("GdiFlush"))
            };
        }
        surface.unselect()?;
        let bytes = surface.pixels()?;
        let mut frame = BgrFrame::zeros(size).map_err(|e| ControlError::Backend(e.to_string()))?;
        for (bgr, bgra) in frame
            .pixels_mut()
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(bytes.as_chunks::<4>().0)
        {
            bgr.copy_from_slice(&bgra[..3]);
        }
        if named && frame.pixels().iter().all(|byte| *byte == 0) {
            Ok(None)
        } else {
            Ok(Some(frame))
        }
    })();
    let cleanup = surface.close();
    match (result, cleanup) {
        (Ok(frame), Ok(())) => Ok(frame),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(ControlError::Backend(format!(
            "{error}; capture cleanup failed: {cleanup}"
        ))),
    }
}
impl CaptureBackend for Windows {
    fn windows(&self) -> Result<Vec<WindowInfo>, ControlError> {
        window::windows()
    }
    fn monitors(&self) -> Result<Vec<PhysicalTarget>, ControlError> {
        window::monitors()
    }
    fn client_box(&self, info: &WindowInfo) -> Result<PixelRect, ControlError> {
        window::client_box(info)
    }
    fn capture_window(&mut self, info: &WindowInfo) -> Result<Option<BgrFrame>, ControlError> {
        // Acquisition errors are window-capture failure, allowing the existing auto fallback.
        capture(window::hwnd(info.id), info.target, true)
    }
    fn capture_screen(&mut self, target: PhysicalTarget) -> Result<BgrFrame, ControlError> {
        capture(null_mut(), target, false)?
            .ok_or_else(|| ControlError::Backend("screen capture unavailable".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "brief owned 16px fixture window; never activates or sends input"]
    fn native_fixture_window_capture() -> Result<(), ControlError> {
        window::enable_dpi_awareness()?;
        use windows_sys::Win32::{
            Foundation::{LPARAM, LRESULT, RECT, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::*,
        };
        unsafe fn paint_pixels(dc: HDC) {
            for (top, bottom, color) in [(0, 8, 0x00a95511), (8, 16, 0x00224477)] {
                let rect = RECT {
                    left: 0,
                    top,
                    right: 16,
                    bottom,
                };
                let old = unsafe { SetDCBrushColor(dc, color) };
                unsafe {
                    FillRect(dc, &rect, GetStockObject(DC_BRUSH).cast());
                    SetDCBrushColor(dc, old);
                }
            }
        }
        unsafe extern "system" fn paint_fixture(
            handle: HWND,
            message: u32,
            dc: WPARAM,
            data: LPARAM,
        ) -> LRESULT {
            if matches!(message, WM_PRINT | WM_PRINTCLIENT) {
                // SAFETY: WM_PRINT supplies a borrowed destination HDC for this synchronous call.
                unsafe {
                    paint_pixels(dc as HDC);
                }
                0
            } else if message == WM_PAINT {
                let mut paint = PAINTSTRUCT::default();
                // SAFETY: paired paint calls on the fixture's own window/thread.
                let dc = unsafe { BeginPaint(handle, &mut paint) };
                if !dc.is_null() {
                    unsafe {
                        paint_pixels(dc);
                        EndPaint(handle, &paint);
                    }
                }
                0
            } else {
                unsafe { DefWindowProcW(handle, message, dc, data) }
            }
        }
        struct TestWindow {
            handle: HWND,
            class: Vec<u16>,
            module: windows_sys::Win32::Foundation::HINSTANCE,
        }
        impl TestWindow {
            fn close(&mut self) -> Result<(), ControlError> {
                if !self.handle.is_null() {
                    if unsafe { DestroyWindow(self.handle) } == 0 {
                        return Err(failure("destroy capture fixture"));
                    }
                    self.handle = null_mut();
                }
                if !self.class.is_empty() {
                    if unsafe { UnregisterClassW(self.class.as_ptr(), self.module) } == 0 {
                        return Err(failure("unregister capture fixture"));
                    }
                    self.class.clear();
                }
                Ok(())
            }
        }
        impl Drop for TestWindow {
            fn drop(&mut self) {
                crate::common::error::best_effort_cleanup(|| self.close());
            }
        }
        let class: Vec<_> = "d2bot-rust-port-hidden-capture"
            .encode_utf16()
            .chain([0])
            .collect();
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let definition = WNDCLASSW {
            lpfnWndProc: Some(paint_fixture),
            hInstance: module,
            lpszClassName: class.as_ptr(),
            hbrBackground: unsafe { GetStockObject(WHITE_BRUSH) }.cast(),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&definition) } == 0 {
            return Err(failure("register hidden capture fixture"));
        }
        let mut owner = TestWindow {
            handle: null_mut(),
            class,
            module,
        };
        let monitors = window::monitors()?;
        let monitor = monitors
            .get(1)
            .ok_or_else(|| ControlError::Backend("fixture monitor unavailable".into()))?;
        let left = monitor.left + 32;
        let top = monitor.top + 32;
        let foreground = unsafe { GetForegroundWindow() };
        // The small owned fixture is painted before capture; NOACTIVATE/TOOLWINDOW
        // keep focus and taskbar ownership unchanged.
        owner.handle = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                owner.class.as_ptr(),
                owner.class.as_ptr(),
                WS_POPUP | WS_VISIBLE,
                left,
                top,
                16,
                16,
                null_mut(),
                null_mut(),
                module,
                std::ptr::null(),
            )
        };
        if owner.handle.is_null() {
            return Err(failure("create hidden capture fixture"));
        }
        unsafe {
            UpdateWindow(owner.handle);
        }
        // Allow the owned fixture's first compositor frame before the single capture.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(foreground, unsafe { GetForegroundWindow() });
        let info = WindowInfo {
            id: crate::platform::WindowId::new(owner.handle as usize)?,
            title: "synthetic fixture".into(),
            target: PhysicalTarget::new(left, top, 16, 16)
                .map_err(|e| ControlError::Backend(e.to_string()))?,
        };
        let frame = Windows::new()?
            .capture_window(&info)?
            .ok_or_else(|| ControlError::Backend("fixture window capture failed".into()))?;
        assert_eq!(info.target.size(), frame.size());
        for (index, pixel) in frame.pixels().as_chunks::<3>().0.iter().enumerate() {
            assert_eq!(
                *pixel,
                if index < 16 * 8 {
                    [0xa9, 0x55, 0x11]
                } else {
                    [0x22, 0x44, 0x77]
                }
            );
        }
        assert_eq!(foreground, unsafe { GetForegroundWindow() });
        owner.close()?;
        Ok(())
    }
    #[test]
    #[ignore = "explicit read-only native capture and GDI resource accounting"]
    fn native_gdi_resources_are_balanced() -> Result<(), ControlError> {
        window::enable_dpi_awareness()?;
        use windows_sys::Win32::System::Threading::{
            GR_GDIOBJECTS, GetCurrentProcess, GetGuiResources,
        };
        let desktop = window::desktop()?;
        let region = PhysicalTarget::new(desktop.left, desktop.top, 8, 8)
            .map_err(|e| ControlError::Backend(e.to_string()))?;
        let mut backend = Windows::new()?;
        // Warm up any process-local Windows bookkeeping before counting our resources.
        backend.capture_screen(region)?;
        let mut anchor = Surface::new(null_mut(), region.size())?
            .ok_or_else(|| ControlError::Backend("GDI accounting anchor unavailable".into()))?;
        let process = unsafe { GetCurrentProcess() };
        let before = unsafe { GetGuiResources(process, GR_GDIOBJECTS) };
        assert!(
            before >= 2,
            "GDI resource query did not see the live DC/bitmap"
        );
        for _ in 0..64 {
            backend.capture_screen(region)?;
        }
        let after = unsafe { GetGuiResources(process, GR_GDIOBJECTS) };
        assert_eq!(before, after, "capture leaked a GDI object");
        anchor.close()?;
        Ok(())
    }
}
