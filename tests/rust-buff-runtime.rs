mod support;

use d2bot::common::buffing::{
    AutomaticBuffRenewal, BuffAction, BuffConfirmation, BuffEnvironment, BuffExecutionEnvironment,
    BuffObservation, BuffRenewalLimits, BuffSequence, BuffSequenceExecutor, ConfirmationSource,
    PauseSchedule,
};
use d2bot::common::survival::{
    BeltContents, BeltReading, CharacterSurvivalPolicy, CharacterSurvivalState, Location,
    ObservationLimits, ObservationStamp, SafetyObservation,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

type TestError = Box<dyn Error + Send + Sync>;
type TestResult = Result<(), TestError>;

fn fixture() -> Result<(Value, CharacterSurvivalState), TestError> {
    let raw: Value = serde_json::from_slice(&std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("config/characters/characters.json"),
    )?)?;
    let raw = raw["characters"]["flash"]["survival"].clone();
    let policy = Arc::new(CharacterSurvivalPolicy::from_value(&raw)?);
    let mut state = CharacterSurvivalState::new("flash", "room", Some(policy))?;
    state.observe_belt(
        &ObservationStamp::new("flash", "room", 0, 0.0)?,
        Some(&contents()),
    );
    Ok((raw, state))
}
fn contents() -> BeltContents {
    (1..=4)
        .map(|column| {
            (
                column,
                BeltReading {
                    kind: Some(
                        if column == 4 {
                            "town_portal_scroll"
                        } else {
                            "purple_potion"
                        }
                        .into(),
                    ),
                    count: 4,
                },
            )
        })
        .collect()
}
fn stamp(sequence: u64, time: f64) -> Value {
    json!({"character_id":"flash","room_id":"room","sequence":sequence,"observed_at":time})
}
fn raw_stamp(raw: &Value) -> Result<ObservationStamp, TestError> {
    Ok(ObservationStamp::new(
        "flash",
        "room",
        raw["sequence"].as_u64().ok_or("missing sequence")?,
        raw["observed_at"].as_f64().ok_or("missing time")?,
    )?)
}
fn safety(stamp: ObservationStamp) -> Result<SafetyObservation, TestError> {
    Ok(SafetyObservation::new(
        stamp,
        Some(1.0),
        Some(1.0),
        Location::Field,
        Some(true),
    )?)
}

#[derive(Default)]
struct Environment {
    now: f64,
    sequence: u64,
    offset: f64,
    delay: f64,
    sent: Vec<Value>,
    waits: Vec<f64>,
    scenes: VecDeque<BuffObservation>,
    interrupted: bool,
    succeeds: bool,
    interrupt_wait: bool,
}
impl BuffEnvironment for Environment {
    type Error = TestError;
    fn clock(&self) -> f64 {
        self.now
    }
    fn interrupted(&self) -> Result<bool, Self::Error> {
        Ok(self.interrupted)
    }
    fn send_token(&mut self, token: &str) -> Result<bool, Self::Error> {
        self.sent.push(json!([token, self.now]));
        self.now += self.delay;
        Ok(self.succeeds)
    }
    fn next_stamp(&mut self) -> Result<ObservationStamp, Self::Error> {
        Ok(ObservationStamp::new(
            "flash",
            "room",
            self.sequence,
            self.now + self.offset,
        )?)
    }
}
impl BuffExecutionEnvironment for Environment {
    fn observe(&mut self) -> Result<BuffObservation, Self::Error> {
        if let Some(scene) = self.scenes.pop_front() {
            return Ok(scene);
        }
        self.sequence += 1;
        let result = BuffObservation {
            safety: safety(ObservationStamp::new(
                "flash",
                "room",
                self.sequence,
                self.now,
            )?)?,
            belt_closed: Some(true),
        };
        self.sequence += 1;
        Ok(result)
    }
    fn wait(&mut self, seconds: f64) -> Result<bool, Self::Error> {
        self.waits.push(seconds);
        self.now += seconds;
        Ok(self.interrupt_wait)
    }
}

