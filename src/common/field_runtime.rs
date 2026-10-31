//! One actor owns mutable field state; publishers and release service share no actor lock.

mod factory;
mod navigation;
mod synchronize;
pub use factory::{
    create_field_control, create_field_control_with_backend, create_field_runtime,
    create_field_runtime_with_backend,
};

use super::buffing::{
    AutomaticBuffRenewal, BuffAction, BuffConfirmation, BuffDirective, BuffEnvironment,
    BuffObservation, PauseWarning,
};
use super::config::ConfigError;
use super::error::ControlError;
use super::field_control::{
    FieldControlLoop, FieldDirective, FieldInputBackend, FieldObservation, FieldPhase,
    HeldFieldInput, HeldInputSnapshot,
};
use super::survival::{
    BeltContents, CharacterSurvivalPolicy, ObservationLimits, ObservationStamp, SafetyObservation,
};
use super::survival_vision::HudEvidence;
use super::time::MonotonicClock;
use super::workers::{Cancellation, WorkerFault, WorkerGroup};
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

pub type FocusQuery = Arc<dyn Fn() -> Result<bool, ControlError> + Send + Sync>;
pub type NextStamp = Box<dyn FnMut() -> Result<ObservationStamp, ControlError> + Send>;
pub struct FieldRuntimeOptions {
    pub clock: Arc<dyn MonotonicClock>,
    pub focused: FocusQuery,
    pub interval: Duration,
    pub next_stamp: NextStamp,
}
#[derive(Debug, Clone)]
pub struct FieldRuntimeStatus {
    pub last: FieldDirective,
    pub phase: FieldPhase,
    pub last_stamp: Option<ObservationStamp>,
    pub belt_counts: Option<BTreeMap<u8, u8>>,
    pub potions: Option<u8>,
    pub paused: bool,
    pub stopped: bool,
    pub inputs: HeldInputSnapshot,
    pub buff_review: Option<String>,
    pub pause_warnings: Vec<PauseWarning>,
    pub next_buff_due: Option<f64>,
    pub pause_revision: u64,
}
pub struct BeltEvidence {
    pub stamp: ObservationStamp,
    pub contents: Option<BeltContents>,
}
struct Pending {
    observation: FieldObservation,
    expanded_belt: Option<BeltEvidence>,
    confirmation: Option<BuffConfirmation>,
}
#[derive(Default)]
struct Mailbox {
    pending: Option<Pending>,
    latest_stamp: Option<ObservationStamp>,
}
struct Shared<B: FieldInputBackend> {
    character_id: String,
    room_id: String,
    ages: ObservationLimits,
    has_buffs: bool,
    policy: Arc<CharacterSurvivalPolicy>,
    inputs: HeldFieldInput<B>,
    clock: Arc<dyn MonotonicClock>,
    focused: FocusQuery,
    stop: Cancellation,
    pause_requested: AtomicBool,
    pause_interrupt: AtomicBool,
    pause_revision: AtomicU64,
    running: AtomicBool,
    mailbox: Mutex<Mailbox>,
    status: Mutex<FieldRuntimeStatus>,
    navigation: Mutex<Option<navigation::NavigationRequest>>,
    pump_request: Mutex<Option<synchronize::PumpRequest>>,
}
pub struct FieldRuntimeHandle<B: FieldInputBackend>(Arc<Shared<B>>);
impl<B: FieldInputBackend> Clone for FieldRuntimeHandle<B> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<B: FieldInputBackend> FieldRuntimeHandle<B> {
    pub fn running(&self) -> bool {
        self.0.running.load(Ordering::Acquire) && !self.0.stop.is_cancelled()
    }
    pub fn publish(
        &self,
        observation: FieldObservation,
        confirmation: Option<BuffConfirmation>,
    ) -> Result<bool, ControlError> {
        self.publish_with_belt(observation, None, confirmation)
    }
    pub fn publish_hud(
        &self,
        evidence: HudEvidence,
        scene: FieldObservation,
        confirmation: Option<BuffConfirmation>,
    ) -> Result<bool, ControlError> {
        if evidence.stamp != *scene.safety().stamp() {
            return Ok(false);
        }
        let safety = SafetyObservation::new(
            evidence.stamp.clone(),
            evidence.reading.life.ratio(),
            evidence.reading.mana.ratio(),
            scene.safety().location(),
            scene.safety().monsters_clear(),
        )?;
        let visibility = evidence.reading.belt_visibility;
        let closed = match visibility {
            super::buffing::BeltVisibility::Closed => Some(true),
            super::buffing::BeltVisibility::Expanded => Some(false),
            super::buffing::BeltVisibility::Unknown => None,
        };
        let belt = if visibility == super::buffing::BeltVisibility::Expanded {
            Some(BeltEvidence {
                stamp: evidence.stamp,
                contents: evidence.reading.belt_contents(&self.0.policy),
            })
        } else {
            None
        };
        self.publish_with_belt(scene.with_safety(safety, closed)?, belt, confirmation)
    }
    pub fn publish_with_belt(
        &self,
        observation: FieldObservation,
        expanded_belt: Option<BeltEvidence>,
        confirmation: Option<BuffConfirmation>,
    ) -> Result<bool, ControlError> {
        let stamp = observation.safety().stamp();
        if confirmation
            .as_ref()
            .is_some_and(|evidence| !self.0.has_buffs || evidence.stamp != *stamp)
            || expanded_belt.as_ref().is_some_and(|evidence| {
                evidence.stamp != *stamp || observation.belt_closed() != Some(false)
            })
        {
            return Ok(false);
        }
        if stamp.character_id() != self.0.character_id
            || stamp.room_id() != self.0.room_id
            || !(0.0..=self.0.ages.screen_age())
                .contains(&(self.0.clock.now() - stamp.observed_at()))
        {
            return Ok(false);
        }
        let mut mailbox = self
            .0
            .mailbox
            .lock()
            .map_err(|_| ControlError::Runtime("field mailbox poisoned".into()))?;
        if self.0.stop.is_cancelled() || !stamp.newer_than(mailbox.latest_stamp.as_ref()) {
            return Ok(false);
        }
        mailbox.latest_stamp = Some(stamp.clone());
        mailbox.pending = Some(Pending {
            observation,
            expanded_belt,
            confirmation,
        });
        Ok(true)
    }
    pub fn pause(&self, paused: bool) -> Result<(), ControlError> {
        if paused {
            self.0.pause_interrupt.store(true, Ordering::Release);
        }
        self.0.pause_requested.store(paused, Ordering::Release);
        let revision =
            self.0
                .pause_revision
                .try_update(Ordering::AcqRel, Ordering::Acquire, |revision| {
                    revision.checked_add(1)
                });
        if revision.is_err() {
            let cleanup = self.stop();
            return combine(
                Err(ControlError::Runtime(
                    "field pause revision exhausted".into(),
                )),
                cleanup,
            );
        }
        let release = if paused {
            self.0.inputs.suspend()
        } else {
            Ok(())
        };
        self.0
            .mailbox
            .lock()
            .map_err(|_| ControlError::Runtime("field mailbox poisoned".into()))?
            .pending = None;
        combine(release, self.cancel_pending_navigation())
    }
    pub fn stop(&self) -> Result<(), ControlError> {
        self.0.stop.cancel();
        let released = self.0.inputs.close();
        self.0
            .mailbox
            .lock()
            .map_err(|_| ControlError::Runtime("field mailbox poisoned".into()))?
            .pending = None;
        let navigation = self.cancel_pending_navigation();
        let pump = self.cancel_pending_pump();
        combine(combine(released, navigation), pump)
    }
    pub fn status(&self) -> Result<FieldRuntimeStatus, ControlError> {
        let mut status = self
            .0
            .status
            .lock()
            .map_err(|_| ControlError::Runtime("field status poisoned".into()))?
            .clone();
        status.paused |= self.0.pause_requested.load(Ordering::Acquire);
        status.stopped |= self.0.stop.is_cancelled();
        status.inputs = self.0.inputs.snapshot()?;
        Ok(status)
    }
}

