//! 판독용 영상과 분리한 진단 PNG 저장. 대기 두 장 외에는 픽셀을 보관하지 않는다.
use super::{EncounterEvidence, RunError};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

const WAITING_IMAGES: usize = 2;

#[derive(Default, Clone, Serialize)]
pub(super) struct StreamReport {
    pub queued: u64,
    pub written: u64,
    pub skipped_files: Vec<String>,
    pub errors: Vec<String>,
}
struct Shared {
    sender: Option<SyncSender<EncounterEvidence>>,
    report: StreamReport,
}
#[derive(Clone)]
pub(super) struct EvidenceChannel {
    shared: Arc<Mutex<Shared>>,
    prefix: String,
}
pub(super) struct EvidenceStream {
    shared: Arc<Mutex<Shared>>,
    worker: Option<JoinHandle<()>>,
}
impl EvidenceStream {
    pub fn new(directory: PathBuf) -> Result<Self, RunError> {
        Self::start(move |item| super::output::save_frame_file(&directory, item))
    }
    fn start(
        mut save: impl FnMut(&EncounterEvidence) -> Result<(), RunError> + Send + 'static,
    ) -> Result<Self, RunError> {
        let (sender, receiver) = mpsc::sync_channel::<EncounterEvidence>(WAITING_IMAGES);
        let shared = Arc::new(Mutex::new(Shared {
            sender: Some(sender),
            report: StreamReport::default(),
        }));
        let state = shared.clone();
        let worker = thread::Builder::new()
            .name("summoner-evidence-png".into())
            .spawn(move || {
                for item in receiver {
                    let result = save(&item);
                    // 파일 I/O가 끝난 즉시 큰 픽셀을 버리고 작은 결과만 기록한다.
                    let name = item.filename.clone();
                    drop(item);
                    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                    match result {
                        Ok(()) => state.report.written = state.report.written.saturating_add(1),
                        Err(e) => state.report.errors.push(format!("{name}: {e}")),
                    }
                }
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    pub fn channel(&self, prefix: &str) -> EvidenceChannel {
        EvidenceChannel {
            shared: self.shared.clone(),
            prefix: prefix.into(),
        }
    }
    pub fn finish(&mut self) -> StreamReport {
        // 채널 clone의 수명과 무관하게 송신을 먼저 닫고, 잠금 밖에서 writer를 합류한다.
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sender
            .take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            self.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .report
                .errors
                .push("PNG writer panicked".into());
        }
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .report
            .clone()
    }
}
impl Drop for EvidenceStream {
    fn drop(&mut self) {
        self.finish();
    }
}
impl EvidenceChannel {
    pub fn drain(&self, frames: &mut Vec<EncounterEvidence>) -> Vec<String> {
        let mut names = Vec::with_capacity(frames.len());
        for mut item in frames.drain(..) {
            let name = item.filename.clone();
            item.filename.insert_str(0, &self.prefix);
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let result = shared.sender.as_ref().map(|sender| sender.try_send(item));
            match result {
                Some(Ok(())) => {
                    shared.report.queued = shared.report.queued.saturating_add(1);
                    names.push(name);
                }
                Some(Err(TrySendError::Full(item))) => {
                    shared.report.skipped_files.push(item.filename)
                }
                Some(Err(TrySendError::Disconnected(item))) => shared
                    .report
                    .errors
                    .push(format!("{}: PNG writer disconnected", item.filename)),
                None => shared
                    .report
                    .errors
                    .push("PNG writer already closed".into()),
            }
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::frame::{BgrFrame, Size};
    use std::time::Duration;

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
    fn item(name: &str) -> TestResult<(EncounterEvidence, std::sync::Weak<BgrFrame>)> {
        let frame = Arc::new(BgrFrame::zeros(Size::new(4, 4)?)?);
        let weak = Arc::downgrade(&frame);
        Ok((
            EncounterEvidence {
                filename: name.into(),
                frame,
            },
            weak,
        ))
    }
    #[test]
    fn full_writer_queue_releases_diagnostics_without_waiting_for_disk() -> TestResult {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut first = true;
        let mut stream = EvidenceStream::start(move |_| {
            if first {
                first = false;
                entered_tx
                    .send(())
                    .map_err(|e| RunError::Runtime(e.to_string()))?;
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|e| RunError::Runtime(e.to_string()))?;
            }
            Ok(())
        })?;
        let channel = stream.channel("phase-00-");
        let (one, weak_one) = item("one.png")?;
        assert_eq!(channel.drain(&mut vec![one]), ["one.png"]);
        entered_rx.recv_timeout(Duration::from_secs(2))?;
        let (two, weak_two) = item("two.png")?;
        let (three, weak_three) = item("three.png")?;
        let (four, weak_four) = item("four.png")?;
        assert_eq!(
            channel.drain(&mut vec![two, three, four]),
            ["two.png", "three.png"]
        );
        assert!(weak_four.upgrade().is_none());
        assert!(
            weak_one.upgrade().is_some()
                && weak_two.upgrade().is_some()
                && weak_three.upgrade().is_some()
        );
        release_tx.send(())?;
        let report = stream.finish();
        assert_eq!((report.queued, report.written), (3, 3));
        assert_eq!(report.skipped_files, ["phase-00-four.png"]);
        assert!(report.errors.is_empty());
        for weak in [weak_one, weak_two, weak_three] {
            assert!(weak.upgrade().is_none());
        }
        // 남은 channel clone이 있어도 finish가 끝나며 닫힌 writer에는 전송하지 않는다.
        let (late, weak_late) = item("late.png")?;
        channel.drain(&mut vec![late]);
        assert!(weak_late.upgrade().is_none());
        assert_eq!(stream.finish().errors, ["PNG writer already closed"]);
        Ok(())
    }
    #[test]
    fn write_errors_are_reported_and_pixels_are_released() -> TestResult {
        let mut stream =
            EvidenceStream::start(|_| Err(RunError::Runtime("disk unavailable".into())))?;
        let (frame, weak) = item("failed.png")?;
        stream.channel("phase-01-").drain(&mut vec![frame]);
        let report = stream.finish();
        assert_eq!((report.queued, report.written), (1, 0));
        assert!(report.errors[0].contains("phase-01-failed.png: disk unavailable"));
        assert!(weak.upgrade().is_none());
        Ok(())
    }
}
