use d2bot::common::{
    error::ControlError,
    field_control::{
        DesktopPoint, FieldAction, FieldDirective, FieldInputBackend, HeldFieldInput,
        InputObservation,
    },
    time::MonotonicClock,
};
use serde_json::{Value, json};
use std::{
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
type TestResult = Result<(), Box<dyn Error>>;

#[derive(Default)]
struct Clock(AtomicU64);
impl Clock {
    fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Release);
    }
}
impl MonotonicClock for Clock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}

#[derive(Default)]
struct Trace {
    point: Mutex<DesktopPoint>,
    events: Mutex<Vec<Value>>,
    move_clock: Option<(Arc<Clock>, f64)>,
}
impl Trace {
    fn event(
        &self,
        action: &str,
        key: &str,
        source: Option<InputObservation>,
    ) -> Result<(), ControlError> {
        self.events
            .lock()
            .map_err(|e| ControlError::Runtime(e.to_string()))?
            .push(json!({"action":action,"key":key,"source":source}));
        Ok(())
    }
    fn events(&self) -> Result<Vec<Value>, Box<dyn Error>> {
        Ok(self.events.lock().map_err(|e| e.to_string())?.clone())
    }
}
impl FieldInputBackend for Trace {
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        self.event("down", key, None)
    }
    fn key_down_observed(
        &self,
        key: &str,
        source: Option<InputObservation>,
    ) -> Result<(), ControlError> {
        self.event("down", key, source)
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        self.event("up", key, None)
    }
    fn press(&self, key: &str) -> Result<(), ControlError> {
        self.event("press", key, None)
    }
    fn move_pointer(&self, point: DesktopPoint) -> Result<(), ControlError> {
        *self
            .point
            .lock()
            .map_err(|e| ControlError::Runtime(e.to_string()))? = point;
        if let Some((clock, completed)) = &self.move_clock {
            clock.set(*completed);
        }
        Ok(())
    }
    fn supports_pointer_confirmation(&self) -> bool {
        true
    }
    fn pointer_matches(&self, point: DesktopPoint) -> Result<bool, ControlError> {
        Ok(*self
            .point
            .lock()
            .map_err(|e| ControlError::Runtime(e.to_string()))?
            == point)
    }
}
fn travel(aim: DesktopPoint, deadline: f64) -> Result<FieldDirective, Box<dyn Error>> {
    Ok(FieldDirective::new(
        FieldAction::Travel,
        "observed",
        None,
        Some(aim),
        Some(deadline),
        false,
    )?)
}

#[test]
fn invalid_lease_extension_releases_input_instead_of_retaining_an_unbounded_hold() -> TestResult {
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        let backend = Arc::new(Trace::default());
        let clock = Arc::new(Clock::default());
        let input = HeldFieldInput::new(backend, "a", "d", Some(clock.clone()), Some(0.1), 0.0)?;
        input.apply(&travel([10, 10], 1.0)?, 0.0)?;
        clock.set(0.1);
        assert!(
            input
                .renew_lease_observed(
                    "a",
                    [10, 10],
                    invalid,
                    0.1,
                    InputObservation::new(1, 0.0, 0.1)?
                )
                .is_err()
        );
        assert!(input.closed());
        assert!(input.snapshot()?.held.is_empty());
    }
    Ok(())
}
#[test]
fn pending_and_repeated_keydowns_keep_their_lease_observation() -> TestResult {
    let backend = Arc::new(Trace::default());
    let clock = Arc::new(Clock::default());
    let input = HeldFieldInput::new(
        backend.clone(),
        "a",
        "d",
        Some(clock.clone()),
        Some(0.1),
        0.05,
    )?;
    input.apply_observed(
        &travel([10, 10], 0.2)?,
        0.0,
        InputObservation::new(7, 0.0, 0.0)?,
    )?;
    assert!(backend.events()?.is_empty());
    clock.set(0.049);
    input.service(0.049)?;
    assert!(backend.events()?.is_empty());
    clock.set(0.05);
    input.service(0.05)?;
    assert_eq!(backend.events()?[0]["source"]["sequence"], 7);
    clock.set(0.1);
    assert!(input.renew_lease_observed(
        "a",
        [10, 10],
        0.3,
        0.1,
        InputObservation::new(8, 0.08, 0.1)?
    )?);
    clock.set(0.151);
    input.service(0.151)?;
    let rows = backend.events()?;
    assert_eq!(rows[1]["source"]["sequence"], 8);
    assert_eq!(rows[1]["source"]["capture_started_at"], 0.08);
    input.release(false)?;
    clock.set(0.2);
    input.apply(&travel([20, 20], 0.5)?, 0.2)?;
    clock.set(0.251);
    input.service(0.251)?;
    assert!(backend.events()?.last().ok_or("missing down")?["source"].is_null());
    input.close()?;
    assert!(input.snapshot()?.held.is_empty());
    Ok(())
}

