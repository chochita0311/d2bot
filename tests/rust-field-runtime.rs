#[path = "support/field_backend.rs"]
mod field_backend;
use d2bot::common::buffing::{
    AutomaticBuffRenewal, BeltInspection, BeltVisibility, BuffRenewalLimits, PauseSchedule,
};
use d2bot::common::config::BotConfig;
use d2bot::common::error::ControlError;
use d2bot::common::field_control::{
    Enemy, FieldAction, FieldInputBackend, FieldLimits, FieldObservation, NavigationEvidence,
};
use d2bot::common::field_runtime::{
    BeltEvidence, FieldControlRuntime, FieldRuntimeOptions, create_field_control,
    create_field_control_with_backend, create_field_runtime,
};
use d2bot::common::survival::{
    BeltContents, BeltReading, Location, ObservationStamp, SafetyObservation,
};
use d2bot::common::survival_vision::{HudEvidence, HudReading, ResourceReading};
use d2bot::common::time::MonotonicClock;
use field_backend::{Backend, Clock};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn Error>>;
struct Fixture {
    runtime: FieldControlRuntime<Backend>,
    clock: Arc<Clock>,
    backend: Arc<Backend>,
    focused: Arc<AtomicBool>,
    bad_focus: Arc<AtomicBool>,
    sequence: Arc<AtomicU64>,
}
fn contents(count: u8) -> BeltContents {
    let mut left = count;
    let mut contents = BeltContents::new();
    for column in 1..=3 {
        let count = left.min(4);
        left -= count;
        contents.insert(
            column,
            BeltReading {
                kind: if count > 0 {
                    Some("purple_potion".into())
                } else {
                    None
                },
                count: i64::from(count),
            },
        );
    }
    contents.insert(
        4,
        BeltReading {
            kind: Some("town_portal_scroll".into()),
            count: 4,
        },
    );
    contents
}
fn stamp(sequence: u64, time: f64) -> Result<ObservationStamp, ControlError> {
    Ok(ObservationStamp::new("flash", "room", sequence, time)?)
}
fn scene(
    sequence: u64,
    time: f64,
    point: Option<[i64; 2]>,
    closed: Option<bool>,
    clear: bool,
    arrived: bool,
) -> Result<FieldObservation, ControlError> {
    Ok(FieldObservation::new(
        SafetyObservation::new(
            stamp(sequence, time)?,
            Some(1.0),
            Some(1.0),
            Location::Field,
            Some(clear),
        )?,
        closed,
        if clear { Enemy::Clear } else { Enemy::Far },
        NavigationEvidence {
            travel_aim: point,
            reachable: Some(true),
            arrived: Some(arrived),
            loot_checked: true,
            ..Default::default()
        },
    )?)
}
fn fixture(buffs: bool, confirmed: bool) -> Result<Fixture, Box<dyn Error>> {
    let config = BotConfig::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("config"))?;
    let clock = Arc::new(Clock::default());
    let backend = Arc::new(Backend::default());
    let limits = FieldLimits::new(0.5, 1000.0, 3.0, 2.0, 1.0)?;
    let mut control = create_field_control_with_backend(
        &config,
        "flash",
        "room",
        limits,
        clock.clone(),
        backend.clone(),
    )?;
    control
        .state_mut()
        .observe_belt(&stamp(0, 0.0)?, Some(&contents(12)));
    if confirmed {
        control.state_mut().confirm_buff(
            &stamp(0, 0.0)?,
            "battle_orders",
            Location::Field,
            Some(true),
        );
    }
    let renewal = if buffs {
        Some(AutomaticBuffRenewal::new(
            control.state(),
            vec![
                "w".into(),
                "a".into(),
                "a".into(),
                "s".into(),
                "d".into(),
                "f1".into(),
                "w".into(),
            ],
            PauseSchedule::new(BTreeMap::new(), 0.4)?,
            BuffRenewalLimits::new(10.0, 3.0, 3.0)?,
            limits.ages(),
        )?)
    } else {
        None
    };
    let focused = Arc::new(AtomicBool::new(true));
    let bad_focus = Arc::new(AtomicBool::new(false));
    let focus = focused.clone();
    let bad = bad_focus.clone();
    let sequence = Arc::new(AtomicU64::new(2));
    let next_sequence = sequence.clone();
    let next_clock = clock.clone();
    let options = FieldRuntimeOptions {
        clock: clock.clone(),
        focused: Arc::new(move || {
            if bad.load(Ordering::Acquire) {
                Err(ControlError::Backend("focus query failed".into()))
            } else {
                Ok(focus.load(Ordering::Acquire))
            }
        }),
        interval: Duration::from_millis(2),
        next_stamp: Box::new(move || {
            stamp(
                next_sequence.fetch_add(1, Ordering::AcqRel),
                next_clock.now(),
            )
        }),
    };
    Ok(Fixture {
        runtime: FieldControlRuntime::new(control, renewal, options)?,
        clock,
        backend,
        focused,
        bad_focus,
        sequence,
    })
}
fn wait_until(mut predicate: impl FnMut() -> Result<bool, Box<dyn Error>>) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if predicate()? {
            return Ok(());
        }
        std::thread::yield_now();
    }
    Err("condition did not complete within two seconds".into())
}

