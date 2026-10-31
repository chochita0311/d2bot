use super::{FieldInputBackend, HeldFieldInput};
use crate::common::{
    error::{ControlError, best_effort_cleanup},
    time::{self, MonotonicClock},
    workers::{Cancellation, CooperativeStop, WorkerFault, WorkerGroup},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

type FocusCheck = Arc<dyn Fn() -> Result<bool, ControlError> + Send + Sync>;
pub struct InputSessionOptions {
    pub clock: Arc<dyn MonotonicClock>,
    pub stop: CooperativeStop,
    pub focused: FocusCheck,
    pub repeat_seconds: Option<f64>,
    pub aim_settle_seconds: f64,
}
/// 실행 주체가 정상 종료를 확인하고, 지연된 감시 작업의 소유권을 재시도 때까지 보존한다.
pub struct FieldInputSession<B: FieldInputBackend> {
    inputs: HeldFieldInput<B>,
    options: InputSessionOptions,
    workers: Option<WorkerGroup>,
    faults: Vec<WorkerFault>,
    finishing: bool,
    deadline: Arc<AtomicU64>,
}
impl<B: FieldInputBackend> FieldInputSession<B> {
    pub fn new(
        backend: Arc<B>,
        movement_key: &str,
        attack_key: &str,
        options: InputSessionOptions,
    ) -> Result<Self, ControlError> {
        let inputs = HeldFieldInput::new(
            backend,
            movement_key,
            attack_key,
            Some(options.clock.clone()),
            options.repeat_seconds,
            options.aim_settle_seconds,
        )?;
        Ok(Self {
            inputs,
            options,
            workers: None,
            faults: vec![],
            finishing: false,
            deadline: Arc::new(AtomicU64::new(0.0_f64.to_bits())),
        })
    }
    pub fn inputs(&self) -> &HeldFieldInput<B> {
        &self.inputs
    }
    pub fn clock(&self) -> &Arc<dyn MonotonicClock> {
        &self.options.clock
    }
    pub fn stop(&self) -> &CooperativeStop {
        &self.options.stop
    }
    pub fn focused(&self) -> Result<bool, ControlError> {
        (self.options.focused)()
    }
    pub fn workers_retained(&self) -> bool {
        self.workers.is_some()
    }
    pub fn faults(&self) -> Result<Vec<WorkerFault>, ControlError> {
        let mut faults = self.faults.clone();
        if let Some(workers) = &self.workers {
            faults.extend(workers.faults()?);
        }
        Ok(faults)
    }
    pub fn start(&mut self, name: &str, deadline: f64) -> Result<(), ControlError> {
        self.start_mode(name, deadline, true)
    }
    /// 버프 준비 구역의 기존 감시기는 반복 누름 없이 임대 만료만 처리한다.
    pub fn start_expiry_only(&mut self, name: &str, deadline: f64) -> Result<(), ControlError> {
        self.start_mode(name, deadline, false)
    }
    pub fn set_deadline(&self, deadline: f64) -> Result<(), ControlError> {
        time::nonnegative(deadline, "input session deadline")?;
        if self.finishing || self.inputs.closed() {
            return Err(ControlError::Runtime("input session is closing".into()));
        }
        self.deadline.store(deadline.to_bits(), Ordering::Release);
        Ok(())
    }
    fn start_mode(&mut self, name: &str, deadline: f64, repeat: bool) -> Result<(), ControlError> {
        time::nonnegative(deadline, "input session deadline")?;
        if self.finishing || self.workers.is_some() || self.inputs.closed() {
            return Err(ControlError::Runtime(
                "input session is already started or closed".into(),
            ));
        }
        let mut workers = WorkerGroup::new(Cancellation::default(), 1);
        self.set_deadline(deadline)?;
        let deadline = self.deadline.clone();
        let clock = self.options.clock.clone();
        let focused = self.options.focused.clone();
        let stop = self.options.stop.clone();
        let inputs = self.inputs.clone();
        workers.spawn(name, move |local| {
            let mut failure = StopOnFailure {
                stop: stop.clone(),
                inputs: inputs.clone(),
                complete: false,
            };
            while !local.wait(Duration::from_millis(20))? {
                let observed_deadline = deadline.load(Ordering::Acquire);
                if stop.is_set() || !focused()? {
                    inputs.release(false)?;
                } else if clock.now() >= f64::from_bits(observed_deadline) {
                    // 느린 포커스 확인 중 갱신한 버프를 이전 시한으로 해제하지 않는다.
                    if deadline.load(Ordering::Acquire) == observed_deadline {
                        inputs.release(false)?;
                    }
                } else if repeat {
                    inputs.service(clock.now())?;
                } else {
                    inputs.expire(clock.now())?;
                }
            }
            failure.complete = true;
            Ok(())
        })?;
        self.workers = Some(workers);
        Ok(())
    }
    pub fn finish(&mut self, timeout: Duration) -> Result<(), ControlError> {
        self.finishing = true;
        let joined = self
            .workers
            .as_mut()
            .map_or(Ok(()), |workers| workers.finish(timeout));
        // join이 늦어져도 해당 입력 소유자는 닫아 이후 누름을 막는다.
        let released = self.inputs.close();
        if joined.is_ok()
            && let Some(workers) = self.workers.take()
        {
            self.faults.extend(workers.faults()?);
        }
        match (joined, released) {
            (Err(join), Err(release)) => Err(ControlError::Runtime(format!(
                "{join}; input cleanup also failed: {release}"
            ))),
            (Err(error), _) | (_, Err(error)) => Err(error),
            _ => Ok(()),
        }
    }
}
impl<B: FieldInputBackend> Drop for FieldInputSession<B> {
    fn drop(&mut self) {
        best_effort_cleanup(|| self.finish(Duration::from_millis(100)));
    }
}
struct StopOnFailure<B: FieldInputBackend> {
    stop: CooperativeStop,
    inputs: HeldFieldInput<B>,
    complete: bool,
}
impl<B: FieldInputBackend> Drop for StopOnFailure<B> {
    fn drop(&mut self) {
        if !self.complete {
            self.stop.set();
            best_effort_cleanup(|| self.inputs.close());
        }
    }
}
