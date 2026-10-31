//! OS 콜백은 원자적 상태만 변경하고 실제 취소는 별도 소유 worker가 수행한다.
use super::failure;
use crate::common::{
    error::ControlError,
    workers::{Cancellation, WorkerGroup},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler};

static OWNED: AtomicBool = AtomicBool::new(false);
static NEXT: AtomicU64 = AtomicU64::new(2);
static STATE: AtomicU64 = AtomicU64::new(0);
fn signal(kind: u32) -> i32 {
    if !matches!(kind, CTRL_C_EVENT | CTRL_BREAK_EVENT) {
        return 0;
    }
    let state = STATE.load(Ordering::Acquire);
    if state == 0 {
        return 0;
    }
    // 이전 등록의 늦은 콜백은 다음 등록을 취소할 수 없다.
    let _ = STATE.compare_exchange(state, state | 1, Ordering::AcqRel, Ordering::Acquire);
    1
}
unsafe extern "system" fn handler(kind: u32) -> i32 {
    signal(kind)
}

pub struct ConsoleInterrupt {
    workers: WorkerGroup,
    registered: bool,
    epoch: u64,
}
impl ConsoleInterrupt {
    pub fn start(stop: Cancellation) -> Result<Self, ControlError> {
        Self::with_callback(Arc::new(move || {
            stop.cancel();
            Ok(())
        }))
    }
    pub fn with_callback(
        callback: Arc<dyn Fn() -> Result<(), ControlError> + Send + Sync>,
    ) -> Result<Self, ControlError> {
        if OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ControlError::Runtime(
                "console interrupt handler already owned".into(),
            ));
        }
        let epoch = match NEXT.try_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(2))
        {
            Ok(epoch) => epoch,
            Err(_) => {
                OWNED.store(false, Ordering::Release);
                return Err(ControlError::Runtime(
                    "console interrupt generation exhausted".into(),
                ));
            }
        };
        STATE.store(epoch, Ordering::Release);
        // SAFETY: 정적 함수 포인터는 등록 수명 전체에 유효하며 OS 스레드에서 원자 연산만 한다.
        if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
            STATE.store(0, Ordering::Release);
            OWNED.store(false, Ordering::Release);
            return Err(failure("SetConsoleCtrlHandler register"));
        }
        let mut owner = Self {
            workers: WorkerGroup::new(Cancellation::default(), 1),
            registered: true,
            epoch,
        };
        owner.workers.spawn("d2-console-interrupt", move |token| {
            while !token.is_cancelled() {
                if STATE.load(Ordering::Acquire) == epoch | 1 {
                    callback()?;
                    break;
                }
                token.wait(Duration::from_millis(5))?;
            }
            Ok(())
        })?;
        Ok(owner)
    }
    pub fn interrupted(&self) -> bool {
        STATE.load(Ordering::Acquire) == (self.epoch | 1)
    }
    pub fn close(&mut self) -> Result<(), ControlError> {
        let stopped = self.workers.finish(Duration::from_secs(1));
        if self.registered {
            // SAFETY: 이 소유자만 등록을 제거하며 함수/정적 상태는 제거 뒤에도 유효하다.
            if unsafe { SetConsoleCtrlHandler(Some(handler), 0) } == 0 {
                return Err(failure("SetConsoleCtrlHandler unregister"));
            }
            self.registered = false;
            if STATE.load(Ordering::Acquire) & !1 == self.epoch {
                STATE.store(0, Ordering::Release);
            }
            OWNED.store(false, Ordering::Release);
        }
        stopped
    }
}
impl Drop for ConsoleInterrupt {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| self.close());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_only_signals_owned_generation() -> Result<(), ControlError> {
        let _serial = super::super::CONSOLE_TEST_LOCK
            .lock()
            .map_err(|_| ControlError::Runtime("console test lock poisoned".into()))?;
        // 실제 콘솔 이벤트나 키를 발생시키지 않고 등록·콜백·worker 정리만 확인한다.
        let stop = Cancellation::default();
        let mut owner = ConsoleInterrupt::start(stop.clone())?;
        assert!(!owner.interrupted());
        assert!(ConsoleInterrupt::start(Cancellation::default()).is_err());
        assert_eq!(signal(CTRL_C_EVENT), 1);
        assert!(owner.interrupted());
        assert!(stop.wait(Duration::from_secs(1))?);
        owner.close()?;
        owner.close()?;
        assert_eq!(signal(CTRL_C_EVENT), 0);
        assert_eq!(signal(2), 0);
        let next = Cancellation::default();
        let mut owner = ConsoleInterrupt::start(next.clone())?;
        assert!(!next.is_cancelled());
        assert_eq!(signal(CTRL_BREAK_EVENT), 1);
        assert!(next.wait(Duration::from_secs(1))?);
        owner.close()?;
        Ok(())
    }
}
