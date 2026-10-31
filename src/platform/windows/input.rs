use super::{Windows, failure, window};
use crate::{
    common::{error::ControlError, frame_geometry::PhysicalTarget},
    platform::{InputEvent, InputSystem, WindowId},
};
use windows_sys::Win32::{
    Foundation::POINT,
    UI::{
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, IsIconic},
    },
};

impl InputSystem for Windows {
    fn focused(&self, id: WindowId) -> Result<bool, ControlError> {
        // SAFETY: querying a non-owning identity never dereferences it in Rust.
        Ok(unsafe { GetForegroundWindow() } == window::hwnd(id))
    }
    fn minimized(&self, id: WindowId) -> Result<bool, ControlError> {
        // SAFETY: 소유하지 않는 창 식별자를 조회하며 포인터를 역참조하지 않는다.
        Ok(unsafe { IsIconic(window::hwnd(id)) } != 0)
    }
    fn bounds(&self, id: WindowId) -> Result<PhysicalTarget, ControlError> {
        window::bounds(id)
    }
    fn desktop(&self) -> Result<PhysicalTarget, ControlError> {
        window::desktop()
    }
    fn pointer(&self) -> Result<[i32; 2], ControlError> {
        let mut point = POINT::default();
        // SAFETY: output points to a live POINT.
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return Err(failure("GetCursorPos"));
        }
        Ok([point.x, point.y])
    }
    fn numlock_state(&self) -> Result<bool, ControlError> {
        Ok(unsafe { GetKeyState(VK_NUMLOCK as i32) } != 0)
    }
    fn arrow_scan(&self, virtual_key: u32) -> Result<u16, ControlError> {
        Ok(unsafe { MapVirtualKeyW(virtual_key, MAPVK_VK_TO_VSC) } as u16)
    }
    fn send(&self, event: InputEvent) -> Result<u32, ControlError> {
        let event = match event {
            InputEvent::Keyboard { scan, up, extended } => INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: 0,
                        wScan: scan,
                        dwFlags: KEYEVENTF_SCANCODE
                            | if up { KEYEVENTF_KEYUP } else { 0 }
                            | if extended { KEYEVENTF_EXTENDEDKEY } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            InputEvent::Mouse { dx, dy, flags } => INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dy,
                        mouseData: 0,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        };
        // SAFETY: one fully initialized INPUT of the exact SDK ABI size is live through the call.
        Ok(unsafe { SendInput(1, &event, size_of::<INPUT>() as i32) })
    }
}
