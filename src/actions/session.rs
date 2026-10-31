//! One owned action worker. Cancellation does not claim that cleanup has ended.
mod launch;
use super::{ActionError, ActionEvent, EventLevel, runtime::ActionEvents};
use crate::common::{
    error::ControlError,
    workers::{Cancellation, WorkerGroup},
};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOutcome {
    Completed,
    Interrupted,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "result")]
pub enum SessionStatus {
    Idle,
    Running,
    Stopping,
    Finished(Result<SessionOutcome, String>),
}
type SharedResult = Arc<Mutex<Option<Result<SessionOutcome, String>>>>;
pub struct ActionSession {
    name: String,
    worker: Option<WorkerGroup>,
    cancellation: Cancellation,
    result: SharedResult,
    events: ActionEvents,
}
impl ActionSession {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            worker: None,
            cancellation: Cancellation::default(),
            result: Arc::new(Mutex::new(None)),
            events: ActionEvents::default(),
        }
    }
    pub fn events(&self) -> ActionEvents {
        self.events.clone()
    }
    pub fn is_running(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }
    pub fn start(
        &mut self,
        work: impl FnOnce(Cancellation, ActionEvents) -> Result<SessionOutcome, ActionError>
        + Send
        + 'static,
    ) -> Result<(), ActionError> {
        if self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            return Err(ActionError::runtime(format!(
                "{} is already running.",
                self.name
            )));
        }
        if let Some(previous) = &mut self.worker {
            previous.finish(Duration::ZERO)?;
        }
        let cancellation = Cancellation::default();
        let next_result = Arc::new(Mutex::new(None));
        let result = next_result.clone();
        let events = self.events.clone();
        let mut worker = WorkerGroup::new(cancellation.clone(), 1);
        worker.spawn(&self.name, move |token| {
            let outcome = (|| {
                let _compute = crate::common::compute::ComputeLease::acquire()?;
                work(token, events)
            })()
            .map_err(|e: ActionError| e.to_string());
            let mut state = result
                .lock()
                .map_err(|_| ControlError::Runtime("action result lock poisoned".into()))?;
            *state = Some(outcome);
            Ok(())
        })?;
        self.cancellation = cancellation;
        self.result = next_result;
        self.worker = Some(worker);
        Ok(())
    }
    pub fn request_stop(&self) {
        self.cancellation.cancel();
    }
    pub fn stop(&mut self, timeout: Duration) -> Result<(), ActionError> {
        self.request_stop();
        if let Some(worker) = &mut self.worker {
            worker.finish(timeout)?;
        }
        Ok(())
    }
    pub fn status(&self) -> Result<SessionStatus, ActionError> {
        let Some(worker) = &self.worker else {
            return Ok(SessionStatus::Idle);
        };
        if !worker.is_finished() {
            return Ok(if self.cancellation.is_cancelled() {
                SessionStatus::Stopping
            } else {
                SessionStatus::Running
            });
        }
        let result = self
            .result
            .lock()
            .map_err(|_| ActionError::runtime("action result lock poisoned"))?
            .clone();
        let result = match result {
            Some(result) => result,
            None => {
                let faults = worker.faults()?;
                Err(faults
                    .iter()
                    .map(|f| f.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "))
            }
        };
        Ok(SessionStatus::Finished(result))
    }
    /// Call from the owning UI/control loop; never from the action thread.
    pub fn stop_with_event(&mut self, timeout: Duration) -> Result<(), ActionError> {
        let result = self.stop(timeout);
        self.events.push(ActionEvent {
            level: if result.is_ok() {
                EventLevel::Info
            } else {
                EventLevel::Warning
            },
            message: format!(
                "{} {}.",
                self.name,
                if self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
                    "stopping"
                } else {
                    "stopped"
                }
            ),
        });
        result
    }
}
