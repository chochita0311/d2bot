//! Owned, cancellable workers. Explicit finish reports a timeout; Drop is a fallback.
mod observations;
mod owned_observer;
pub use observations::{ObservationPool, ObservationTask};
pub(crate) use owned_observer::OwnedObserver;

use super::error::ControlError;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Signal {
    cancelled: AtomicBool,
    wait: Mutex<()>,
    wake: Condvar,
}
#[derive(Clone, Default)]
pub struct Cancellation(Arc<Signal>);
impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        let _guard = self.0.wait.lock();
        self.0.wake.notify_all();
    }
    pub fn wait(&self, duration: Duration) -> Result<bool, ControlError> {
        let deadline = Instant::now()
            .checked_add(duration)
            .ok_or_else(|| ControlError::Runtime("worker wait exceeds clock range".into()))?;
        let mut guard = self
            .0
            .wait
            .lock()
            .map_err(|_| ControlError::Runtime("cancellation wait lock poisoned".into()))?;
        while !self.is_cancelled() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (next, _) = self
                .0
                .wake
                .wait_timeout(guard, remaining)
                .map_err(|_| ControlError::Runtime("cancellation wait lock poisoned".into()))?;
            guard = next;
        }
        Ok(self.is_cancelled())
    }
}

/// 방별 정리는 local만 닫고, 사용자 중단은 전체 세션 신호에도 전달한다.
#[derive(Clone, Default)]
pub struct CooperativeStop {
    local: Cancellation,
    external: Option<Cancellation>,
}
impl CooperativeStop {
    pub fn new(external: Option<Cancellation>) -> Self {
        Self {
            local: Cancellation::default(),
            external,
        }
    }
    pub fn is_set(&self) -> bool {
        self.local.is_cancelled()
            || self
                .external
                .as_ref()
                .is_some_and(Cancellation::is_cancelled)
    }
    pub fn set(&self) {
        self.local.cancel();
    }
    pub fn request_stop(&self) {
        self.set();
        if let Some(external) = &self.external {
            external.cancel();
        }
    }
    pub fn wait(&self, duration: Duration) -> Result<bool, ControlError> {
        let end = Instant::now()
            .checked_add(duration)
            .ok_or_else(|| ControlError::Runtime("cooperative wait exceeds clock range".into()))?;
        while !self.is_set() {
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.local.wait(remaining.min(Duration::from_millis(20)))?;
        }
        Ok(self.is_set())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerFault {
    pub stage: String,
    pub message: String,
}
#[derive(Default)]
struct Completion {
    done: Mutex<usize>,
    wake: Condvar,
}
pub struct WorkerGroup {
    cancellation: Cancellation,
    capacity: usize,
    handles: Vec<JoinHandle<()>>,
    completion: Arc<Completion>,
    faults: Arc<Mutex<Vec<WorkerFault>>>,
    finishing: bool,
}
impl WorkerGroup {
    pub fn new(cancellation: Cancellation, capacity: usize) -> Self {
        Self {
            cancellation,
            capacity,
            handles: Vec::with_capacity(capacity),
            completion: Arc::new(Completion::default()),
            faults: Arc::new(Mutex::new(Vec::new())),
            finishing: false,
        }
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
    /// Completion means the actual owned threads have exited, including cleanup.
    pub fn is_finished(&self) -> bool {
        self.handles.iter().all(JoinHandle::is_finished)
    }
    pub fn faults(&self) -> Result<Vec<WorkerFault>, ControlError> {
        Ok(self
            .faults
            .lock()
            .map_err(|_| ControlError::Runtime("worker fault lock poisoned".into()))?
            .clone())
    }
    pub fn spawn(
        &mut self,
        name: &str,
        work: impl FnOnce(Cancellation) -> Result<(), ControlError> + Send + 'static,
    ) -> Result<(), ControlError> {
        if self.finishing || self.handles.len() >= self.capacity {
            return Err(ControlError::Runtime(
                "worker group is stopped or full".into(),
            ));
        }
        let token = self.cancellation.clone();
        let completed = self.completion.clone();
        let faults = self.faults.clone();
        let stage = name.to_owned();
        let handle = thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| work(token.clone())));
                let failure = match result {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error.to_string()),
                    Err(payload) => Some(format!(
                        "worker panicked: {}",
                        payload
                            .downcast_ref::<&str>()
                            .copied()
                            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                            .unwrap_or("non-string panic")
                    )),
                };
                if let Some(message) = failure {
                    let mut faults = match faults.lock() {
                        Ok(faults) => faults,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    faults.push(WorkerFault { stage, message });
                    drop(faults);
                    token.cancel();
                }
                let mut done = match completed.done.lock() {
                    Ok(done) => done,
                    Err(poisoned) => poisoned.into_inner(),
                };
                *done += 1;
                completed.wake.notify_all();
            })
            .map_err(|error| ControlError::Runtime(format!("cannot start {name}: {error}")))?;
        self.handles.push(handle);
        Ok(())
    }
    pub fn finish(&mut self, timeout: Duration) -> Result<(), ControlError> {
        self.finishing = true;
        self.cancellation.cancel();
        if self
            .handles
            .iter()
            .any(|handle| handle.thread().id() == thread::current().id())
        {
            return Err(ControlError::Runtime("worker cannot join itself".into()));
        }
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            ControlError::Runtime("worker stop timeout exceeds clock range".into())
        })?;
        let mut done = self
            .completion
            .done
            .lock()
            .map_err(|_| ControlError::Runtime("worker completion lock poisoned".into()))?;
        while *done < self.handles.len() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ControlError::Runtime("worker failed to stop within timeout; handles retained for another finish attempt".into()));
            }
            let (next, _) = self
                .completion
                .wake
                .wait_timeout(done, remaining)
                .map_err(|_| ControlError::Runtime("worker completion lock poisoned".into()))?;
            done = next;
        }
        drop(done);
        let mut failed = false;
        for handle in self.handles.drain(..) {
            if handle.join().is_err() {
                failed = true;
            }
        }
        if failed {
            return Err(ControlError::Runtime(
                "worker panicked outside its error boundary".into(),
            ));
        }
        Ok(())
    }
}
impl Drop for WorkerGroup {
    fn drop(&mut self) {
        super::error::best_effort_cleanup(|| self.finish(Duration::from_millis(100)));
    }
}
