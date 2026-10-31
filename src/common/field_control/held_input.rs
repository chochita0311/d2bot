use super::{DesktopPoint, FieldAction, FieldDirective};
use crate::common::config::ConfigError;
use crate::common::error::ControlError;
use crate::common::time::{self, MonotonicClock};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

pub trait FieldInputBackend: Send + Sync + 'static {
    fn is_dry_run(&self) -> bool {
        false
    }
    fn key_down(&self, key: &str) -> Result<(), ControlError>;
    fn key_down_observed(
        &self,
        key: &str,
        _source: Option<InputObservation>,
    ) -> Result<(), ControlError> {
        self.key_down(key)
    }
    /// Cleanup must remain available after stop, pause and focus loss.
    fn key_up(&self, key: &str) -> Result<(), ControlError>;
    /// A complete press must attempt key-up even when key-down partially fails.
    fn press(&self, key: &str) -> Result<(), ControlError>;
    fn move_pointer(&self, point: DesktopPoint) -> Result<(), ControlError>;
    fn move_pointer_observed(
        &self,
        point: DesktopPoint,
        _source: Option<InputObservation>,
    ) -> Result<(), ControlError> {
        self.move_pointer(point)
    }
    fn supports_pointer_confirmation(&self) -> bool {
        false
    }
    fn pointer_matches(&self, _point: DesktopPoint) -> Result<bool, ControlError> {
        Ok(false)
    }
}

