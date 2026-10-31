#[path = "support/field_backend.rs"]
mod field_backend;
use d2bot::common::{
    error::ControlError,
    field_control::{FieldAction, FieldDirective, FieldInputSession, InputSessionOptions},
    workers::{Cancellation, CooperativeStop},
};
use field_backend::{Backend, Clock};
use serde_json::json;
use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
type TestResult = Result<(), Box<dyn Error>>;
type Session = FieldInputSession<Backend>;
fn session(
    clock: Arc<Clock>,
    backend: Arc<Backend>,
    stop: CooperativeStop,
    focused: Arc<dyn Fn() -> Result<bool, ControlError> + Send + Sync>,
) -> Result<Session, ControlError> {
    FieldInputSession::new(
        backend,
        "f2",
        "f4",
        InputSessionOptions {
            clock,
            stop,
            focused,
            repeat_seconds: Some(0.32),
            aim_settle_seconds: 0.05,
        },
    )
}
fn combat(session: &Session, now: f64) -> Result<(), ControlError> {
    session.inputs().apply(
        &FieldDirective::new(
            FieldAction::Combat,
            "fixture",
            None,
            None,
            Some(now + 0.25),
            false,
        )?,
        now,
    )
}
fn wait_until(predicate: impl Fn() -> bool) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !predicate() {
        if Instant::now() >= deadline {
            return Err("input watchdog did not progress".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
#[test]
fn expiry_only_watchdog_does_not_repeat_attack_requests() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let checks = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let observed = checks.clone();
    let mut inputs = FieldInputSession::new(
        backend.clone(),
        "f2",
        "f4",
        InputSessionOptions {
            clock: clock.clone(),
            stop: CooperativeStop::default(),
            focused: Arc::new(move || {
                observed.fetch_add(1, Ordering::AcqRel);
                Ok(true)
            }),
            repeat_seconds: Some(0.05),
            aim_settle_seconds: 0.0,
        },
    )?;
    inputs.start_expiry_only("buff-site-expiry-fixture", 20.0)?;
    combat(&inputs, 10.0)?;
    clock.set(10.1);
    wait_until(|| checks.load(Ordering::Acquire) >= 3)?;
    assert_eq!(backend.events()?, json!([["down", "f4"]]));
    clock.set(10.25);
    wait_until(|| inputs.inputs().snapshot().is_ok_and(|s| s.held.is_empty()))?;
    assert_eq!(backend.events()?, json!([["down", "f4"], ["up", "f4"]]));
    inputs.finish(Duration::from_secs(1))?;
    Ok(())
}
#[test]
fn absent_repeat_interval_keeps_the_original_paused_hold() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let mut inputs = FieldInputSession::new(
        backend.clone(),
        "f2",
        "f4",
        InputSessionOptions {
            clock: clock.clone(),
            stop: CooperativeStop::default(),
            focused: Arc::new(|| Ok(true)),
            repeat_seconds: None,
            aim_settle_seconds: 0.0,
        },
    )?;
    inputs.start("paused-hold-fixture", 20.0)?;
    combat(&inputs, 10.0)?;
    clock.set(10.2);
    combat(&inputs, 10.2)?;
    clock.set(10.4);
    combat(&inputs, 10.4)?;
    assert_eq!(backend.events()?, json!([["down", "f4"]]));
    inputs.finish(Duration::from_secs(1))?;
    assert_eq!(backend.events()?, json!([["down", "f4"], ["up", "f4"]]));
    Ok(())
}
#[test]
fn renewed_deadline_is_not_released_by_a_slow_old_focus_check() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let (entered_send, entered) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    let blocked = Arc::new(std::sync::Mutex::new(blocked));
    let first = Arc::new(AtomicBool::new(true));
    let seen = first.clone();
    let mut inputs = session(
        clock.clone(),
        backend.clone(),
        CooperativeStop::default(),
        Arc::new(move || {
            if seen.swap(false, Ordering::AcqRel) {
                entered_send
                    .send(())
                    .map_err(|e| ControlError::Runtime(e.to_string()))?;
                blocked
                    .lock()
                    .map_err(|_| ControlError::Runtime("fixture lock".into()))?
                    .recv()
                    .map_err(|e| ControlError::Runtime(e.to_string()))?;
            }
            Ok(true)
        }),
    )?;
    combat(&inputs, 10.0)?;
    inputs.start("renewed-deadline-fixture", 10.1)?;
    entered.recv_timeout(Duration::from_secs(2))?;
    inputs.set_deadline(11.0)?;
    clock.set(10.11);
    release.send(())?;
    // 이전 시한을 읽은 감시 작업이 돌아와도 갱신한 기한을 사용한다.
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(backend.events()?, json!([["down", "f4"]]));
    clock.set(11.0);
    wait_until(|| inputs.inputs().snapshot().is_ok_and(|s| s.held.is_empty()))?;
    inputs.finish(Duration::from_secs(1))?;
    assert!(inputs.set_deadline(12.0).is_err());
    assert!(inputs.set_deadline(f64::NAN).is_err());
    Ok(())
}
#[test]
fn watchdog_releases_during_idle_vision_on_deadline_focus_or_global_stop() -> TestResult {
    for reason in ["lease", "focus", "stop"] {
        let clock = Arc::new(Clock::default());
        clock.set(10.0);
        let backend = Arc::new(Backend::default());
        let parent = Cancellation::default();
        let stop = CooperativeStop::new(Some(parent.clone()));
        let focused = Arc::new(AtomicBool::new(true));
        let observed = focused.clone();
        let mut session = session(
            clock.clone(),
            backend.clone(),
            stop,
            Arc::new(move || Ok(observed.load(Ordering::Acquire))),
        )?;
        session.start("input-session-fixture", 20.0)?;
        combat(&session, 10.0)?;
        match reason {
            "lease" => clock.set(10.25),
            "focus" => focused.store(false, Ordering::Release),
            _ => parent.cancel(),
        }
        wait_until(|| {
            session
                .inputs()
                .snapshot()
                .is_ok_and(|snapshot| snapshot.held.is_empty())
        })?;
        assert_eq!(backend.events()?, json!([["down", "f4"], ["up", "f4"]]));
        session.finish(Duration::from_secs(1))?;
        assert!(session.faults()?.is_empty());
        assert!(!session.workers_retained());
    }
    Ok(())
}
#[test]
fn watchdog_fault_closes_owned_input_and_cancels_room_without_parent() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let parent = Cancellation::default();
    let stop = CooperativeStop::new(Some(parent.clone()));
    let mut session = session(
        clock,
        backend.clone(),
        stop.clone(),
        Arc::new(|| Err(ControlError::Backend("focus check failed".into()))),
    )?;
    combat(&session, 10.0)?;
    session.start("fault-fixture", 20.0)?;
    wait_until(|| stop.is_set())?;
    session.finish(Duration::from_secs(1))?;
    assert!(!parent.is_cancelled());
    assert!(session.inputs().closed());
    assert_eq!(backend.events()?, json!([["down", "f4"], ["up", "f4"]]));
    assert!(
        session
            .faults()?
            .iter()
            .any(|fault| fault.stage == "fault-fixture" && fault.message == "focus check failed")
    );
    Ok(())
}
#[test]
fn watchdog_panic_is_owned_and_releases_input() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let stop = CooperativeStop::default();
    let mut session = session(
        clock,
        backend.clone(),
        stop.clone(),
        Arc::new(|| panic!("focus panic fixture")),
    )?;
    combat(&session, 10.0)?;
    session.start("panic-fixture", 20.0)?;
    wait_until(|| stop.is_set())?;
    session.finish(Duration::from_secs(1))?;
    assert!(session.inputs().snapshot()?.held.is_empty());
    assert!(
        session
            .faults()?
            .iter()
            .any(|fault| fault.message.contains("focus panic fixture"))
    );
    Ok(())
}
#[test]
fn delayed_watchdog_retains_owner_for_explicit_finish_retry() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release = std::sync::Mutex::new(release_rx);
    let mut session = session(
        clock,
        backend.clone(),
        CooperativeStop::default(),
        Arc::new(move || {
            entered_tx
                .send(())
                .map_err(|e| ControlError::Runtime(e.to_string()))?;
            release
                .lock()
                .map_err(|e| ControlError::Runtime(e.to_string()))?
                .recv_timeout(Duration::from_secs(2))
                .map_err(|e| ControlError::Runtime(e.to_string()))?;
            Ok(true)
        }),
    )?;
    combat(&session, 10.0)?;
    session.start("delayed-fixture", 20.0)?;
    entered_rx.recv_timeout(Duration::from_secs(2))?;
    assert!(session.finish(Duration::ZERO).is_err());
    assert!(session.workers_retained());
    assert!(session.inputs().closed());
    assert!(session.inputs().snapshot()?.held.is_empty());
    release_tx.send(())?;
    session.finish(Duration::from_secs(1))?;
    assert!(!session.workers_retained());
    Ok(())
}
#[test]
fn explicit_cleanup_failure_retains_key_for_release_retry_and_blocks_restart() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let mut session = session(
        clock,
        backend.clone(),
        CooperativeStop::default(),
        Arc::new(|| Ok(true)),
    )?;
    combat(&session, 10.0)?;
    backend.fail_up("f4", true)?;
    assert!(session.finish(Duration::from_secs(1)).is_err());
    assert!(session.start("closed", 20.0).is_err());
    assert_eq!(session.inputs().snapshot()?.held, vec!["f4"]);
    backend.fail_up("f4", false)?;
    session.finish(Duration::from_secs(1))?;
    assert!(session.inputs().snapshot()?.held.is_empty());
    Ok(())
}

