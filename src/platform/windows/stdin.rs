//! 감독 JSON 행의 읽기 작업만 소유한다. 읽기 취소 완료와 스레드 종료를 따로 확인한다.
use crate::common::{error::ControlError, workers::Cancellation};
use std::{
    io::{self, BufRead, Read},
    os::windows::io::AsRawHandle,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::ERROR_NOT_FOUND, System::IO::CancelSynchronousIo};
static STDIN_OWNED: AtomicBool = AtomicBool::new(false);

pub struct StdinLines {
    worker: Option<JoinHandle<()>>,
    stop: Cancellation,
    receiver: Receiver<Result<String, String>>,
    stdio_reserved: bool,
}
impl StdinLines {
    pub fn start(maximum: usize) -> Result<Self, ControlError> {
        if STDIN_OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ControlError::Runtime(
                "supervised stdin reader already owned".into(),
            ));
        }
        match Self::reader(|| io::stdin().lock(), maximum) {
            Ok(mut reader) => {
                reader.stdio_reserved = true;
                Ok(reader)
            }
            Err(error) => {
                STDIN_OWNED.store(false, Ordering::Release);
                Err(error)
            }
        }
    }
    fn reader<R: BufRead>(
        factory: impl FnOnce() -> R + Send + 'static,
        maximum: usize,
    ) -> Result<Self, ControlError> {
        if !(1..=32).contains(&maximum) {
            return Err(ControlError::Runtime(
                "one to thirty-two supervised signals required".into(),
            ));
        }
        let (sender, receiver) = mpsc::sync_channel(maximum + 1);
        let stop = Cancellation::default();
        let token = stop.clone();
        let worker = thread::Builder::new()
            .name("d2-probe-stdin".into())
            .spawn(move || {
                let mut reader = factory();
                for _ in 0..maximum {
                    if token.is_cancelled() {
                        break;
                    }
                    let mut line = String::new();
                    // UTF-8 한 글자는 최대4바이트다. 잘못된 긴 입력도 무제한 할당하지 않는다.
                    let read = reader.by_ref().take(4 * 4096 + 3).read_line(&mut line);
                    if token.is_cancelled() {
                        break;
                    }
                    let result = match read {
                        Ok(0) => Err("EOF when reading a line".into()),
                        Ok(_) => {
                            if line.ends_with('\n') {
                                line.pop();
                                if line.ends_with('\r') {
                                    line.pop();
                                }
                            }
                            if line.chars().count() > 4096 {
                                Err("probe signal exceeds 4096 characters".into())
                            } else {
                                Ok(line)
                            }
                        }
                        Err(error) => Err(error.to_string()),
                    };
                    let failed = result.is_err();
                    if sender.try_send(result).is_err() || failed {
                        break;
                    }
                }
            })
            .map_err(|e| {
                ControlError::Runtime(format!("cannot start supervised stdin reader: {e}"))
            })?;
        Ok(Self {
            worker: Some(worker),
            stop,
            receiver,
            stdio_reserved: false,
        })
    }
    pub fn try_read(&self) -> Result<Option<String>, ControlError> {
        match self.receiver.try_recv() {
            Ok(Ok(line)) => Ok(Some(line)),
            Ok(Err(error)) => Err(ControlError::Runtime(error)),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => Ok(None),
        }
    }
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn close(&mut self, timeout: Duration) -> Result<(), ControlError> {
        self.stop.cancel();
        let start = Instant::now();
        let mut cancel_error = None;
        if let Some(worker) = &self.worker {
            while !worker.is_finished() {
                // SAFETY: JoinHandle가 이 전용 읽기 스레드의 핸들을 계속 소유한다.
                // 취소 대상은 해당 스레드의 현재 I/O뿐이며 완료 전에 join/핸들 해제를 하지 않는다.
                if unsafe { CancelSynchronousIo(worker.as_raw_handle()) } == 0 {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() != Some(ERROR_NOT_FOUND as i32)
                        && cancel_error.is_none()
                    {
                        cancel_error = Some(error);
                    }
                }
                // read 직전/취소 사이의 경합도 다음 반복에서 같은 소유 스레드에 다시 전달한다.
                if start.elapsed() >= timeout {
                    return Err(ControlError::Runtime(
                        "stdin reader still stopping; thread retained for another close attempt"
                            .into(),
                    ));
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            if self.stdio_reserved {
                STDIN_OWNED.store(false, Ordering::Release);
                self.stdio_reserved = false;
            }
            return Err(ControlError::Runtime(
                "supervised stdin reader panicked".into(),
            ));
        }
        if self.stdio_reserved {
            STDIN_OWNED.store(false, Ordering::Release);
            self.stdio_reserved = false;
        }
        if let Some(error) = cancel_error {
            return Err(ControlError::Runtime(format!(
                "stdin cancellation failed: {error}"
            )));
        }
        Ok(())
    }
}
impl Drop for StdinLines {
    fn drop(&mut self) {
        crate::common::error::best_effort_cleanup(|| self.close(Duration::from_secs(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::File,
        io::{BufReader, Cursor},
        os::windows::io::FromRawHandle,
        ptr::null_mut,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    #[test]
    fn bounded_unicode_eof_and_owned_blocked_pipe_cancellation() -> Result<(), ControlError> {
        let mut reader = StdinLines::reader(|| Cursor::new("한글😀\r\nsecond\n".as_bytes()), 3)?;
        let start = Instant::now();
        let mut lines = vec![];
        let mut eof = false;
        while start.elapsed() < Duration::from_secs(1) {
            match reader.try_read() {
                Ok(Some(line)) => lines.push(line),
                Err(_) => {
                    eof = true;
                    break;
                }
                _ => thread::yield_now(),
            }
        }
        reader.close(Duration::from_secs(1))?;
        assert_eq!(lines, ["한글😀", "second"]);
        assert!(eof);
        let mut read = null_mut();
        let mut write = null_mut();
        // SAFETY: 두 초기화된 출력 핸들은 아래 File 소유자로 한 번씩 이전한다. 외부 stdin은 쓰지 않는다.
        if unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) } == 0 {
            return Err(super::super::failure("CreatePipe fixture"));
        }
        let input = unsafe { File::from_raw_handle(read) };
        let output = unsafe { File::from_raw_handle(write) };
        let mut reader = StdinLines::reader(move || BufReader::new(input), 1)?;
        thread::sleep(Duration::from_millis(20));
        assert!(!reader.is_finished());
        reader.close(Duration::from_secs(1))?;
        assert!(reader.is_finished());
        reader.close(Duration::ZERO)?;
        drop(output);
        let mut reader = StdinLines::reader(|| Cursor::new("a".repeat(4097).into_bytes()), 1)?;
        let start = Instant::now();
        let mut rejected = false;
        while start.elapsed() < Duration::from_secs(1) {
            if reader.try_read().is_err() {
                rejected = true;
                break;
            }
            thread::yield_now();
        }
        reader.close(Duration::from_secs(1))?;
        assert!(rejected);
        Ok(())
    }
}
