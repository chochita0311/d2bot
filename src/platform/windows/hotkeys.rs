//! Same system keyboard-listener boundary as the existing Python keyboard library.
//! The hook never enters the game process and always forwards events unchanged.
use super::failure;
use crate::common::{
    error::ControlError,
    hotkeys::{HotkeyAction, HotkeyBinding, HotkeyTracker},
    workers::{Cancellation, WorkerFault, WorkerGroup},
};
use std::{
    cell::RefCell,
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::GetAsyncKeyState, WindowsAndMessaging::*},
};

struct Context {
    tracker: HotkeyTracker,
    sender: SyncSender<HotkeyAction>,
    failed: Arc<AtomicBool>,
}
thread_local! { static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) }; }
unsafe extern "system" fn keyboard(code: i32, message: WPARAM, data: LPARAM) -> LRESULT {
    if code >= 0
        && matches!(
            message as u32,
            WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP
        )
        && data != 0
    {
        let result = std::panic::catch_unwind(|| {
            CONTEXT.with(|slot| {
                if let Some(context) = slot.borrow_mut().as_mut() {
                    // SAFETY: Windows provides a KBDLLHOOKSTRUCT valid for this callback.
                    let event = unsafe { &*(data as *const KBDLLHOOKSTRUCT) };
                    let key = event.vkCode;
                    // Match the installed keyboard backend's VK_PACKET/injected-Alt filter.
                    if key == 0xe7 || event.flags & 0x30 == 0x30 {
                        return;
                    }
                    if let Some(action) = context
                        .tracker
                        .event(key, matches!(message as u32, WM_KEYDOWN | WM_SYSKEYDOWN))
                        && let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) =
                            context.sender.try_send(action)
                    {
                        context.failed.store(true, Ordering::Release);
                    }
                }
            })
        });
        if result.is_err() {
            CONTEXT.with(|slot| {
                if let Ok(context) = slot.try_borrow()
                    && let Some(context) = &*context
                {
                    context.failed.store(true, Ordering::Release);
                }
            });
        }
    }
    // SAFETY: forward the OS-owned payload untouched, regardless of recognition/error.
    unsafe { CallNextHookEx(null_mut(), code, message, data) }
}
struct Hook(HHOOK);
impl Hook {
    fn close(&mut self) -> Result<(), ControlError> {
        if !self.0.is_null() {
            // SAFETY: only this message-pumping thread invokes its low-level callback.
            if unsafe { UnhookWindowsHookEx(self.0) } == 0 {
                return Err(failure("UnhookWindowsHookEx"));
            }
            self.0 = null_mut();
        }
        CONTEXT.with(|slot| *slot.borrow_mut() = None);
        Ok(())
    }
}
impl Drop for Hook {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| self.close());
    }
}
type Callback = Arc<dyn Fn(HotkeyAction) -> Result<(), ControlError> + Send + Sync>;
struct FailureStop {
    expected: Arc<AtomicBool>,
    callback: Callback,
    finished: bool,
}
impl FailureStop {
    fn finish(&mut self) -> Result<(), ControlError> {
        self.finished = true;
        if self.expected.load(Ordering::Acquire) {
            Ok(())
        } else {
            (self.callback)(HotkeyAction::Stop)
        }
    }
}
impl Drop for FailureStop {
    fn drop(&mut self) {
        if !self.finished {
            crate::common::error::best_effort_cleanup(|| self.finish());
        }
    }
}
fn combine(
    first: Result<(), ControlError>,
    cleanup: Result<(), ControlError>,
) -> Result<(), ControlError> {
    match (first, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(ControlError::Runtime(format!(
            "{error}; listener cleanup failed: {cleanup}"
        ))),
    }
}
pub struct HotkeyListener {
    cancellation: Cancellation,
    workers: WorkerGroup,
    expected_stop: Arc<AtomicBool>,
}
impl HotkeyListener {
    /// Callback runs on a separate worker, never inside the system listener.
    pub fn start(
        bindings: Vec<HotkeyBinding>,
        callback: Arc<dyn Fn(HotkeyAction) -> Result<(), ControlError> + Send + Sync>,
    ) -> Result<Self, ControlError> {
        let mut tracker = HotkeyTracker::new(bindings)?;
        let cancellation = Cancellation::default();
        let failed = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::sync_channel(64);
        let (ready, started) = mpsc::sync_channel(1);
        let mut owner = Self {
            workers: WorkerGroup::new(cancellation.clone(), 2),
            cancellation,
            expected_stop: Arc::new(AtomicBool::new(false)),
        };
        let listener_failure = failed.clone();
        owner.workers.spawn("d2-hotkeys-listener", move |token| {
            // Seed modifiers already held before startup. No keys are generated.
            for key in [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c] {
                tracker.seed(key, unsafe { GetAsyncKeyState(key as i32) } < 0);
            }
            CONTEXT.with(|slot| {
                *slot.borrow_mut() = Some(Context {
                    tracker,
                    sender,
                    failed: listener_failure.clone(),
                })
            });
            // SAFETY: WH_KEYBOARD_LL callback is local code invoked on this message-pumping
            // thread. No module is injected and no HWND/process handle is used.
            let module = unsafe { GetModuleHandleW(std::ptr::null()) };
            let raw = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), module, 0) };
            let mut hook = Hook(raw);
            if hook.0.is_null() {
                let _ = ready.send(false);
                return Err(failure("SetWindowsHookExW keyboard listener"));
            }
            let _ = ready.send(true);
            let result = (|| {
                let mut message = MSG::default();
                while !token.is_cancelled() {
                    // PeekMessage services low-level hook callbacks without a blocking GetMessage,
                    // so cancellation/error in the callback worker can also stop this thread.
                    while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
                        if message.message == WM_QUIT {
                            return Ok(());
                        }
                        unsafe {
                            TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    if listener_failure.load(Ordering::Acquire) {
                        return Err(ControlError::Runtime(
                            "hotkey listener queue overflow or callback failure".into(),
                        ));
                    }
                    token.wait(Duration::from_millis(5))?;
                }
                Ok(())
            })();
            combine(result, hook.close())
        })?;
        if started.recv_timeout(Duration::from_secs(1)) != Ok(true) {
            owner.stop(Duration::from_secs(1))?;
            return Err(ControlError::Runtime(
                "hotkey listener could not start".into(),
            ));
        }
        let expected = owner.expected_stop.clone();
        owner.workers.spawn("d2-hotkeys-actions", move |token| {
            let mut cleanup = FailureStop {
                expected,
                callback: callback.clone(),
                finished: false,
            };
            let result = (|| {
                while !token.is_cancelled() {
                    if failed.load(Ordering::Acquire) {
                        return Err(ControlError::Runtime(
                            "hotkey event delivery failed; stop requested".into(),
                        ));
                    }
                    match receiver.recv_timeout(Duration::from_millis(5)) {
                        Ok(action) => callback(action)?,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            if !token.is_cancelled() {
                                return Err(ControlError::Runtime(
                                    "hotkey listener disconnected".into(),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            })();
            combine(result, cleanup.finish())
        })?;
        Ok(owner)
    }
    pub fn faults(&self) -> Result<Vec<WorkerFault>, ControlError> {
        self.workers.faults()
    }
    pub fn stop(&mut self, timeout: Duration) -> Result<(), ControlError> {
        self.expected_stop.store(true, Ordering::Release);
        self.cancellation.cancel();
        self.workers.finish(timeout)
    }
}
impl Drop for HotkeyListener {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| self.stop(Duration::from_millis(100)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit native listener lifecycle; generates no keys and records no events"]
    fn native_listener_starts_and_unhooks_without_input() -> Result<(), ControlError> {
        for _ in 0..4 {
            let mut listener = HotkeyListener::start(
                vec![HotkeyBinding::parse("f10", HotkeyAction::Stop)?],
                Arc::new(|_| Ok(())),
            )?;
            listener.stop(Duration::from_secs(1))?;
            assert!(listener.faults()?.is_empty());
            listener.stop(Duration::from_secs(1))?;
        }
        Ok(())
    }
}