/// 입력이 실제로 소비한 프레임의 시각. 대기/반복 중 새 프레임으로 재명명하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct InputObservation {
    sequence: u64,
    capture_started_at: f64,
    decision_ready_at: f64,
}
impl InputObservation {
    pub fn new(sequence: u64, captured: f64, ready: f64) -> Result<Self, ConfigError> {
        time::nonnegative(captured, "input source capture time")?;
        time::nonnegative(ready, "input source decision time")?;
        if ready < captured {
            return Err(ConfigError::value("input decision precedes its capture"));
        }
        Ok(Self {
            sequence,
            capture_started_at: captured,
            decision_ready_at: ready,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeldInputSnapshot {
    pub held: Vec<String>,
    pub pending: Option<String>,
    pub dispatches: BTreeMap<String, u64>,
    pub last_dispatch_at: Option<f64>,
    pub cast_pointer: Option<DesktopPoint>,
    pub pointer: Option<DesktopPoint>,
}

struct HeldState {
    held: BTreeSet<String>,
    pending: Option<String>,
    dispatches: BTreeMap<String, u64>,
    last_dispatch_at: Option<f64>,
    cast_pointer: Option<DesktopPoint>,
    pointer: Option<DesktopPoint>,
    aimed_at: Option<f64>,
    pressed_at: Option<f64>,
    deadline: Option<f64>,
    epoch: u64,
    fault: Option<String>,
    observation: Option<InputObservation>,
}

struct InputOwner<B: FieldInputBackend> {
    backend: Arc<B>,
    keys: [String; 2],
    clock: Option<Arc<dyn MonotonicClock>>,
    repeat_seconds: Option<f64>,
    settle_seconds: f64,
    state: Mutex<HeldState>,
    // Only short key down/up operations share this lock. Pointer movement and
    // confirmation run outside it, so a stalled pointer cannot block lease release.
    key_dispatch: Mutex<()>,
    closed: AtomicBool,
    suspended: AtomicBool,
}

impl<B: FieldInputBackend> Drop for InputOwner<B> {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let state = match self.state.get_mut() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        for key in &state.held {
            crate::common::error::best_effort_cleanup(|| self.backend.key_up(key));
        }
    }
}

/// Clones refer to the same held-key owner; final Drop performs best-effort release.
pub struct HeldFieldInput<B: FieldInputBackend> {
    inner: Arc<InputOwner<B>>,
}
impl<B: FieldInputBackend> Clone for HeldFieldInput<B> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

fn lock<'a, T>(mutex: &'a Mutex<T>, name: &str) -> Result<MutexGuard<'a, T>, ControlError> {
    mutex
        .lock()
        .map_err(|_| ControlError::Runtime(format!("{name} lock was poisoned")))
}

impl<B: FieldInputBackend> HeldFieldInput<B> {
    pub fn new(
        backend: Arc<B>,
        movement_key: &str,
        attack_key: &str,
        clock: Option<Arc<dyn MonotonicClock>>,
        repeat_seconds: Option<f64>,
        aim_settle_seconds: f64,
    ) -> Result<Self, ConfigError> {
        if movement_key.trim().is_empty()
            || attack_key.trim().is_empty()
            || movement_key.to_lowercase() == attack_key.to_lowercase()
        {
            return Err(ConfigError::value(
                "distinct movement and attack keys required",
            ));
        }
        if let Some(repeat) = repeat_seconds {
            time::positive(repeat, "held key repeat interval")?;
        }
        time::nonnegative(aim_settle_seconds, "held aim settling interval")?;
        let keys = [movement_key.to_lowercase(), attack_key.to_lowercase()];
        let dispatches = keys.iter().map(|key| (key.clone(), 0)).collect();
        let state = HeldState {
            held: BTreeSet::new(),
            pending: None,
            dispatches,
            last_dispatch_at: None,
            cast_pointer: None,
            pointer: None,
            aimed_at: None,
            pressed_at: None,
            deadline: None,
            epoch: 0,
            fault: None,
            observation: None,
        };
        Ok(Self {
            inner: Arc::new(InputOwner {
                backend,
                keys,
                clock,
                repeat_seconds,
                settle_seconds: aim_settle_seconds,
                state: Mutex::new(state),
                key_dispatch: Mutex::new(()),
                closed: AtomicBool::new(false),
                suspended: AtomicBool::new(false),
            }),
        })
    }
    pub fn backend(&self) -> &B {
        &self.inner.backend
    }
    pub fn keys(&self) -> &[String; 2] {
        &self.inner.keys
    }
    pub fn closed(&self) -> bool {
        self.inner.closed.load(Ordering::Acquire)
    }
    pub(crate) fn shares_clock(&self, clock: &Arc<dyn MonotonicClock>) -> bool {
        self.inner
            .clock
            .as_ref()
            .is_some_and(|own| Arc::ptr_eq(own, clock))
    }
    pub fn suspend(&self) -> Result<(), ControlError> {
        self.inner.suspended.store(true, Ordering::Release);
        self.release(false)
    }
    pub fn resume(&self) -> Result<(), ControlError> {
        self.release(false)?;
        self.inner.suspended.store(false, Ordering::Release);
        Ok(())
    }
    fn blocked(&self) -> bool {
        self.closed() || self.inner.suspended.load(Ordering::Acquire)
    }
    pub fn fault(&self) -> Result<Option<String>, ControlError> {
        Ok(lock(&self.inner.state, "held state")?.fault.clone())
    }
    fn checked_now(&self, now: f64) -> Result<f64, ControlError> {
        let checked = self.inner.clock.as_ref().map_or(now, |clock| clock.now());
        Ok(time::nonnegative(checked, "held input clock")?)
    }
    pub fn snapshot(&self) -> Result<HeldInputSnapshot, ControlError> {
        let state = lock(&self.inner.state, "held state")?;
        Ok(HeldInputSnapshot {
            held: state.held.iter().cloned().collect(),
            pending: state.pending.clone(),
            dispatches: state.dispatches.clone(),
            last_dispatch_at: state.last_dispatch_at,
            cast_pointer: state.cast_pointer,
            pointer: state.pointer,
        })
    }
    pub fn release(&self, preserve_aim: bool) -> Result<(), ControlError> {
        self.release_epoch(preserve_aim, None).map(|_| ())
    }
    fn release_epoch(
        &self,
        preserve_aim: bool,
        expected: Option<u64>,
    ) -> Result<Option<u64>, ControlError> {
        let _dispatch = lock(&self.inner.key_dispatch, "key dispatch")?;
        let (keys, mut failure, epoch) = {
            let mut state = lock(&self.inner.state, "held state")?;
            if expected.is_some_and(|expected| expected != state.epoch) {
                return Ok(None);
            }
            state.deadline = None;
            state.pressed_at = None;
            state.pending = None;
            state.observation = None;
            if !preserve_aim || !self.inner.backend.supports_pointer_confirmation() {
                state.pointer = None;
                state.aimed_at = None;
            }
            let failure = match state.epoch.checked_add(1) {
                Some(epoch) => {
                    state.epoch = epoch;
                    None
                }
                None => {
                    self.inner.closed.store(true, Ordering::Release);
                    Some(ControlError::Runtime("input epoch exhausted".into()))
                }
            };
            (
                state.held.iter().cloned().collect::<Vec<_>>(),
                failure,
                state.epoch,
            )
        };
        for key in keys {
            match self.inner.backend.key_up(&key) {
                Ok(()) => {
                    lock(&self.inner.state, "held state")?.held.remove(&key);
                }
                Err(error) => {
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
            }
        }
        if let Some(error) = failure {
            self.inner.closed.store(true, Ordering::Release);
            lock(&self.inner.state, "held state")?.fault = Some(error.to_string());
            return Err(error);
        }
        Ok(Some(epoch))
    }
    pub fn close(&self) -> Result<(), ControlError> {
        self.inner.closed.store(true, Ordering::Release);
        self.release(false)
    }
    fn failed(&self, error: ControlError) -> ControlError {
        self.inner.closed.store(true, Ordering::Release);
        if let Ok(mut state) = self.inner.state.lock() {
            state.fault = Some(error.to_string());
        }
        match self.release(false) {
            Ok(()) => error,
            Err(cleanup) => {
                ControlError::Backend(format!("{error}; input cleanup also failed: {cleanup}"))
            }
        }
    }
    pub fn apply(&self, directive: &FieldDirective, now: f64) -> Result<(), ControlError> {
        let result = self.apply_inner(directive, now, None);
        result.map_err(|error| self.failed(error))
    }
    pub fn apply_observed(
        &self,
        directive: &FieldDirective,
        now: f64,
        source: InputObservation,
    ) -> Result<(), ControlError> {
        let result = self.apply_inner(directive, now, Some(source));
        result.map_err(|error| self.failed(error))
    }
    pub fn press_token(&self, key: &str) -> Result<bool, ControlError> {
        let result = (|| {
            let _dispatch = lock(&self.inner.key_dispatch, "key dispatch")?;
            if self.blocked() {
                return Ok(false);
            }
            self.inner.backend.press(key)?;
            Ok(true)
        })();
        result.map_err(|error| self.failed(error))
    }
    fn apply_inner(
        &self,
        directive: &FieldDirective,
        now: f64,
        observation: Option<InputObservation>,
    ) -> Result<(), ControlError> {
        time::nonnegative(now, "now")?;
        if observation.is_some_and(|source| source.decision_ready_at > now) {
            return Err(ConfigError::value("input observation is from the future").into());
        }
        if self.blocked() {
            return self.release(false);
        }
        if !matches!(directive.action, FieldAction::Travel | FieldAction::Combat) {
            self.release(false)?;
            if directive.action == FieldAction::UsePotion {
                let _dispatch = lock(&self.inner.key_dispatch, "key dispatch")?;
                if self.blocked() {
                    return Ok(());
                }
                let key = directive.key.as_deref().ok_or_else(|| {
                    ControlError::Runtime("potion request is missing its key".into())
                })?;
                self.inner.backend.press(key)?;
            }
            return Ok(());
        }
        let desired = &self.inner.keys[usize::from(directive.action == FieldAction::Combat)];
        if let Some(deadline) = directive.deadline {
            time::nonnegative(deadline, "input deadline")?;
        }
        let Some(deadline) = directive.deadline.filter(|deadline| now < *deadline) else {
            return self.release(false);
        };
        let (mut epoch, must_release) = {
            let state = lock(&self.inner.state, "held state")?;
            (
                state.epoch,
                !state.held.is_empty() && (state.held.len() != 1 || !state.held.contains(desired)),
            )
        };
        if must_release {
            let Some(next) = self.release_epoch(false, Some(epoch))? else {
                return Ok(());
            };
            epoch = next;
        }
        let pointer = lock(&self.inner.state, "held state")?.pointer;
        if let Some(pointer) = pointer
            && self.inner.backend.supports_pointer_confirmation()
            && !self.inner.backend.pointer_matches(pointer)?
        {
            let Some(next) = self.release_epoch(false, Some(epoch))? else {
                return Ok(());
            };
            epoch = next;
            if directive.aim.is_none() {
                return Ok(());
            }
        }
        let pointer = {
            let state = lock(&self.inner.state, "held state")?;
            if self.blocked() || state.epoch != epoch {
                return Ok(());
            }
            state.pointer
        };
        if let Some(aim) = directive.aim.filter(|aim| Some(*aim) != pointer) {
            self.inner.backend.move_pointer_observed(aim, observation)?;
            let aimed_at = self.checked_now(now)?;
            let mut state = lock(&self.inner.state, "held state")?;
            if self.blocked() || state.epoch != epoch {
                return Ok(());
            }
            state.pointer = Some(aim);
            state.aimed_at = Some(aimed_at);
        }
        if self.checked_now(now)? >= deadline {
            return self.release_epoch(false, Some(epoch)).map(|_| ());
        }
        {
            let mut state = lock(&self.inner.state, "held state")?;
            if self.blocked() || state.epoch != epoch {
                return Ok(());
            }
            state.pending = Some(desired.clone());
            state.deadline = Some(deadline);
            state.observation = observation;
        }
        // 포인터 확인 중 지난 시간을 반영한다. 이전 now는0ms gate도 다음 service로 미룬다.
        self.press_if_ready(self.checked_now(now)?)
    }
    fn press_if_ready(&self, now: f64) -> Result<(), ControlError> {
        let (desired, pointer, epoch, deadline, observation) = {
            let state = lock(&self.inner.state, "held state")?;
            let Some(desired) = &state.pending else {
                return Ok(());
            };
            let repeated = self
                .inner
                .repeat_seconds
                .zip(state.pressed_at)
                .is_some_and(|(repeat, pressed)| now - pressed >= repeat);
            if state
                .aimed_at
                .is_some_and(|aimed| now - aimed < self.inner.settle_seconds)
                || (state.held.contains(desired) && !repeated)
            {
                return Ok(());
            }
            (
                desired.clone(),
                state.pointer,
                state.epoch,
                state.deadline,
                state.observation,
            )
        };
        if let Some(pointer) = pointer
            && self.inner.backend.supports_pointer_confirmation()
            && !self.inner.backend.pointer_matches(pointer)?
        {
            return self.release(false);
        }
        if let Some(deadline) = deadline
            && self.checked_now(now)? >= deadline
        {
            return self.release(true);
        }
        let dispatch = lock(&self.inner.key_dispatch, "key dispatch")?;
        {
            let mut state = lock(&self.inner.state, "held state")?;
            if self.blocked() || state.epoch != epoch || state.pending.as_deref() != Some(&desired)
            {
                return Ok(());
            }
            if let Some(deadline) = state.deadline
                && self.checked_now(now)? >= deadline
            {
                drop(state);
                drop(dispatch);
                return self.release(true);
            }
            // Record intent before dispatch so a partial backend failure is still released.
            state.held.insert(desired.clone());
        }
        self.inner
            .backend
            .key_down_observed(&desired, observation)?;
        let pressed = self.checked_now(now)?;
        {
            let mut state = lock(&self.inner.state, "held state")?;
            state.pressed_at = Some(pressed);
            state.last_dispatch_at = Some(pressed);
            state.cast_pointer = state.pointer;
            let count = state
                .dispatches
                .get_mut(&desired)
                .ok_or_else(|| ControlError::Runtime("unowned held key".into()))?;
            *count = count
                .checked_add(1)
                .ok_or_else(|| ControlError::Runtime("input dispatch counter exhausted".into()))?;
        }
        drop(dispatch);
        if self.blocked() {
            self.release(false)?;
        }
        Ok(())
    }
    pub fn service(&self, now: f64) -> Result<(), ControlError> {
        let deadline = lock(&self.inner.state, "held state")?.deadline;
        if self.blocked() || deadline.is_none() {
            return Ok(());
        }
        if deadline.is_some_and(|deadline| now >= deadline) {
            return self.release(true);
        }
        self.press_if_ready(now).map_err(|error| self.failed(error))
    }
    pub fn expire(&self, now: f64) -> Result<bool, ControlError> {
        let expired = lock(&self.inner.state, "held state")?
            .deadline
            .is_some_and(|deadline| now >= deadline);
        if expired {
            self.release(true)?;
        }
        Ok(expired)
    }
    pub fn renew_lease(
        &self,
        key: &str,
        pointer: DesktopPoint,
        deadline: f64,
        now: f64,
    ) -> Result<bool, ControlError> {
        self.renew_lease_inner(key, pointer, deadline, now, None)
            .map_err(|error| self.failed(error))
    }
    pub fn renew_lease_observed(
        &self,
        key: &str,
        pointer: DesktopPoint,
        deadline: f64,
        now: f64,
        observation: InputObservation,
    ) -> Result<bool, ControlError> {
        if observation.decision_ready_at > now {
            return Err(
                self.failed(ConfigError::value("input observation is from the future").into())
            );
        }
        self.renew_lease_inner(key, pointer, deadline, now, Some(observation))
            .map_err(|error| self.failed(error))
    }
    fn renew_lease_inner(
        &self,
        key: &str,
        pointer: DesktopPoint,
        deadline: f64,
        now: f64,
        observation: Option<InputObservation>,
    ) -> Result<bool, ControlError> {
        time::nonnegative(now, "lease_now")?;
        time::nonnegative(deadline, "lease_deadline")?;
        let (epoch, previous) = {
            let state = lock(&self.inner.state, "held state")?;
            if self.blocked()
                || state.held.len() != 1
                || !state.held.contains(key)
                || state.pointer != Some(pointer)
            {
                return Ok(false);
            }
            let Some(previous) = state.deadline else {
                return Ok(false);
            };
            (state.epoch, previous)
        };
        if now >= previous || now >= deadline {
            self.release(true)?;
            return Ok(false);
        }
        if !self.inner.backend.supports_pointer_confirmation()
            || !self.inner.backend.pointer_matches(pointer)?
        {
            self.release(false)?;
            return Ok(false);
        }
        let checked = self.checked_now(now)?;
        if checked >= previous || checked >= deadline {
            self.release(true)?;
            return Ok(false);
        }
        let mut state = lock(&self.inner.state, "held state")?;
        if self.blocked()
            || state.epoch != epoch
            || state.held.len() != 1
            || !state.held.contains(key)
        {
            return Ok(false);
        }
        state.deadline = Some(deadline);
        state.observation = observation;
        Ok(true)
    }
}