#[test]
fn pointer_confirmation_elapsed_time_does_not_defer_zero_settle_keydown() -> TestResult {
    for settle in [0.0, 0.05] {
        let clock = Arc::new(Clock::default());
        clock.set(1.0);
        let backend = Arc::new(Trace {
            move_clock: Some((clock.clone(), 1.012)),
            ..Default::default()
        });
        let input = HeldFieldInput::new(
            backend.clone(),
            "a",
            "d",
            Some(clock.clone()),
            Some(0.1),
            settle,
        )?;
        input.apply_observed(
            &travel([10, 10], 1.25)?,
            1.0,
            InputObservation::new(1, 0.99, 1.0)?,
        )?;
        if settle == 0.0 {
            assert_eq!(
                backend.events()?.len(),
                1,
                "confirmed zero-settle aim must dispatch without service()"
            );
        } else {
            assert!(backend.events()?.is_empty());
            clock.set(1.061);
            input.service(1.061)?;
            assert!(backend.events()?.is_empty());
            clock.set(1.063);
            input.service(1.063)?;
            assert_eq!(backend.events()?.len(), 1);
        }
        assert_eq!(backend.events()?[0]["source"]["decision_ready_at"], 1.0);
        input.close()?;
        assert!(input.snapshot()?.held.is_empty());
    }
    Ok(())
}
#[test]
fn confirmed_pointer_has_no_timer_gate_and_turn_keeps_the_movement_key_held() -> TestResult {
    let backend = Arc::new(Trace::default());
    let clock = Arc::new(Clock::default());
    let input = HeldFieldInput::new(
        backend.clone(),
        "a",
        "d",
        Some(clock.clone()),
        Some(0.1),
        0.0,
    )?;
    input.apply_observed(
        &travel([10, 10], 1.0)?,
        0.0,
        InputObservation::new(1, 0.0, 0.0)?,
    )?;
    assert_eq!(backend.events()?.len(), 1);
    clock.set(0.001);
    input.apply_observed(
        &travel([20, 20], 1.0)?,
        0.001,
        InputObservation::new(2, 0.0, 0.001)?,
    )?;
    assert_eq!(backend.events()?.len(), 1);
    assert_eq!(input.snapshot()?.held, vec!["a"]);
    assert_eq!(input.snapshot()?.pointer, Some([20, 20]));
    clock.set(0.101);
    input.service(0.101)?;
    assert_eq!(backend.events()?[1]["source"]["sequence"], 2);
    let combat = FieldDirective::new(FieldAction::Combat, "near", None, None, Some(1.0), false)?;
    clock.set(0.102);
    input.apply_observed(&combat, 0.102, InputObservation::new(3, 0.1, 0.102)?)?;
    let rows = backend.events()?;
    assert_eq!(rows[2]["action"], "up");
    assert_eq!(rows[2]["key"], "a");
    assert_eq!(rows[3]["action"], "down");
    assert_eq!(rows[3]["key"], "d");
    assert_eq!(input.snapshot()?.held, vec!["d"]);
    input.close()?;
    assert!(input.snapshot()?.held.is_empty());
    Ok(())
}

#[test]
fn invalid_observation_cannot_relabel_or_extend_input() -> TestResult {
    for value in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(InputObservation::new(0, value, 1.0).is_err());
        assert!(InputObservation::new(0, 0.0, value).is_err());
    }
    assert!(InputObservation::new(0, 1.0, 0.0).is_err());
    let backend = Arc::new(Trace::default());
    let input = HeldFieldInput::new(backend.clone(), "a", "d", None, None, 0.0)?;
    input.apply(&travel([10, 10], 1.0)?, 0.0)?;
    assert!(
        input
            .apply_observed(
                &travel([20, 20], 1.0)?,
                0.1,
                InputObservation::new(2, 0.1, 0.2)?
            )
            .is_err()
    );
    assert!(input.closed());
    assert!(input.snapshot()?.held.is_empty());
    assert_eq!(
        backend
            .events()?
            .iter()
            .filter(|row| row["action"] == "down")
            .count(),
        1
    );
    Ok(())
}
