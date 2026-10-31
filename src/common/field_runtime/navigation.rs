use super::{FieldActor, FieldControlRuntime, FieldRuntimeHandle};
use crate::common::error::ControlError;
use crate::common::field_control::{DesktopPoint, FieldAction, FieldInputBackend, FieldPhase};
use crate::common::survival::ObservationStamp;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

pub(super) struct NavigationRequest {
    stamp: ObservationStamp,
    aim: DesktopPoint,
    cast: bool,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::SyncSender<Result<bool, ControlError>>,
}
impl<B: FieldInputBackend> FieldRuntimeHandle<B> {
    pub fn navigation_allowed(&self, stamp: &ObservationStamp) -> Result<bool, ControlError> {
        if self.0.stop.is_cancelled()
            || self.0.pause_requested.load(Ordering::Acquire)
            || self.0.pause_interrupt.load(Ordering::Acquire)
            || self.0.inputs.closed()
            || (!self.0.inputs.backend().is_dry_run() && !self.running())
        {
            return Ok(false);
        }
        let status = self.status()?;
        let latest = self
            .0
            .mailbox
            .lock()
            .map_err(|_| ControlError::Runtime("field mailbox poisoned".into()))?
            .latest_stamp
            .clone();
        let now = self.0.clock.now();
        Ok(!status.paused
            && !status.stopped
            && status.pause_revision == self.0.pause_revision.load(Ordering::Acquire)
            && (self.0.focused)()?
            && latest.as_ref() == Some(stamp)
            && status.last_stamp.as_ref() == Some(stamp)
            && (0.0..=self.0.ages.screen_age()).contains(&(now - stamp.observed_at()))
            && status.next_buff_due.is_none_or(|due| now < due)
            && status.phase == FieldPhase::Travel
            && status.last.action == FieldAction::Hold
            && status.last.reason == "destination_unconfirmed")
    }
    pub fn dispatch_navigation(
        &self,
        stamp: &ObservationStamp,
        aim: DesktopPoint,
        cast: bool,
    ) -> Result<bool, ControlError> {
        if !self.navigation_allowed(stamp)? {
            return Ok(false);
        }
        let (reply, sent) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut pending = self
                .0
                .navigation
                .lock()
                .map_err(|_| ControlError::Runtime("navigation mailbox poisoned".into()))?;
            if pending.is_some() {
                return Ok(false);
            }
            *pending = Some(NavigationRequest {
                stamp: stamp.clone(),
                aim,
                cast,
                cancelled: cancelled.clone(),
                reply,
            });
        }
        let remaining =
            (self.0.ages.screen_age() - (self.0.clock.now() - stamp.observed_at())).max(0.0);
        let timeout = Duration::try_from_secs_f64(remaining)
            .map_err(|_| ControlError::Runtime("navigation wait exceeds runtime range".into()))?;
        match sent.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                cancelled.store(true, Ordering::Release);
                Ok(false)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ControlError::Runtime(
                "navigation actor stopped before replying".into(),
            )),
        }
    }
    pub(super) fn cancel_pending_navigation(&self) -> Result<(), ControlError> {
        let request = self
            .0
            .navigation
            .lock()
            .map_err(|_| ControlError::Runtime("navigation mailbox poisoned".into()))?
            .take();
        if let Some(request) = request {
            request.cancelled.store(true, Ordering::Release);
            let _ = request.reply.send(Ok(false));
        }
        Ok(())
    }
}
impl<B: FieldInputBackend> FieldActor<B> {
    fn navigate(
        &mut self,
        stamp: &ObservationStamp,
        aim: DesktopPoint,
        cast: bool,
        cancelled: &AtomicBool,
    ) -> Result<bool, ControlError> {
        if cancelled.load(Ordering::Acquire) || !self.handle.navigation_allowed(stamp)? {
            return Ok(false);
        }
        self.control.inputs().release(false)?;
        let backend = self.control.inputs().backend();
        backend.move_pointer(aim)?;
        if cancelled.load(Ordering::Acquire) || !self.handle.navigation_allowed(stamp)? {
            return Ok(false);
        }
        if cast {
            return self
                .control
                .inputs()
                .press_token(&self.control.inputs().keys()[0]);
        }
        Ok(true)
    }
    pub(super) fn process_navigation(&mut self) -> Result<(), ControlError> {
        let request = self
            .handle
            .0
            .navigation
            .lock()
            .map_err(|_| ControlError::Runtime("navigation mailbox poisoned".into()))?
            .take();
        if let Some(request) = request {
            let result = self.navigate(
                &request.stamp,
                request.aim,
                request.cast,
                &request.cancelled,
            );
            let fault = result.as_ref().err().map(ToString::to_string);
            let _ = request.reply.send(result);
            if let Some(fault) = fault {
                return Err(ControlError::Runtime(fault));
            }
        }
        Ok(())
    }
}
impl<B: FieldInputBackend> FieldControlRuntime<B> {
    pub fn navigation_allowed(&self, stamp: &ObservationStamp) -> Result<bool, ControlError> {
        self.handle.navigation_allowed(stamp)
    }
    pub fn dispatch_navigation(
        &mut self,
        stamp: &ObservationStamp,
        aim: DesktopPoint,
        cast: bool,
    ) -> Result<bool, ControlError> {
        if let Some(actor) = &mut self.actor {
            actor.navigate(stamp, aim, cast, &AtomicBool::new(false))
        } else {
            self.handle.dispatch_navigation(stamp, aim, cast)
        }
    }
}