struct FieldActor<B: FieldInputBackend> {
    control: FieldControlLoop<B>,
    buffs: Option<AutomaticBuffRenewal>,
    next_stamp: NextStamp,
    handle: FieldRuntimeHandle<B>,
    pause_revision: u64,
}
impl<B: FieldInputBackend> FieldActor<B> {
    fn save_status(&self) -> Result<(), ControlError> {
        let status = FieldRuntimeStatus {
            last: self.control.last().clone(),
            phase: self.control.phase(),
            last_stamp: self.control.last_stamp().cloned(),
            belt_counts: self.control.state().belt_counts().cloned(),
            potions: self.control.state().potions_remaining(),
            paused: self.control.paused(),
            stopped: self.control.stopped(),
            inputs: self.control.inputs().snapshot()?,
            buff_review: self
                .buffs
                .as_ref()
                .and_then(|buffs| buffs.review_reason().map(str::to_owned)),
            pause_warnings: self
                .buffs
                .as_ref()
                .map_or_else(Vec::new, |buffs| buffs.pause_warnings().to_vec()),
            next_buff_due: self.control.state().next_buff_renewal_at(),
            pause_revision: self.pause_revision,
        };
        *self
            .handle
            .0
            .status
            .lock()
            .map_err(|_| ControlError::Runtime("field status poisoned".into()))? = status;
        Ok(())
    }
    fn shutdown(&mut self, reason: &str) -> Result<(), ControlError> {
        if let Some(buffs) = &mut self.buffs {
            buffs.interrupt(reason);
        }
        let closed = self.control.stop();
        combine(closed, self.save_status())
    }
    fn pump(&mut self) -> Result<FieldDirective, ControlError> {
        match self.process_navigation().and_then(|_| self.pump_inner()) {
            Ok(result) => {
                self.save_status()?;
                Ok(result)
            }
            Err(error) => Err(match self.shutdown("buff_worker_error") {
                Ok(()) => error,
                Err(cleanup) => {
                    ControlError::Runtime(format!("{error}; field cleanup failed: {cleanup}"))
                }
            }),
        }
    }
    fn pump_inner(&mut self) -> Result<FieldDirective, ControlError> {
        let now = self.handle.0.clock.now();
        if self.handle.0.stop.is_cancelled() {
            self.control.stop()?;
            return self.control.tick(now);
        }
        let focused = (self.handle.0.focused)()?;
        if !focused {
            self.handle.pause(true)?;
        }
        loop {
            let interrupted = self.handle.0.pause_interrupt.swap(false, Ordering::AcqRel);
            if interrupted {
                if let Some(buffs) = &mut self.buffs {
                    buffs.interrupt("buff_paused_or_unfocused");
                }
                self.control.pause(true)?;
            }
            let revision = self.handle.0.pause_revision.load(Ordering::Acquire);
            if revision == self.pause_revision && !interrupted {
                break;
            }
            let paused = self.handle.0.pause_requested.load(Ordering::Acquire);
            if paused && let Some(buffs) = &mut self.buffs {
                buffs.interrupt("buff_paused_or_unfocused");
            }
            self.control.pause(paused)?;
            self.pause_revision = revision;
        }
        if !focused {
            return self.control.tick(now);
        }
        self.control.tick(now)?;
        let pending = self
            .handle
            .0
            .mailbox
            .lock()
            .map_err(|_| ControlError::Runtime("field mailbox poisoned".into()))?
            .pending
            .take();
        let Some(pending) = pending else {
            if let Some(buffs) = &mut self.buffs
                && let Some(result) = buffs.tick(self.control.state(), now)?
            {
                return self.buff_result(result, now);
            }
            return Ok(self.control.last().clone());
        };
        let observation = pending.observation;
        if let Some(belt) = pending.expanded_belt
            && (0.0..=self.handle.0.ages.screen_age())
                .contains(&(now - observation.safety().stamp().observed_at()))
        {
            self.control
                .reconcile_belt(&belt.stamp, belt.contents.as_ref());
        }
        if let Some(buffs) = &mut self.buffs
            && !self.control.stopped()
            && !self.control.paused()
            && !self.control.inputs().closed()
        {
            if self.control.state().potions_remaining() == Some(0)
                && self
                    .control
                    .state()
                    .policy()
                    .is_some_and(|policy| policy.exit_on_empty())
            {
                buffs.interrupt("buff_potions_empty");
                return self.control.observe(&observation, now, true);
            }
            if !self.control.potion_pending() {
                let mut environment = DispatchEnvironment {
                    handle: self.handle.clone(),
                    stamp: observation.safety().stamp().clone(),
                    next_stamp: &mut self.next_stamp,
                };
                let result = buffs.observe(
                    self.control.state_mut(),
                    &BuffObservation {
                        safety: observation.safety().clone(),
                        belt_closed: observation.belt_closed(),
                    },
                    now,
                    &mut environment,
                    pending.confirmation.as_ref(),
                )?;
                if let Some(result) = result {
                    if matches!(result.action, BuffAction::UsePotion | BuffAction::Exit) {
                        return self.control.observe(&observation, now, true);
                    }
                    return self.buff_result(result, now);
                }
            }
        }
        self.control.observe(&observation, now, true)
    }
    fn buff_result(
        &mut self,
        result: BuffDirective,
        now: f64,
    ) -> Result<FieldDirective, ControlError> {
        if self.control.state().potions_remaining() == Some(0)
            && self
                .control
                .state()
                .policy()
                .is_some_and(|policy| policy.exit_on_empty())
        {
            return self.control.tick(now);
        }
        self.control.emit(FieldDirective::hold(result.reason), now)
    }
}

