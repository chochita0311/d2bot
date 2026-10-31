//! 스냅샷의 캡처·파일 작업은 UI 밖에서 끝내고 종료할 때까지 소유한다.
use crate::common::{
    capture::CaptureSettings,
    workers::{Cancellation, WorkerGroup},
};
use crate::runs::RunError;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
pub type SnapshotEffect =
    Arc<dyn Fn(CaptureSettings, PathBuf, Cancellation) -> Result<PathBuf, RunError> + Send + Sync>;
type SharedResult = Arc<Mutex<Option<Result<PathBuf, String>>>>;
pub struct SnapshotSession {
    worker: Option<WorkerGroup>,
    stop: Cancellation,
    result: SharedResult,
}
impl Default for SnapshotSession {
    fn default() -> Self {
        Self {
            worker: None,
            stop: Cancellation::default(),
            result: Arc::new(Mutex::new(None)),
        }
    }
}
impl SnapshotSession {
    pub fn is_running(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished())
    }
    pub fn has_pending_work(&self) -> bool {
        // 스레드가 끝나도 UI가 결과를 수신하고 join할 때까지 세션을 보유한다.
        self.worker.is_some()
    }
    pub fn start(
        &mut self,
        settings: CaptureSettings,
        root: PathBuf,
        effect: SnapshotEffect,
    ) -> Result<(), RunError> {
        if self.has_pending_work() {
            return Err(RunError::Runtime(
                "Snapshot capture is already running.".into(),
            ));
        }
        self.stop = Cancellation::default();
        self.result = Arc::new(Mutex::new(None));
        let result = self.result.clone();
        let mut worker = WorkerGroup::new(self.stop.clone(), 1);
        worker.spawn("gui-snapshot", move |token| {
            let value = effect(settings, root, token).map_err(|e| e.to_string());
            *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
            Ok(())
        })?;
        self.worker = Some(worker);
        Ok(())
    }
    pub fn request_stop(&self) {
        self.stop.cancel();
    }
    pub fn take_finished(&mut self) -> Result<Option<Result<PathBuf, String>>, RunError> {
        if self.is_running() {
            return Ok(None);
        }
        let Some(worker) = &mut self.worker else {
            return Ok(None);
        };
        worker.finish(Duration::ZERO)?;
        let value = self
            .result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .unwrap_or_else(|| {
                let faults = worker
                    .faults()
                    .map(|v| {
                        v.into_iter()
                            .map(|f| f.message)
                            .collect::<Vec<_>>()
                            .join("; ")
                    })
                    .unwrap_or_else(|e| e.to_string());
                Err(faults)
            });
        self.worker = None;
        Ok(Some(value))
    }
}
