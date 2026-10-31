use super::{Cancellation, CooperativeStop, WorkerGroup};
use crate::common::{
    config::ConfigError,
    error::{ControlError, best_effort_cleanup},
};
use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, mpsc},
    time::Duration,
};
type Job = Box<dyn FnOnce() + Send>;
struct Queue {
    jobs: VecDeque<Job>,
    closing: bool,
}
struct Shared {
    queue: Mutex<Queue>,
    ready: Condvar,
    space: Condvar,
    capacity: usize,
}
/// 작업을 기다리는 동안 입력 감시는 별도 소유자에서 계속 실행된다.
pub struct ObservationTask<T> {
    result: mpsc::Receiver<Result<T, ControlError>>,
}
impl<T> ObservationTask<T> {
    /// 결과가 아직 없으면 기다리지 않는다. 완료 오류와 끊긴 producer는 숨기지 않는다.
    pub fn try_join(&self) -> Result<Option<T>, ControlError> {
        match self.result.try_recv() {
            Ok(result) => result.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(error) => Err(ControlError::Runtime(format!(
                "observation result unavailable: {error}"
            ))),
        }
    }
    pub fn join(self) -> Result<T, ControlError> {
        self.result.recv().map_err(|error| {
            ControlError::Runtime(format!("observation result unavailable: {error}"))
        })?
    }
}
/// 정해진 수의 영상 작업만 실행/대기시킨다. 종료는 이미 받은 작업을 모두 정리한다.
pub struct ObservationPool {
    shared: Arc<Shared>,
    workers: WorkerGroup,
    stop: CooperativeStop,
    internal: Cancellation,
}
impl ObservationPool {
    pub fn new(count: usize, capacity: usize, stop: CooperativeStop) -> Result<Self, ControlError> {
        if !(1..=4).contains(&count) || !(1..=12).contains(&capacity) {
            return Err(ConfigError::value(
                "one to four observation workers and one to twelve queued tasks required",
            )
            .into());
        }
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                jobs: VecDeque::new(),
                closing: false,
            }),
            ready: Condvar::new(),
            space: Condvar::new(),
            capacity,
        });
        let internal = Cancellation::default();
        let mut workers = WorkerGroup::new(internal.clone(), count);
        for index in 0..count {
            let shared = shared.clone();
            workers.spawn(&format!("observation-{index}"), move |local| {
                loop {
                    let job = {
                        let mut queue = shared.queue.lock().map_err(|_| {
                            ControlError::Runtime("observation queue lock poisoned".into())
                        })?;
                        loop {
                            if let Some(job) = queue.jobs.pop_front() {
                                shared.space.notify_all();
                                break Some(job);
                            }
                            if queue.closing || local.is_cancelled() {
                                break None;
                            }
                            let (next, _) = shared
                                .ready
                                .wait_timeout(queue, Duration::from_millis(20))
                                .map_err(|_| {
                                    ControlError::Runtime("observation queue wait poisoned".into())
                                })?;
                            queue = next;
                        }
                    };
                    let Some(job) = job else {
                        return Ok(());
                    };
                    job();
                }
            })?;
        }
        Ok(Self {
            shared,
            workers,
            stop,
            internal,
        })
    }
    pub fn submit<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<ObservationTask<T>, ControlError> {
        let (sender, result) = mpsc::sync_channel(1);
        let job = Box::new(move || {
            let outcome = catch_unwind(AssertUnwindSafe(work)).map_err(|payload| {
                let message = payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("non-string panic");
                ControlError::Runtime(format!("observation task panicked: {message}"))
            });
            // 소비자가 결과를 사용하지 않아도 작업과 그 프레임은 정상적으로 정리한다.
            let _ = sender.send(outcome);
        });
        let mut queue = self
            .shared
            .queue
            .lock()
            .map_err(|_| ControlError::Runtime("observation queue lock poisoned".into()))?;
        while queue.jobs.len() >= self.shared.capacity
            && !queue.closing
            && !self.stop.is_set()
            && !self.internal.is_cancelled()
        {
            let (next, _) = self
                .shared
                .space
                .wait_timeout(queue, Duration::from_millis(20))
                .map_err(|_| ControlError::Runtime("observation queue wait poisoned".into()))?;
            queue = next;
        }
        if queue.closing || self.internal.is_cancelled() {
            return Err(ControlError::Runtime("observation pool is closed".into()));
        }
        if self.stop.is_set() {
            return Err(ControlError::Runtime(
                "observation submission interrupted".into(),
            ));
        }
        queue.jobs.push_back(job);
        self.shared.ready.notify_one();
        Ok(ObservationTask { result })
    }
    pub fn queued(&self) -> Result<usize, ControlError> {
        Ok(self
            .shared
            .queue
            .lock()
            .map_err(|_| ControlError::Runtime("observation queue lock poisoned".into()))?
            .jobs
            .len())
    }
    pub fn is_finished(&self) -> bool {
        self.workers.is_finished()
    }
    pub fn finish(&mut self, timeout: Duration) -> Result<(), ControlError> {
        {
            let mut queue = self
                .shared
                .queue
                .lock()
                .map_err(|_| ControlError::Runtime("observation queue lock poisoned".into()))?;
            queue.closing = true;
            self.shared.ready.notify_all();
            self.shared.space.notify_all();
        }
        self.workers.finish(timeout)
    }
}
impl Drop for ObservationPool {
    fn drop(&mut self) {
        best_effort_cleanup(|| self.finish(Duration::from_millis(100)));
    }
}
