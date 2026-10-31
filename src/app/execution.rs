//! CLI 출력 실패와 창 닫기도 취소를 전달하고 실제 worker 종료를 확인한다.
use crate::{
    actions::{ActionEvent, EventLevel, runtime::ActionEvents},
    common::workers::Cancellation,
    core::PreviewFrames,
    platform::windows::ConsoleInterrupt,
    runs::RunError,
};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub type Emitter = Arc<dyn Fn(&str) + Send + Sync>;
struct Finished(Arc<AtomicBool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
struct CancelOnDrop(Cancellation);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn combine<T>(result: Result<T, RunError>, cleanup: Result<(), RunError>) -> Result<T, RunError> {
    match (result, cleanup) {
        (Ok(v), Ok(())) => Ok(v),
        (Err(e), Ok(())) | (Ok(_), Err(e)) => Err(e),
        (Err(e), Err(c)) => Err(e.with_cleanup(c)),
    }
}
fn print_events(
    events: ActionEvents,
    stop: Cancellation,
    finished: Arc<AtomicBool>,
    output: &mut (impl Write + Send),
) -> Result<(), RunError> {
    let _cancel_on_failure = CancelOnDrop(stop.clone());
    let mut error = None;
    loop {
        let done = finished.load(Ordering::Acquire);
        for event in events.drain() {
            if error.is_none()
                && let Err(e) = writeln!(output, "{}", event.message).and_then(|()| output.flush())
            {
                error = Some(e);
                stop.cancel();
            }
        }
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    error.map_or(Ok(()), |e| Err(e.into()))
}
pub fn run(
    output: &mut (impl Write + Send),
    preview: Option<PreviewFrames>,
    interrupt_exit: Option<u8>,
    work: impl FnOnce(Cancellation, Emitter) -> Result<u8, RunError> + Send + 'static,
) -> Result<u8, RunError> {
    let stop = Cancellation::default();
    let mut console = ConsoleInterrupt::start(stop.clone())?;
    let finished = Arc::new(AtomicBool::new(false));
    let events = ActionEvents::default();
    let sink = events.clone();
    let emitter: Emitter = Arc::new(move |text| {
        sink.push(ActionEvent {
            level: EventLevel::Info,
            message: text.into(),
        })
    });
    let worker_stop = stop.clone();
    let done = Finished(finished.clone());
    let task = std::thread::Builder::new()
        .name("d2-cli-session".into())
        .spawn(move || {
            let _done = done;
            work(worker_stop, emitter)
        })?;
    let (display, logs) = if let Some(frames) = preview {
        std::thread::scope(|scope| {
            let log_stop = stop.clone();
            let log_done = finished.clone();
            let logger = scope.spawn(move || print_events(events, log_stop, log_done, output));
            let _cancel_on_display_failure = CancelOnDrop(stop.clone());
            let display = crate::ui::preview::run(frames, stop.clone(), finished.clone());
            stop.cancel();
            let logs = logger
                .join()
                .unwrap_or_else(|_| Err(RunError::Runtime("CLI output worker panicked".into())));
            (display, logs)
        })
    } else {
        (Ok(()), print_events(events, stop.clone(), finished, output))
    };
    stop.cancel();
    let result = task
        .join()
        .unwrap_or_else(|_| Err(RunError::Runtime("CLI session worker panicked".into())));
    let interrupted = console.interrupted();
    combine(
        combine(combine(result, display), logs),
        console.close().map_err(Into::into),
    )
    .map(|code| {
        if interrupted {
            interrupt_exit.unwrap_or(code)
        } else {
            code
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broken_output_cancels_and_joins_session() -> Result<(), RunError> {
        let _serial = crate::platform::windows::CONSOLE_TEST_LOCK
            .lock()
            .map_err(|_| RunError::Runtime("console test lock poisoned".into()))?;
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("closed output"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let closed = Arc::new(AtomicBool::new(false));
        let done = closed.clone();
        let result = run(&mut Broken, None, Some(130), move |stop, emit| {
            emit("ready");
            while !stop.is_cancelled() {
                stop.wait(Duration::from_millis(10))?;
            }
            done.store(true, Ordering::Release);
            Ok(0)
        });
        assert!(result.is_err());
        assert!(closed.load(Ordering::Acquire));
        Ok(())
    }
}
