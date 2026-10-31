use d2bot::{
    common::{
        buffing::BeltVisibility,
        config::BotConfig,
        error::ControlError,
        field_control::{
            DesktopPoint, DryFieldInput, FieldAction, FieldInputBackend, HeldFieldInput,
        },
        field_preparation::PreparationEnvironment,
        frame::{BgrFrame, Size},
        survival::CharacterSurvivalPolicy,
        survival_vision::{HudReading, ResourceReading},
        time::{MonotonicClock, SystemClock},
        workers::Cancellation,
    },
    runs::RunError,
    tools::field_input_probe::hold::{self, HoldControl, HoldRequest},
};
use std::{
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
type TestResult = Result<(), Box<dyn Error>>;
struct Clock(AtomicU64);
impl MonotonicClock for Clock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}
struct Environment {
    clock: Arc<dyn MonotonicClock>,
    stop: Cancellation,
    life: u64,
    grabbed: usize,
    blocked: Option<mpsc::SyncSender<()>>,
    resume: Option<mpsc::Receiver<()>>,
}
impl PreparationEnvironment for Environment {
    type Error = RunError;
    fn clock(&self) -> f64 {
        self.clock.now()
    }
    fn wall_clock(&self) -> f64 {
        1000.0 + self.clock.now()
    }
    fn stopped(&self) -> bool {
        self.stop.is_cancelled()
    }
    fn focused(&self) -> Result<bool, RunError> {
        Ok(true)
    }
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError> {
        self.grabbed += 1;
        if self.grabbed == 2 {
            if let Some(blocked) = self.blocked.take() {
                let _ = blocked.send(());
            }
            if let Some(resume) = self.resume.take() {
                resume
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|e| RunError::Runtime(e.to_string()))?;
            }
        }
        Ok(Arc::new(
            BgrFrame::zeros(Size::new(1, 1).map_err(|e| RunError::Runtime(e.to_string()))?)
                .map_err(|e| RunError::Runtime(e.to_string()))?,
        ))
    }
    fn hud(&mut self, _: &Arc<BgrFrame>) -> Result<HudReading, RunError> {
        Ok(HudReading {
            layout_id: "fake".into(),
            revision: "fake".into(),
            life: ResourceReading::from_counts(self.life, 100, "read", None),
            mana: ResourceReading::from_counts(100, 100, "read", None),
            belt_status: "read".into(),
            slots: vec![],
            belt_visibility: BeltVisibility::Closed,
            mercenary_status: None,
        })
    }
    fn press(&mut self, _: &str) -> Result<(), RunError> {
        Ok(())
    }
    fn wait(&mut self, seconds: f64) -> Result<(), RunError> {
        self.stop.wait(Duration::from_secs_f64(seconds))?;
        Ok(())
    }
}
struct Backend {
    events: Mutex<Vec<String>>,
    released: mpsc::SyncSender<()>,
}
impl FieldInputBackend for Backend {
    fn key_down(&self, key: &str) -> Result<(), ControlError> {
        self.events
            .lock()
            .map_err(|_| ControlError::Runtime("events".into()))?
            .push(format!("down:{key}"));
        Ok(())
    }
    fn key_up(&self, key: &str) -> Result<(), ControlError> {
        self.events
            .lock()
            .map_err(|_| ControlError::Runtime("events".into()))?
            .push(format!("up:{key}"));
        let _ = self.released.try_send(());
        Ok(())
    }
    fn press(&self, _: &str) -> Result<(), ControlError> {
        Ok(())
    }
    fn move_pointer(&self, _: DesktopPoint) -> Result<(), ControlError> {
        Ok(())
    }
}
fn policy() -> Result<CharacterSurvivalPolicy, Box<dyn Error>> {
    let c = BotConfig::load("config")?;
    Ok(CharacterSurvivalPolicy::from_value(
        &c.section("characters").ok_or("characters")?["flash"]["survival"],
    )?)
}
#[test]
fn deadline_releases_held_key_while_the_capture_owner_is_still_blocked() -> TestResult {
    let clock = Arc::new(Clock(AtomicU64::new(0.0f64.to_bits())));
    let stop = Cancellation::default();
    let policy = policy()?;
    let (blocked, seen_block) = mpsc::sync_channel(1);
    let (resume, wait_resume) = mpsc::sync_channel(1);
    let (released, seen_release) = mpsc::sync_channel(1);
    let backend = Arc::new(Backend {
        events: Mutex::new(vec![]),
        released,
    });
    let inputs = HeldFieldInput::new(backend.clone(), "f2", "f4", Some(clock.clone()), None, 0.0)?;
    let mut env = Environment {
        clock: clock.clone(),
        stop: stop.clone(),
        life: 100,
        grabbed: 0,
        blocked: Some(blocked),
        resume: Some(wait_resume),
    };
    let control = HoldControl {
        inputs,
        clock: clock.clone(),
        stop,
        focused: Arc::new(|| Ok(true)),
    };
    let worker = std::thread::spawn(move || {
        hold::hold_probe(
            &mut env,
            &policy,
            control,
            HoldRequest {
                action: FieldAction::Travel,
                seconds: 0.1,
                aim: [10, 10],
                buff_deadline: 1.0,
            },
        )
    });
    seen_block.recv_timeout(Duration::from_secs(1))?;
    clock.0.store(0.2f64.to_bits(), Ordering::Release);
    seen_release.recv_timeout(Duration::from_secs(1))?;
    assert!(!worker.is_finished());
    assert_eq!(
        *backend.events.lock().map_err(|_| "events")?,
        ["down:f2", "up:f2"]
    );
    resume.send(())?;
    let result = worker.join().map_err(|_| "probe thread panic")??;
    assert_eq!(result["status"], "duration_complete");
    assert_eq!(result["held_after"], serde_json::json!([]));
    assert_eq!(result["errors"], serde_json::json!([]));
    Ok(())
}
#[test]
fn low_resource_and_renewal_monitor_refuse_input_and_report_errors() -> TestResult {
    let clock: Arc<dyn MonotonicClock> = Arc::new(SystemClock::default());
    let stop = Cancellation::default();
    let policy = policy()?;
    let backend = Arc::new(DryFieldInput::default());
    let inputs = HeldFieldInput::new(backend.clone(), "f2", "f4", Some(clock.clone()), None, 0.0)?;
    let mut env = Environment {
        clock: clock.clone(),
        stop: stop.clone(),
        life: 1,
        grabbed: 0,
        blocked: None,
        resume: None,
    };
    let result = hold::hold_probe(
        &mut env,
        &policy,
        HoldControl {
            inputs,
            clock: clock.clone(),
            stop,
            focused: Arc::new(|| Ok(true)),
        },
        HoldRequest {
            action: FieldAction::Travel,
            seconds: 0.1,
            aim: [10, 10],
            buff_deadline: 100.0,
        },
    )?;
    assert_eq!(result["status"], "resource_low");
    assert!(backend.events()?.is_empty());
    env.stop = Cancellation::default();
    let due = clock.now() + 0.05;
    let result = hold::wait_renewal_deadline(&mut env, &policy, due, |_| Ok(()));
    assert!(result.is_err());
    env.life = 100;
    let due = clock.now() + 0.03;
    let mut progress = vec![];
    hold::wait_renewal_deadline(&mut env, &policy, due, |remaining| {
        progress.push(remaining);
        Ok(())
    })?;
    assert_eq!(progress.len(), 1);
    assert!(progress[0] > 0.0 && progress[0] <= 0.03);
    Ok(())
}