#[test]
fn latest_mailbox_watchdog_and_focus_pause_require_new_screen_after_resume() -> TestResult {
    let mut fixture = fixture(false, true)?;
    let handle = fixture.runtime.handle();
    fixture.clock.set(1.0);
    assert!(handle.publish(
        scene(2, 1.0, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    fixture.clock.set(1.1);
    assert!(handle.publish(
        scene(3, 1.1, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    assert!(!handle.publish(
        scene(2, 1.0, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    assert_eq!(fixture.runtime.pump()?.action, FieldAction::Travel);
    fixture.clock.set(1.7);
    assert_eq!(fixture.runtime.pump()?.reason, "input_lease_expired");
    assert!(handle.status()?.inputs.held.is_empty());
    fixture.focused.store(false, Ordering::Release);
    assert_eq!(fixture.runtime.pump()?.action, FieldAction::Hold);
    fixture.focused.store(true, Ordering::Release);
    assert_eq!(fixture.runtime.pump()?.action, FieldAction::Hold);
    handle.pause(false)?;
    fixture.clock.set(1.8);
    assert!(!handle.publish(
        scene(3, 1.1, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    assert!(handle.publish(
        scene(4, 1.8, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    assert_eq!(fixture.runtime.pump()?.action, FieldAction::Travel);
    fixture.runtime.stop(Duration::ZERO)?;
    assert!(handle.status()?.inputs.held.is_empty());
    Ok(())
}

#[test]
fn stale_or_cross_frame_expanded_belt_cannot_mutate_confirmed_counts() -> TestResult {
    let mut fixture = fixture(false, true)?;
    let handle = fixture.runtime.handle();
    fixture.clock.set(1.0);
    assert!(!handle.publish_with_belt(
        scene(2, 1.0, None, Some(false), true, false)?,
        Some(BeltEvidence {
            stamp: stamp(3, 1.0)?,
            contents: Some(contents(11))
        }),
        None
    )?);
    assert!(handle.publish_with_belt(
        scene(2, 1.0, None, Some(false), true, false)?,
        Some(BeltEvidence {
            stamp: stamp(2, 1.0)?,
            contents: Some(contents(11))
        }),
        None
    )?);
    assert_eq!(handle.status()?.potions, Some(12));
    fixture.clock.set(2.0);
    assert_eq!(fixture.runtime.pump()?.reason, "screen_stale");
    assert_eq!(handle.status()?.potions, Some(12));
    fixture.clock.set(2.1);
    assert!(handle.publish_with_belt(
        scene(3, 2.1, None, Some(false), true, false)?,
        Some(BeltEvidence {
            stamp: stamp(3, 2.1)?,
            contents: Some(contents(11))
        }),
        None
    )?);
    assert_eq!(fixture.runtime.pump()?.reason, "belt_not_confirmed_closed");
    assert_eq!(handle.status()?.potions, Some(11));
    fixture.runtime.stop(Duration::ZERO)?;
    Ok(())
}

#[test]
fn release_worker_runs_during_blocked_actor_and_stop_timeout_can_be_retried() -> TestResult {
    let mut fixture = fixture(false, true)?;
    let handle = fixture.runtime.handle();
    assert!(handle.publish(
        scene(1, 0.0, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    fixture.runtime.start()?;
    wait_until(|| Ok(!handle.status()?.inputs.held.is_empty()))?;
    let (entered, resume) = fixture.backend.block(true, [20, 20])?;
    fixture.clock.set(0.4);
    assert!(handle.publish(
        scene(2, 0.4, Some([20, 20]), Some(true), false, true)?,
        None
    )?);
    let started = entered.recv_timeout(Duration::from_secs(2));
    if let Err(error) = started {
        let _ = resume.send(());
        let _ = fixture.runtime.stop(Duration::from_secs(2));
        return Err(error.into());
    }
    fixture.clock.set(0.5);
    let released = wait_until(|| Ok(handle.status()?.inputs.held.is_empty()));
    let timed_out = fixture.runtime.stop(Duration::ZERO).is_err();
    let _ = resume.send(());
    let joined = fixture.runtime.stop(Duration::from_secs(2));
    released?;
    assert!(timed_out);
    joined?;
    assert!(handle.status()?.inputs.held.is_empty());
    assert!(fixture.runtime.faults()?.is_empty());
    assert_eq!(
        fixture
            .backend
            .events()?
            .as_array()
            .ok_or("events missing")?
            .iter()
            .filter(|event| event[0] == "down")
            .count(),
        1
    );
    assert!(fixture.runtime.start().is_err());
    Ok(())
}

#[test]
fn buff_dispatch_releases_travel_first_and_focus_failure_cleans_up_workers() -> TestResult {
    let mut fixture = fixture(true, true)?;
    let handle = fixture.runtime.handle();
    fixture.clock.set(139.5);
    fixture.sequence.store(2, Ordering::Release);
    handle.publish(
        scene(1, 139.5, Some([10, 10]), Some(true), false, false)?,
        None,
    )?;
    assert_eq!(fixture.runtime.pump()?.action, FieldAction::Travel);
    fixture.clock.set(140.0);
    fixture.sequence.store(4, Ordering::Release);
    handle.publish(scene(3, 140.0, None, Some(true), true, false)?, None)?;
    assert_eq!(fixture.runtime.pump()?.reason, "buff_sequence_running");
    assert!(handle.status()?.inputs.held.is_empty());
    let events = fixture.backend.events()?;
    let events = events.as_array().ok_or("events missing")?;
    let up = events
        .iter()
        .position(|event| *event == json!(["up", "f2"]))
        .ok_or("travel release missing")?;
    let buff = events
        .iter()
        .position(|event| *event == json!(["press", "w"]))
        .ok_or("buff missing")?;
    assert!(up < buff);
    fixture.bad_focus.store(true, Ordering::Release);
    fixture.runtime.start()?;
    wait_until(|| Ok(!fixture.runtime.faults()?.is_empty()))?;
    fixture.runtime.stop(Duration::from_secs(2))?;
    assert!(handle.status()?.inputs.held.is_empty());
    assert!(
        fixture
            .runtime
            .faults()?
            .iter()
            .any(|fault| fault.message.contains("focus query failed"))
    );
    Ok(())
}

#[test]
fn factory_is_dry_and_inspection_needs_fresh_positive_closed_evidence() -> TestResult {
    let read = |path: &str| -> Result<Value, Box<dyn Error>> {
        Ok(serde_json::from_slice(&std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
        )?)?)
    };
    let mut characters = read("config/characters/characters.json")?;
    characters["characters"]["flash"]["actions"]["buff_action_pause_seconds"]["s"] = json!(1e300);
    let rules = read("config/game-rules/casting.json")?;
    let config = BotConfig::from_value(
        json!({"dry_run":false,"characters":characters["characters"],"casting_rule_sets":rules["casting_rule_sets"]}),
    )?;
    assert_eq!(config.warnings().len(), 1);
    assert_eq!(
        config.warnings()[0].path,
        "/characters/flash/actions/buff_action_pause_seconds/s"
    );
    assert_eq!(config.warnings()[0].original, json!(1e300));
    assert!(
        config.snapshot()["characters"]["flash"]["actions"]["buff_action_pause_seconds"]
            .get("s")
            .is_none()
    );
    let limits = FieldLimits::new(0.5, 1000.0, 3.0, 2.0, 1.0)?;
    let clock = Arc::new(Clock::default());
    let mut control = create_field_control(&config, "flash", "room", limits, clock.clone())?;
    assert!(control.inputs().backend().is_dry_run());
    assert_eq!(control.inputs().keys(), &["f2".to_owned(), "f4".to_owned()]);
    assert!(create_field_control(&config, "constantin", "room", limits, clock.clone()).is_err());
    let stamp_clock = clock.clone();
    let mut runtime = create_field_runtime(
        &config,
        "flash",
        "room",
        limits,
        BuffRenewalLimits::new(10.0, 3.0, 3.0)?,
        FieldRuntimeOptions {
            clock,
            focused: Arc::new(|| Ok(true)),
            interval: Duration::from_millis(2),
            next_stamp: Box::new(move || stamp(1, stamp_clock.now())),
        },
    )?;
    // 중앙 loader가 먼저 정리하므로 실행 시 같은 경고를 다시 만들지 않는다.
    assert!(runtime.handle().status()?.pause_warnings.is_empty());
    runtime.stop(Duration::ZERO)?;
    let mut inspection = BeltInspection::new(control.state())?;
    assert_eq!(inspection.request_toggle(control.state())?, "`");
    assert!(inspection.observe(
        control.state_mut(),
        &stamp(1, 1.0)?,
        BeltVisibility::Expanded,
        Some(&contents(12))
    ));
    control.state_mut().confirm_buff(
        &stamp(1, 1.0)?,
        "battle_orders",
        Location::Field,
        Some(true),
    );
    let safe = SafetyObservation::new(
        stamp(3, 1.3)?,
        Some(1.0),
        Some(1.0),
        Location::Field,
        Some(true),
    )?;
    assert!(!inspection.permits_movement(control.state(), &safe, 1.3, 0.5, 1000.0)?);
    assert!(inspection.observe(
        control.state_mut(),
        &stamp(2, 1.2)?,
        BeltVisibility::Closed,
        None
    ));
    assert!(inspection.permits_movement(control.state(), &safe, 1.3, 0.5, 1000.0)?);
    assert!(!inspection.permits_movement(control.state(), &safe, 2.0, 0.5, 1000.0)?);
    assert!(!inspection.observe(
        control.state_mut(),
        &stamp(4, 1.4)?,
        BeltVisibility::Expanded,
        None
    ));
    assert_eq!(control.state().potions_remaining(), None);
    Ok(())
}

#[test]
fn failed_dispatch_and_failed_cleanup_remain_visible_until_explicit_retry() -> TestResult {
    let mut fixture = fixture(false, true)?;
    let handle = fixture.runtime.handle();
    fixture.backend.fail_down(true)?;
    fixture.backend.fail_up("f2", true)?;
    handle.publish(
        scene(1, 0.0, Some([10, 10]), Some(true), false, false)?,
        None,
    )?;
    let error = fixture
        .runtime
        .pump()
        .err()
        .ok_or("dispatch failure was hidden")?;
    assert!(error.to_string().contains("partial key down failure"));
    assert!(error.to_string().contains("key up failure"));
    assert_eq!(handle.status()?.inputs.held, vec!["f2"]);
    assert!(handle.status()?.stopped);
    fixture.backend.fail_up("f2", false)?;
    fixture.runtime.stop(Duration::ZERO)?;
    assert!(handle.status()?.inputs.held.is_empty());
    Ok(())
}

fn hud(life: &str, mana: ResourceReading) -> HudReading {
    HudReading {
        layout_id: "layout".into(),
        revision: "revision".into(),
        life: ResourceReading::from_text(life, 1.0),
        mana,
        belt_status: "unused".into(),
        slots: Vec::new(),
        belt_visibility: BeltVisibility::Closed,
        mercenary_status: None,
    }
}

#[test]
fn hud_bridge_uses_current_maximum_and_unknown_resource_preempts_travel() -> TestResult {
    let mut fixture = fixture(false, true)?;
    let handle = fixture.runtime.handle();
    fixture.clock.set(1.0);
    let evidence = HudEvidence {
        stamp: stamp(2, 1.0)?,
        reading: hud("1580/1580", ResourceReading::from_text("1543/1543", 1.0)),
    };
    assert!(handle.publish_hud(
        evidence,
        scene(2, 1.0, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    assert_eq!(fixture.runtime.pump()?.action, FieldAction::Travel);
    fixture.clock.set(1.1);
    let evidence = HudEvidence {
        stamp: stamp(3, 1.1)?,
        reading: hud("790/1600", ResourceReading::from_text("1543/1543", 1.0)),
    };
    assert!(handle.publish_hud(
        evidence,
        scene(3, 1.1, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    assert_eq!(fixture.runtime.pump()?.action, FieldAction::UsePotion);
    assert!(handle.status()?.inputs.held.is_empty());
    // A full later belt reading resolves the pending potion; an unreadable current mana stays unknown.
    fixture.clock.set(1.2);
    handle.publish_with_belt(
        scene(4, 1.2, None, Some(false), true, false)?,
        Some(BeltEvidence {
            stamp: stamp(4, 1.2)?,
            contents: Some(contents(11)),
        }),
        None,
    )?;
    fixture.runtime.pump()?;
    fixture.clock.set(1.3);
    let evidence = HudEvidence {
        stamp: stamp(5, 1.3)?,
        reading: hud("1580/1580", ResourceReading::default()),
    };
    assert!(handle.publish_hud(
        evidence,
        scene(5, 1.3, Some([10, 10]), Some(true), false, false)?,
        None
    )?);
    assert_eq!(fixture.runtime.pump()?.reason, "hud_unknown");
    assert!(handle.status()?.inputs.held.is_empty());
    fixture.runtime.stop(Duration::ZERO)?;
    Ok(())
}

#[test]
fn navigation_aim_and_single_cast_require_current_permit_and_watchdog_for_live_backend()
-> TestResult {
    let mut fixture = fixture(false, true)?;
    let handle = fixture.runtime.handle();
    fixture.clock.set(1.0);
    handle.publish(scene(1, 1.0, None, Some(true), true, false)?, None)?;
    assert_eq!(fixture.runtime.pump()?.reason, "destination_unconfirmed");
    assert!(fixture.runtime.navigation_allowed(&stamp(1, 1.0)?)?);
    assert!(!fixture.runtime.navigation_allowed(&stamp(0, 1.0)?)?);
    assert!(
        fixture
            .runtime
            .dispatch_navigation(&stamp(1, 1.0)?, [600, 400], false)?
    );
    assert_eq!(fixture.backend.events()?, json!([["move", 600, 400]]));
    assert!(
        fixture
            .runtime
            .dispatch_navigation(&stamp(1, 1.0)?, [600, 400], true)?
    );
    assert_eq!(
        fixture.backend.events()?,
        json!([["move", 600, 400], ["move", 600, 400], ["press", "f2"]])
    );
    assert!(handle.status()?.inputs.held.is_empty());
    fixture.backend.native_like.store(true, Ordering::Release);
    assert!(!fixture.runtime.navigation_allowed(&stamp(1, 1.0)?)?);
    fixture.runtime.start()?;
    assert!(fixture.runtime.navigation_allowed(&stamp(1, 1.0)?)?);
    fixture.runtime.stop(Duration::from_secs(2))?;
    let mut due = self::fixture(false, true)?;
    due.clock.set(139.5);
    due.runtime
        .handle()
        .publish(scene(1, 139.5, None, Some(true), true, false)?, None)?;
    due.runtime.pump()?;
    due.clock.set(140.0);
    assert!(!due.runtime.navigation_allowed(&stamp(1, 139.5)?)?);
    due.runtime.stop(Duration::ZERO)?;
    Ok(())
}

#[test]
fn pause_during_navigation_move_and_reply_timeout_prevent_late_cast() -> TestResult {
    for threaded in [false, true] {
        let mut fixture = fixture(false, true)?;
        let handle = fixture.runtime.handle();
        handle.publish(scene(1, 0.0, None, Some(true), true, false)?, None)?;
        fixture.runtime.pump()?;
        let (entered, resume) = fixture.backend.block(true, [600, 400])?;
        if threaded {
            fixture.runtime.start()?;
        }
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = if threaded {
            let handle = handle.clone();
            let runtime = fixture.runtime;
            std::thread::spawn(move || {
                let result = handle.dispatch_navigation(&stamp(1, 0.0)?, [600, 400], true);
                let _ = done_tx.send(());
                Ok::<_, ControlError>((runtime, result))
            })
        } else {
            let mut runtime = fixture.runtime;
            std::thread::spawn(move || {
                let result = runtime.dispatch_navigation(&stamp(1, 0.0)?, [600, 400], true);
                let _ = done_tx.send(());
                Ok::<_, ControlError>((runtime, result))
            })
        };
        let started = entered.recv_timeout(Duration::from_secs(2));
        if let Err(error) = started {
            let _ = resume.send(());
            let _ = worker.join();
            return Err(error.into());
        }
        let waited = if threaded {
            done_rx
                .recv_timeout(Duration::from_secs(1))
                .map_err(|error| error.to_string())
        } else {
            handle.pause(true).map_err(|error| error.to_string())
        };
        let _ = resume.send(());
        let (mut runtime, result) = worker.join().map_err(|_| "navigation thread panicked")??;
        waited?;
        assert!(!result?);
        runtime.stop(Duration::from_secs(2))?;
        assert!(
            fixture
                .backend
                .events()?
                .as_array()
                .ok_or("events missing")?
                .iter()
                .all(|event| event[0] != "press")
        );
    }
    Ok(())
}

#[test]
fn pause_then_resume_before_actor_pump_still_cancels_active_buff_transaction() -> TestResult {
    let mut fixture = fixture(true, false)?;
    let handle = fixture.runtime.handle();
    handle.publish(scene(1, 0.0, None, Some(true), true, false)?, None)?;
    assert_eq!(fixture.runtime.pump()?.reason, "buff_sequence_running");
    handle.pause(true)?;
    handle.pause(false)?;
    fixture.clock.set(0.5);
    fixture.sequence.store(4, Ordering::Release);
    handle.publish(scene(3, 0.5, None, Some(true), true, false)?, None)?;
    assert_eq!(fixture.runtime.pump()?.reason, "buff_paused_or_unfocused");
    assert_eq!(
        fixture
            .backend
            .events()?
            .as_array()
            .ok_or("events missing")?
            .iter()
            .filter(|event| event[0] == "press")
            .count(),
        1
    );
    fixture.runtime.stop(Duration::ZERO)?;
    Ok(())
}

#[test]
fn current_screen_pump_uses_existing_actor_before_external_navigation() -> TestResult {
    let mut fixture = fixture(false, true)?;
    let handle = fixture.runtime.handle();
    fixture.clock.set(1.0);
    handle.publish(scene(2, 1.0, None, Some(true), true, false)?, None)?;
    assert_eq!(
        fixture.runtime.pump_current()?.reason,
        "destination_unconfirmed"
    );
    assert!(fixture.runtime.navigation_allowed(&stamp(2, 1.0)?)?);
    fixture.backend.native_like.store(true, Ordering::Release);
    fixture.runtime.start()?;
    fixture.clock.set(1.1);
    handle.publish(scene(3, 1.1, None, Some(true), true, false)?, None)?;
    assert_eq!(
        fixture.runtime.pump_current()?.reason,
        "destination_unconfirmed"
    );
    assert_eq!(handle.status()?.last_stamp, Some(stamp(3, 1.1)?));
    assert!(
        fixture
            .runtime
            .dispatch_navigation(&stamp(3, 1.1)?, [300, 200], false)?
    );
    handle.release_inputs()?;
    fixture.runtime.stop(Duration::from_secs(1))?;
    assert!(!handle.running());
    assert!(handle.status()?.inputs.held.is_empty());
    assert!(fixture.runtime.pump_current().is_err());
    assert!(
        fixture
            .backend
            .events()?
            .as_array()
            .ok_or("events")?
            .iter()
            .any(|row| row == &json!(["move", 300, 200]))
    );
    Ok(())
}
#[test]
fn pump_timeout_keeps_actor_owner_until_blocked_adapter_returns() -> TestResult {
    let mut fixture = fixture(false, true)?;
    fixture.clock.set(1.0);
    let handle = fixture.runtime.handle();
    handle.publish(scene(2, 1.0, None, Some(true), true, false)?, None)?;
    fixture.runtime.pump_current()?;
    fixture.backend.native_like.store(true, Ordering::Release);
    fixture.runtime.start()?;
    let (entered, resume) = fixture.backend.block(true, [400, 300])?;
    let request = handle.clone();
    let navigation =
        std::thread::spawn(move || request.dispatch_navigation(&stamp(2, 1.0)?, [400, 300], false));
    entered.recv_timeout(Duration::from_secs(2))?;
    let mut runtime = fixture.runtime;
    let result = runtime.pump_current();
    let result_message = result.as_ref().err().map(ToString::to_string);
    let resumed = resume.send(());
    let navigation_result = navigation
        .join()
        .map_err(|_| "navigation caller panicked")?;
    runtime.stop(Duration::from_secs(1))?;
    resumed?;
    navigation_result?;
    assert!(
        result_message
            .as_deref()
            .is_some_and(|message| message.contains("field pump response unavailable"))
    );
    assert!(!handle.running());
    assert!(handle.status()?.inputs.held.is_empty());
    assert!(
        fixture
            .backend
            .events()?
            .as_array()
            .ok_or("events")?
            .iter()
            .all(|row| row[0] != "press")
    );
    Ok(())
}
