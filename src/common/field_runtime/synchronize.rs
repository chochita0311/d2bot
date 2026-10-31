//! 독립 화면 소비자의 동기 pump 요청도 가변 상태를 소유한 기존 actor가 처리한다.
use super::{FieldActor, FieldControlRuntime, FieldRuntimeHandle};
use crate::common::{
    error::ControlError,
    field_control::{FieldDirective, FieldInputBackend},
    time::MonotonicClock,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
pub(super) struct PumpRequest {
    reply: mpsc::SyncSender<Result<FieldDirective, ControlError>>,
    cancelled: Arc<AtomicBool>,
}
impl<B: FieldInputBackend> FieldRuntimeHandle<B> {
    pub fn clock(&self) -> Arc<dyn MonotonicClock> {
        self.0.clock.clone()
    }
    pub fn focused(&self) -> Result<bool, ControlError> {
        (self.0.focused)()
    }
    pub fn screen_age(&self) -> f64 {
        self.0.ages.screen_age()
    }
    pub fn is_dry_run(&self) -> bool {
        self.0.inputs.backend().is_dry_run()
    }
    pub fn release_inputs(&self) -> Result<(), ControlError> {
        self.0.inputs.release(false)
    }
    fn request_pump(&self) -> Result<FieldDirective, ControlError> {
        if !self.running() {
            return Err(ControlError::Runtime("field actor is not running".into()));
        }
        let (reply, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut slot = self
                .0
                .pump_request
                .lock()
                .map_err(|_| ControlError::Runtime("field pump mailbox poisoned".into()))?;
            if slot
                .as_ref()
                .is_some_and(|r| !r.cancelled.load(Ordering::Acquire))
            {
                return Err(ControlError::Runtime(
                    "field pump request already queued".into(),
                ));
            }
            *slot = Some(PumpRequest {
                reply,
                cancelled: cancelled.clone(),
            });
        }
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(result) => result,
            Err(error) => {
                cancelled.store(true, Ordering::Release);
                Err(ControlError::Runtime(format!(
                    "field pump response unavailable: {error}"
                )))
            }
        }
    }
    pub(super) fn cancel_pending_pump(&self) -> Result<(), ControlError> {
        let request = self
            .0
            .pump_request
            .lock()
            .map_err(|_| ControlError::Runtime("field pump mailbox poisoned".into()))?
            .take();
        if let Some(request) = request {
            request.cancelled.store(true, Ordering::Release);
            let _ = request.reply.send(Err(ControlError::Runtime(
                "field stopped before pump reply".into(),
            )));
        }
        Ok(())
    }
}
impl<B: FieldInputBackend> FieldActor<B> {
    pub(super) fn pump_requested(&mut self) -> Result<FieldDirective, ControlError> {
        let request = self
            .handle
            .0
            .pump_request
            .lock()
            .map_err(|_| ControlError::Runtime("field pump mailbox poisoned".into()))?
            .take();
        let result = self.pump();
        if let Some(request) = request.filter(|r| !r.cancelled.load(Ordering::Acquire)) {
            let reply = result
                .as_ref()
                .cloned()
                .map_err(|e| ControlError::Runtime(e.to_string()));
            let _ = request.reply.send(reply);
        }
        result
    }
}
impl<B: FieldInputBackend> FieldControlRuntime<B> {
    /// 수동 모드는 직접 실행하고, worker 시작 후에는 기존 200ms actor 응답 경계로 전달한다.
    pub fn pump_current(&mut self) -> Result<FieldDirective, ControlError> {
        if let Some(actor) = &mut self.actor {
            actor.pump()
        } else {
            self.handle.request_pump()
        }
    }
}
