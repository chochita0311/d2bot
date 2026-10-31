use crate::common::{
    error::{ControlError, best_effort_cleanup},
    realtime::{CapturedFrame, ScreenSource},
    time::{self, MonotonicClock},
    workers::{Cancellation, CooperativeStop, WorkerGroup},
};
use std::{
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
pub struct ControlCapture {
    pub captured: CapturedFrame,
    pub sequence: u64,
    pub started_at: f64,
    pub completed_at: f64,
    pub method: Option<&'static str>,
}
struct Slot {
    latest: Option<ControlCapture>,
    error: Option<ControlError>,
    ended: bool,
    pending: bool,
    in_flight: bool,
}
struct Shared {
    slot: Mutex<Slot>,
    ready: Condvar,
}
/// producer 한 개와 대기 영상 한 개. 소비자가 가진 불변 Arc를 덮어쓰지 않는다.
pub struct LatestCapture {
    shared: Arc<Shared>,
    workers: WorkerGroup,
    stop: CooperativeStop,
    clock: Arc<dyn MonotonicClock>,
}
struct Source<C: ScreenSource>(Option<C>);
impl<C: ScreenSource> Source<C> {
    fn close(&mut self) -> Result<(), ControlError> {
        match self.0.take() {
            Some(mut c) => c.close(),
            None => Ok(()),
        }
    }
}
impl<C: ScreenSource> Drop for Source<C> {
    fn drop(&mut self) {
        best_effort_cleanup(|| self.close());
    }
}
impl LatestCapture {
    pub fn new<C: ScreenSource>(
        source: C,
        clock: Arc<dyn MonotonicClock>,
        stop: CooperativeStop,
    ) -> Result<Self, ControlError> {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot {
                latest: None,
                error: None,
                ended: false,
                pending: true,
                in_flight: false,
            }),
            ready: Condvar::new(),
        });
        let mut workers = WorkerGroup::new(Cancellation::default(), 1);
        let state = shared.clone();
        let timing = clock.clone();
        let cancellation = stop.clone();
        let mut source = Source(Some(source));
        workers.spawn("control-capture", move |local| {
            let mut sequence = 0_u64;
            let result = (|| {
                while !local.is_cancelled() && !cancellation.is_set() {
                    {
                        let mut slot = state
                            .slot
                            .lock()
                            .map_err(|_| ControlError::Runtime("capture slot poisoned".into()))?;
                        while !slot.pending && !local.is_cancelled() && !cancellation.is_set() {
                            let (next, _) = state
                                .ready
                                .wait_timeout(slot, Duration::from_millis(10))
                                .map_err(|_| {
                                    ControlError::Runtime("capture request wait poisoned".into())
                                })?;
                            slot = next;
                        }
                        if local.is_cancelled() || cancellation.is_set() {
                            break;
                        }
                        slot.pending = false;
                        slot.in_flight = true;
                    }
                    let cycle = Instant::now();
                    let started_at = timing.now();
                    time::nonnegative(started_at, "capture_started_at")?;
                    let captured = source
                        .0
                        .as_mut()
                        .ok_or_else(|| ControlError::Runtime("capture owner missing".into()))?
                        .grab()?;
                    let method = source.0.as_ref().and_then(ScreenSource::capture_method);
                    let completed_at = timing.now();
                    time::nonnegative(completed_at, "capture_completed_at")?;
                    if completed_at < started_at {
                        return Err(ControlError::Runtime(
                            "capture clock moved backwards".into(),
                        ));
                    }
                    sequence = sequence.checked_add(1).ok_or_else(|| {
                        ControlError::Runtime("capture sequence exhausted".into())
                    })?;
                    let old = {
                        let mut slot = state
                            .slot
                            .lock()
                            .map_err(|_| ControlError::Runtime("capture slot poisoned".into()))?;
                        let old = slot.latest.replace(ControlCapture {
                            captured,
                            sequence,
                            started_at,
                            completed_at,
                            method,
                        });
                        slot.in_flight = false;
                        state.ready.notify_all();
                        old
                    };
                    drop(old);
                    local.wait(
                        Duration::from_secs_f64(1.0 / 60.0).saturating_sub(cycle.elapsed()),
                    )?;
                }
                Ok(())
            })();
            let close = source.close();
            let result = match (result, close) {
                (Err(e), Err(c)) => Err(ControlError::Runtime(format!(
                    "{e}; capture close failed: {c}"
                ))),
                (Err(e), _) | (_, Err(e)) => Err(e),
                _ => Ok(()),
            };
            let failed = result.is_err();
            let mut slot = state
                .slot
                .lock()
                .map_err(|_| ControlError::Runtime("capture slot poisoned".into()))?;
            slot.error = result.err();
            slot.ended = true;
            slot.pending = false;
            slot.in_flight = false;
            slot.latest = None;
            state.ready.notify_all();
            drop(slot);
            if failed {
                cancellation.set();
            }
            Ok(())
        })?;
        Ok(Self {
            shared,
            workers,
            stop,
            clock,
        })
    }
    /// after가 있으면 그 요청 뒤 시작한 화면만 반환한다. 단발 포인터 후 재캡처에 사용한다.
    pub fn grab(&self, after: Option<f64>) -> Result<ControlCapture, ControlError> {
        if let Some(at) = after {
            time::nonnegative(at, "capture_requested_at")?;
        }
        let waiting = Instant::now();
        let mut slot = self
            .shared
            .slot
            .lock()
            .map_err(|_| ControlError::Runtime("capture slot poisoned".into()))?;
        loop {
            if let Some(error) = slot.error.take() {
                return Err(error);
            }
            if self.stop.is_set() || slot.ended {
                return Err(ControlError::Runtime("control capture stopped".into()));
            }
            if let Some(frame) = slot.latest.as_ref() {
                let now = self.clock.now();
                time::nonnegative(now, "capture_consumer_at")?;
                if after.is_none_or(|at| frame.started_at >= at)
                    && (0.0..=0.04).contains(&(now - frame.completed_at))
                {
                    return slot
                        .latest
                        .take()
                        .ok_or_else(|| ControlError::Runtime("capture slot missing".into()));
                }
            }
            if self.workers.is_finished() {
                return Err(ControlError::Runtime(format!(
                    "control capture ended: {:?}",
                    self.workers.faults()?
                )));
            }
            if !slot.in_flight {
                slot.pending = true;
                self.shared.ready.notify_all();
            }
            if waiting.elapsed() >= Duration::from_secs(2) {
                return Err(ControlError::Runtime(
                    "control capture wait timed out".into(),
                ));
            }
            let (next, _) = self
                .shared
                .ready
                .wait_timeout(slot, Duration::from_millis(10))
                .map_err(|_| ControlError::Runtime("capture wait poisoned".into()))?;
            slot = next;
        }
    }
    pub fn finish(&mut self, timeout: Duration) -> Result<(), ControlError> {
        self.workers.finish(timeout)?;
        let mut slot = self
            .shared
            .slot
            .lock()
            .map_err(|_| ControlError::Runtime("capture slot poisoned".into()))?;
        slot.latest = None;
        if let Some(error) = slot.error.take() {
            return Err(error);
        }
        let faults = self.workers.faults()?;
        if !faults.is_empty() {
            return Err(ControlError::Runtime(format!(
                "capture worker failed: {faults:?}"
            )));
        }
        Ok(())
    }
    pub fn is_finished(&self) -> bool {
        self.workers.is_finished()
    }
}
impl Drop for LatestCapture {
    fn drop(&mut self) {
        best_effort_cleanup(|| self.finish(Duration::from_millis(100)));
    }
}