#[test]
fn partial_watchdog_dispatch_is_released_and_reported_once() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let stop = CooperativeStop::default();
    let mut session = session(
        clock.clone(),
        backend.clone(),
        stop.clone(),
        Arc::new(|| Ok(true)),
    )?;
    session.inputs().apply(
        &FieldDirective::new(
            FieldAction::Travel,
            "fixture",
            None,
            Some([960, 580]),
            Some(10.25),
            false,
        )?,
        10.0,
    )?;
    backend.fail_down(true)?;
    clock.set(10.05);
    session.start("partial-dispatch", 20.0)?;
    wait_until(|| stop.is_set())?;
    session.finish(Duration::from_secs(1))?;
    assert!(session.inputs().snapshot()?.held.is_empty());
    assert!(
        session
            .faults()?
            .iter()
            .any(|fault| fault.message.contains("partial key down failure"))
    );
    assert_eq!(
        backend.events()?,
        json!([["move", 960, 580], ["down", "f2"], ["up", "f2"]])
    );
    Ok(())
}

#[test]
fn delayed_pointer_confirmation_cannot_press_after_cleanup() -> TestResult {
    let clock = Arc::new(Clock::default());
    clock.set(10.0);
    let backend = Arc::new(Backend::default());
    let mut session = session(
        clock.clone(),
        backend.clone(),
        CooperativeStop::default(),
        Arc::new(|| Ok(true)),
    )?;
    session.inputs().apply(
        &FieldDirective::new(
            FieldAction::Travel,
            "fixture",
            None,
            Some([960, 580]),
            Some(10.25),
            false,
        )?,
        10.0,
    )?;
    let (entered, resume) = backend.block(false, [960, 580])?;
    clock.set(10.05);
    session.start("delayed-pointer", 20.0)?;
    entered.recv_timeout(Duration::from_secs(2))?;
    assert!(session.finish(Duration::ZERO).is_err());
    assert!(session.workers_retained());
    resume.send(())?;
    session.finish(Duration::from_secs(1))?;
    assert_eq!(backend.events()?, json!([["move", 960, 580]]));
    Ok(())
}
