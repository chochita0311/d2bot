use d2bot::common::error::ControlError;
use d2bot::common::field_control::{DesktopPoint, FieldInputBackend};
use d2bot::common::time::MonotonicClock;
use serde_json::{Value, json};
use std::collections::{BTreeSet, VecDeque};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
};
use std::time::Duration;

#[derive(Default)]
pub struct Clock(AtomicU64);
impl Clock {
    pub fn set(&self, now: f64) {
        self.0.store(now.to_bits(), Ordering::Release);
    }
}
impl MonotonicClock for Clock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}

#[derive(Default)]
struct State {
    pointer: Option<DesktopPoint>,
    events: VecDeque<Value>,
    fail_down: bool,
    fail_up: BTreeSet<String>,
}
struct Gate {
    movement: bool,
    point: DesktopPoint,
    entered: Sender<()>,
    resume: Receiver<()>,
}
#[derive(Default)]
pub struct Backend {
    state: Mutex<State>,
    gate: Mutex<Option<Gate>>,
    pub native_like: AtomicBool,
}
impl Backend {
    fn state(&self) -> Result<std::sync::MutexGuard<'_, State>, ControlError> {
        self.state
            .lock()
            .map_err(|_| ControlError::Runtime("test backend poisoned".into()))
    }
    fn event(&self, event: Value) -> Result<(), ControlError> {
        let mut state = self.state()?;
        if state.events.len() == 1000 {
            state.events.pop_front();
        }
        state.events.push_back(event);
        Ok(())
    }
    pub fn events(&self) -> Result<Value, ControlError> {
        Ok(json!(self.state()?.events))
    }
    pub fn fail_down(&self, fail: bool) -> Result<(), ControlError> {
        self.state()?.fail_down = fail;
        Ok(())
    }
    pub fn fail_up(&self, key: &str, fail: bool) -> Result<(), ControlError> {
        let mut state = self.state()?;
        if fail {
            state.fail_up.insert(key.into());
        } else {
            state.fail_up.remove(key);
        }
        Ok(())
    }
    pub fn block(
        &self,
        movement: bool,
        point: DesktopPoint,
    ) -> Result<(Receiver<()>, Sender<()>), ControlError> {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        *self
            .gate
            .lock()
            .map_err(|_| ControlError::Runtime("test gate poisoned".into()))? = Some(Gate {
            movement,
            point,
            entered: entered_tx,
            resume: resume_rx,
        });
        Ok((entered_rx, resume_tx))
    }
    fn wait_gate(&self, movement: bool, point: DesktopPoint) -> Result<(), ControlError> {
        let gate = {
            let mut guard = self
                .gate
                .lock()
                .map_err(|_| ControlError::Runtime("test gate poisoned".into()))?;
            if guard
                .as_ref()
                .is_some_and(|gate| gate.movement == movement && gate.point == point)
            {
                guard.take()
            } else {
                None
            }
        };
        if let Some(gate) = gate {
            gate.entered
                .send(())
                .map_err(|error| ControlError::Runtime(error.to_string()))?;
            gate.resume
                .recv_timeout(Duration::from_secs(3))
                .map_err(|error| ControlError::Runtime(error.to_string()))?;
        }
        Ok(())
    }
}
impl FieldInputBackend for Backend {
    fn is_dry_run(&self) -> bool {
        !self.native_like.load(Ordering::Acquire)
    }
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        self.event(json!(["down", key]))?;
        if self.state()?.fail_down {
            Err(ControlError::Backend("partial key down failure".into()))
        } else {
            Ok(())
        }
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        self.event(json!(["up", key]))?;
        if self.state()?.fail_up.contains(key) {
            Err(ControlError::Backend("key up failure".into()))
        } else {
            Ok(())
        }
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        self.event(json!(["press", key]))
    }
    fn move_pointer(&self, point: DesktopPoint) -> Result<(), ControlError> {
        self.event(json!(["move", point[0], point[1]]))?;
        self.wait_gate(true, point)?;
        self.state()?.pointer = Some(point);
        Ok(())
    }
    fn supports_pointer_confirmation(&self) -> bool {
        true
    }
    fn pointer_matches(&self, point: DesktopPoint) -> Result<bool, ControlError> {
        self.wait_gate(false, point)?;
        Ok(self.state()?.pointer == Some(point))
    }
}