#[test]
fn executor_preserves_all_tokens_gaps_and_separate_effect_confirmation() -> TestResult {
    let (_, state) = fixture()?;
    let order: Vec<_> = ["w", "a", "a", "s", "d", "f1", "w"]
        .into_iter()
        .map(String::from)
        .collect();
    let pauses = PauseSchedule::new(BTreeMap::from([("a".into(), 0.2), ("s".into(), 0.8)]), 0.4)?;
    let mut executor = BuffSequenceExecutor::new(
        BuffSequence::new(&state, &order)?,
        pauses,
        ObservationLimits::new(1.0, 1000.0)?,
        10.0,
    )?;
    let mut environment = Environment {
        succeeds: true,
        delay: 0.05,
        ..Default::default()
    };
    let result = executor.run(&state, &mut environment)?;
    assert_eq!(result.action, BuffAction::AwaitConfirmation);
    assert_eq!(
        environment
            .sent
            .iter()
            .map(|entry| entry[0].as_str())
            .collect::<Vec<_>>(),
        order
            .iter()
            .map(|token| Some(token.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(environment.waits, vec![0.4, 0.2, 0.2, 0.8, 0.4, 0.4, 0.4]);
    assert_eq!(state.next_buff_renewal_at(), None);
    assert_eq!(state.potions_remaining(), Some(12));
    Ok(())
}

#[test]
fn invalid_optional_pauses_repair_at_duration_boundary_and_executor_cancels() -> TestResult {
    let pauses = PauseSchedule::new(
        BTreeMap::from([
            ("nan".into(), f64::NAN),
            ("inf".into(), f64::INFINITY),
            ("negative".into(), -1.0),
            ("huge".into(), 1e300),
            ("zero".into(), 0.0),
            ("valid".into(), 0.25),
        ]),
        0.4,
    )?;
    assert_eq!(pauses.warnings().len(), 4);
    for token in ["nan", "inf", "negative", "huge", "missing"] {
        assert_eq!(pauses.seconds(token), 0.4);
    }
    assert_eq!(pauses.seconds("zero"), 0.0);
    assert_eq!(pauses.seconds("valid"), 0.25);
    for (succeeds, interrupt_wait, timeout, reason) in [
        (false, false, 10.0, "dispatch_unconfirmed"),
        (true, true, 10.0, "sequence_interrupted"),
        (true, false, 0.1, "sequence_interrupted"),
    ] {
        let (_, state) = fixture()?;
        let mut executor = BuffSequenceExecutor::new(
            BuffSequence::new(&state, &["s".into(), "w".into()])?,
            pauses.clone(),
            ObservationLimits::new(1.0, 1000.0)?,
            timeout,
        )?;
        let mut environment = Environment {
            succeeds,
            interrupt_wait,
            delay: 0.05,
            ..Default::default()
        };
        assert_eq!(executor.run(&state, &mut environment)?.reason, reason);
        assert_eq!(environment.sent.len(), 1);
        assert_eq!(
            executor.sequence().phase(),
            d2bot::common::buffing::BuffPhase::Cancelled
        );
        assert_eq!(state.next_buff_renewal_at(), None);
    }
    Ok(())
}

fn run(request: &Value) -> Result<Value, TestError> {
    let (_, mut state) = fixture()?;
    if let Some(time) = request["confirmed_at"].as_f64() {
        state.confirm_buff(
            &ObservationStamp::new("flash", "room", 0, time)?,
            "battle_orders",
            Location::Field,
            Some(true),
        );
    }
    let order = request["order"]
        .as_array()
        .ok_or("order missing")?
        .iter()
        .map(|token| token.as_str().unwrap_or("").into())
        .collect();
    let raw_limits = request.get("limits").cloned().unwrap_or(json!([10, 2, 3]));
    let limits = BuffRenewalLimits::new(
        raw_limits[0].as_f64().ok_or("limit missing")?,
        raw_limits[1].as_f64().ok_or("limit missing")?,
        raw_limits[2].as_f64().ok_or("limit missing")?,
    )?;
    let screen_age = request
        .get("screen_age")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let mut renewal = AutomaticBuffRenewal::new(
        &state,
        order,
        PauseSchedule::new(BTreeMap::new(), 0.4)?,
        limits,
        ObservationLimits::new(screen_age, 1000.0)?,
    )?;
    let mut environment = Environment {
        delay: request
            .get("dispatch_delay")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        succeeds: request
            .get("dispatch_succeeds")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        ..Default::default()
    };
    let mut results = Vec::new();
    for operation in request["operations"]
        .as_array()
        .ok_or("operations missing")?
    {
        let now = operation["now"].as_f64().ok_or("now missing")?;
        environment.now = now;
        environment.sequence = operation
            .get("ack_sequence")
            .and_then(Value::as_u64)
            .unwrap_or(operation["stamp"]["sequence"].as_u64().unwrap_or(0) + 1);
        environment.interrupted = operation
            .get("interrupted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        environment.offset = operation
            .get("ack_offset")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let result = match operation["op"].as_str() {
            Some("tick") => renewal.tick(&state, now)?,
            Some("interrupt") => {
                renewal.interrupt(operation["reason"].as_str().unwrap_or("interrupt"));
                None
            }
            _ => {
                let location = match operation["location"].as_str().unwrap_or("field") {
                    "field" => Location::Field,
                    "town" => Location::Town,
                    _ => Location::Unknown,
                };
                let safety = SafetyObservation::new(
                    raw_stamp(&operation["stamp"])?,
                    operation.get("life").map_or(Some(1.0), Value::as_f64),
                    operation.get("mana").map_or(Some(1.0), Value::as_f64),
                    location,
                    operation
                        .get("monsters_clear")
                        .map_or(Some(true), Value::as_bool),
                )?;
                let confirmation = operation["confirm"]
                    .as_bool()
                    .filter(|confirmed| *confirmed)
                    .map(|_| BuffConfirmation {
                        stamp: safety.stamp().clone(),
                        source: ConfirmationSource::ValidatedVision,
                        effect_verified: true,
                        equipment_restored: true,
                        sequence_effects_verified: true,
                    });
                renewal.observe(
                    &mut state,
                    &BuffObservation {
                        safety,
                        belt_closed: operation
                            .get("belt_closed")
                            .map_or(Some(true), Value::as_bool),
                    },
                    now,
                    &mut environment,
                    confirmation.as_ref(),
                )?
            }
        };
        results.push(json!({"value":result,"pending":renewal.pending(),"review":renewal.review_reason(),"phase":renewal.sequence().map(BuffSequence::phase),"next_step":renewal.next_step_at(),"renewal":state.next_buff_renewal_at(),"sent":environment.sent}));
    }
    Ok(json!({"value":results}))
}

#[test]
fn renewal_watchdogs_delayed_frames_preemption_and_confirmation_match_python() -> TestResult {
    let (policy, _) = fixture()?;
    let cases = vec![
        json!({"order":["s","w"],"dispatch_delay":0.05,"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0},{"op":"observe","stamp":stamp(3,1.2),"now":1.2},{"op":"observe","stamp":stamp(4,1.5),"now":1.5},{"op":"observe","stamp":stamp(6,2.0),"now":2.0,"confirm":true}]}),
        json!({"order":["s","w"],"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0},{"op":"tick","now":2.00001},{"op":"observe","stamp":stamp(4,2.1),"now":2.1}]}),
        json!({"order":["s","w"],"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0},{"op":"observe","stamp":stamp(2,1.2),"now":1.2},{"op":"observe","stamp":stamp(3,1.3),"now":1.3,"monsters_clear":false}]}),
        json!({"order":["s"],"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0,"ack_offset":2}]}),
        json!({"order":["s"],"dispatch_succeeds":false,"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0}]}),
        json!({"order":["s","w"],"screen_age":100,"limits":[2,2,3],"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0},{"op":"tick","now":3.0}]}),
        json!({"order":["s"],"screen_age":100,"limits":[10,2,3],"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0},{"op":"tick","now":3.0}]}),
        json!({"order":["s"],"screen_age":100,"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0,"monsters_clear":false},{"op":"tick","now":4.0}]}),
        json!({"order":["s"],"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0},{"op":"tick","now":0.9}]}),
        json!({"order":["s"],"confirmed_at":0,"operations":[{"op":"tick","now":140.0},{"op":"observe","stamp":stamp(4,140.1),"now":140.1}]}),
        json!({"order":["s"],"confirmed_at":0,"operations":[{"op":"observe","stamp":stamp(1,139.9),"now":139.9},{"op":"tick","now":140.0},{"op":"observe","stamp":stamp(4,140.1),"now":140.1},{"op":"observe","stamp":stamp(6,140.6),"now":140.6,"confirm":true}]}),
        json!({"order":["s","w"],"operations":[{"op":"observe","stamp":stamp(1,1.0),"now":1.0},{"op":"observe","stamp":stamp(3,1.5),"now":1.5,"life":0.1}]}),
    ];
    let requests: Vec<_> = cases.into_iter().map(|mut request| { request["mode"] = json!("buff_runtime"); request["policy"] = policy.clone(); request["initial_stamp"] = stamp(0,0.0); request["initial_belt"] = json!({"1":["purple_potion",4],"2":["purple_potion",4],"3":["purple_potion",4],"4":["town_portal_scroll",4]}); request }).collect();
    let expected = support::legacy_expectation("policy-oracle", &json!(requests))
        .map_err(|error| error.to_string())?;
    for (index, request) in requests.iter().enumerate() {
        let actual = run(request)?;
        assert_values(&actual, &expected[index], &format!("case/{index}"));
    }
    Ok(())
}

fn assert_values(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        // These fake clocks/counters are small; Python may preserve 0 while Rust writes 0.0.
        (Value::Number(actual), Value::Number(expected)) => {
            assert_eq!(actual.as_f64(), expected.as_f64(), "{path}")
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_values(actual, expected, &format!("{path}/{index}"));
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{path}");
            for (key, expected) in expected {
                assert_values(&actual[key], expected, &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn transactions_bind_the_owning_state_even_with_identical_context_and_policy() -> TestResult {
    let (raw, _) = fixture()?;
    let policy = Arc::new(CharacterSurvivalPolicy::from_value(&raw)?);
    let mut first = CharacterSurvivalState::new("flash", "room", Some(policy.clone()))?;
    let mut other = CharacterSurvivalState::new("flash", "room", Some(policy))?;
    let initial = ObservationStamp::new("flash", "room", 0, 0.0)?;
    first.observe_belt(&initial, Some(&contents()));
    other.observe_belt(&initial, Some(&contents()));
    let observation = safety(ObservationStamp::new("flash", "room", 1, 1.0)?)?;
    let mut sequence = BuffSequence::new(&first, &["s".into()])?;
    assert_eq!(
        sequence
            .next_step(&other, &observation, 1.0, 1.0, 1000.0)?
            .reason,
        "wrong_or_old_context"
    );
    assert_eq!(
        sequence
            .next_step(&first, &observation, 1.0, 1.0, 1000.0)?
            .action,
        BuffAction::InputRequest
    );
    let ack = ObservationStamp::new("flash", "room", 2, 1.0)?;
    assert!(!sequence.acknowledge_step(&other, 0, &ack, true));
    assert!(sequence.acknowledge_step(&first, 0, &ack, true));
    let mut renewal = AutomaticBuffRenewal::new(
        &first,
        vec!["s".into()],
        PauseSchedule::new(BTreeMap::new(), 0.4)?,
        BuffRenewalLimits::new(10.0, 2.0, 3.0)?,
        ObservationLimits::new(1.0, 1000.0)?,
    )?;
    assert!(renewal.tick(&other, 1.0).is_err());
    let mut inspection = d2bot::common::buffing::BeltInspection::new(&first)?;
    assert!(inspection.request_toggle(&other).is_err());
    assert!(!inspection.observe(
        &mut other,
        &ack,
        d2bot::common::buffing::BeltVisibility::Closed,
        None
    ));
    Ok(())
}