struct DispatchEnvironment<'a, B: FieldInputBackend> {
    handle: FieldRuntimeHandle<B>,
    stamp: ObservationStamp,
    next_stamp: &'a mut NextStamp,
}
impl<B: FieldInputBackend> BuffEnvironment for DispatchEnvironment<'_, B> {
    type Error = ControlError;
    fn clock(&self) -> f64 {
        self.handle.0.clock.now()
    }
    fn interrupted(&self) -> Result<bool, Self::Error> {
        Ok(self.handle.0.stop.is_cancelled()
            || self.handle.0.pause_requested.load(Ordering::Acquire)
            || self.handle.0.pause_interrupt.load(Ordering::Acquire)
            || !(self.handle.0.focused)()?)
    }
    fn send_token(&mut self, token: &str) -> Result<bool, Self::Error> {
        self.handle.0.inputs.release(false)?;
        if self.interrupted()?
            || !(0.0..=self.handle.0.ages.screen_age())
                .contains(&(self.clock() - self.stamp.observed_at()))
        {
            return Ok(false);
        }
        self.handle.0.inputs.press_token(token)
    }
    fn next_stamp(&mut self) -> Result<ObservationStamp, Self::Error> {
        (self.next_stamp)()
    }
}

pub struct FieldControlRuntime<B: FieldInputBackend> {
    handle: FieldRuntimeHandle<B>,
    actor: Option<FieldActor<B>>,
    workers: WorkerGroup,
    interval: Duration,
    started: bool,
}
impl<B: FieldInputBackend> FieldControlRuntime<B> {
    pub fn new(
        control: FieldControlLoop<B>,
        buffs: Option<AutomaticBuffRenewal>,
        options: FieldRuntimeOptions,
    ) -> Result<Self, ControlError> {
        if options.interval.is_zero() {
            return Err(ConfigError::value("positive watchdog interval required").into());
        }
        if !control.inputs().shares_clock(&options.clock) {
            return Err(ConfigError::value(
                "field inputs and actor must share one monotonic clock",
            )
            .into());
        }
        if buffs.as_ref().is_some_and(|buffs| {
            !buffs.owns_state(control.state()) || buffs.ages() != control.limits().ages()
        }) {
            return Err(ConfigError::value(
                "buff consumer must share this loop's state and observation limits",
            )
            .into());
        }
        let status = FieldRuntimeStatus {
            last: control.last().clone(),
            phase: control.phase(),
            last_stamp: control.last_stamp().cloned(),
            belt_counts: control.state().belt_counts().cloned(),
            potions: control.state().potions_remaining(),
            paused: control.paused(),
            stopped: control.stopped(),
            inputs: control.inputs().snapshot()?,
            buff_review: None,
            pause_warnings: buffs
                .as_ref()
                .map_or_else(Vec::new, |buffs| buffs.pause_warnings().to_vec()),
            next_buff_due: control.state().next_buff_renewal_at(),
            pause_revision: 0,
        };
        let stop = Cancellation::default();
        let handle = FieldRuntimeHandle(Arc::new(Shared {
            character_id: control.state().character_id().into(),
            room_id: control.state().room_id().into(),
            ages: control.limits().ages(),
            has_buffs: buffs.is_some(),
            policy: control
                .state()
                .shared_policy()
                .ok_or_else(|| ConfigError::value("character survival policy required"))?,
            inputs: control.inputs().clone(),
            clock: options.clock,
            focused: options.focused,
            stop: stop.clone(),
            pause_requested: AtomicBool::new(false),
            pause_interrupt: AtomicBool::new(false),
            pause_revision: AtomicU64::new(0),
            running: AtomicBool::new(false),
            mailbox: Mutex::new(Mailbox::default()),
            status: Mutex::new(status),
            navigation: Mutex::new(None),
            pump_request: Mutex::new(None),
        }));
        let actor = FieldActor {
            control,
            buffs,
            next_stamp: options.next_stamp,
            handle: handle.clone(),
            pause_revision: 0,
        };
        Ok(Self {
            handle,
            actor: Some(actor),
            workers: WorkerGroup::new(stop, 2),
            interval: options.interval,
            started: false,
        })
    }
    pub fn handle(&self) -> FieldRuntimeHandle<B> {
        self.handle.clone()
    }
    pub fn faults(&self) -> Result<Vec<WorkerFault>, ControlError> {
        self.workers.faults()
    }
    pub fn pump(&mut self) -> Result<FieldDirective, ControlError> {
        self.actor
            .as_mut()
            .ok_or_else(|| {
                ControlError::Runtime("manual pump unavailable after worker start".into())
            })?
            .pump()
    }
    pub fn start(&mut self) -> Result<(), ControlError> {
        if self.started || self.handle.0.stop.is_cancelled() {
            return Err(ControlError::Runtime(
                "runtime can only be started once".into(),
            ));
        }
        self.started = true;
        let mut actor = self
            .actor
            .take()
            .ok_or_else(|| ControlError::Runtime("field actor already moved".into()))?;
        let interval = self.interval;
        // The actor may receive a queued frame immediately. Dispatch must wait until
        // the independent release service owns its cleanup guard.
        let (release_ready, release_started) = std::sync::mpsc::sync_channel(1);
        let handle = self.handle.clone();
        handle.0.running.store(true, Ordering::Release);
        let result = self.workers.spawn("d2-field-control", move |token| {
            let result = (|| {
                release_started
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|error| {
                        ControlError::Runtime(format!(
                            "field release service did not start: {error}"
                        ))
                    })?;
                while !token.is_cancelled() {
                    actor.pump_requested()?;
                    token.wait(interval)?;
                }
                Ok(())
            })();
            let cleanup = actor.shutdown("buff_worker_stopped");
            handle.0.running.store(false, Ordering::Release);
            combine(result, cleanup)
        });
        if let Err(error) = result {
            self.handle.0.running.store(false, Ordering::Release);
            self.handle.stop()?;
            return Err(error);
        }
        let handle = self.handle.clone();
        if let Err(error) = self.workers.spawn("d2-field-release", move |token| {
            let _release_on_exit = ReleaseOnDrop(handle.0.inputs.clone());
            let _ = release_ready.send(());
            let result = (|| {
                let mut previous = None;
                while !token.is_cancelled() {
                    if !(handle.0.focused)()? && !handle.0.pause_requested.load(Ordering::Acquire) {
                        handle.pause(true)?;
                    }
                    let now =
                        super::time::nonnegative(handle.0.clock.now(), "field release clock")?;
                    if previous.is_some_and(|previous| now < previous) {
                        return Err(ControlError::Runtime(
                            "field release monotonic clock moved backwards".into(),
                        ));
                    }
                    previous = Some(now);
                    handle.0.inputs.service(now)?;
                    token.wait(interval)?;
                }
                Ok(())
            })();
            combine(result, handle.0.inputs.close())
        }) {
            self.stop(Duration::from_secs(1))?;
            return Err(error);
        }
        Ok(())
    }
    pub fn stop(&mut self, timeout: Duration) -> Result<(), ControlError> {
        let release = self.handle.stop();
        let actor = self
            .actor
            .as_mut()
            .map_or(Ok(()), |actor| actor.shutdown("buff_stopped"));
        let joined = self.workers.finish(timeout);
        combine(combine(release, actor), joined)
    }
}
impl<B: FieldInputBackend> Drop for FieldControlRuntime<B> {
    fn drop(&mut self) {
        super::error::best_effort_cleanup(|| self.stop(Duration::from_millis(100)));
    }
}
struct ReleaseOnDrop<B: FieldInputBackend>(HeldFieldInput<B>);
impl<B: FieldInputBackend> Drop for ReleaseOnDrop<B> {
    fn drop(&mut self) {
        super::error::best_effort_cleanup(|| self.0.close());
    }
}
fn combine(
    first: Result<(), ControlError>,
    second: Result<(), ControlError>,
) -> Result<(), ControlError> {
    match (first, second) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup)) => Err(ControlError::Runtime(format!(
            "{error}; cleanup also failed: {cleanup}"
        ))),
    }
}
