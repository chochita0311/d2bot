#[path = "support/field_backend.rs"]
mod field_backend;
mod support;

use d2bot::common::casting::{CastingCatalog, CharacterCastingProfile};
use d2bot::common::field_control::{
    AdaptiveTeleportCadence, Enemy, FieldAction, FieldControlLoop, FieldDirective, FieldLimits,
    FieldObservation, HeldFieldInput, NavigationEvidence,
};
use d2bot::common::survival::{
    BeltContents, BeltReading, CharacterSurvivalPolicy, CharacterSurvivalState, Location,
    ObservationStamp, SafetyObservation,
};
use field_backend::{Backend, Clock};
use serde_json::{Value, json};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;
fn stamp(sequence: u64, time: f64) -> Value {
    json!({"character_id":"flash","room_id":"room","sequence":sequence,"observed_at":time})
}
fn raw_stamp(raw: &Value) -> Result<ObservationStamp, Box<dyn Error>> {
    Ok(ObservationStamp::new(
        raw["character_id"].as_str().ok_or("ID missing")?,
        raw["room_id"].as_str().ok_or("room missing")?,
        raw["sequence"].as_u64().ok_or("sequence missing")?,
        raw["observed_at"].as_f64().ok_or("time missing")?,
    )?)
}
fn belt(count: u8) -> Value {
    let mut left = count;
    let mut raw = serde_json::Map::new();
    for column in 1..=3 {
        let count = left.min(4);
        left -= count;
        raw.insert(
            column.to_string(),
            json!([
                if count > 0 {
                    Some("purple_potion")
                } else {
                    None
                },
                count
            ]),
        );
    }
    raw.insert("4".into(), json!(["town_portal_scroll", 4]));
    Value::Object(raw)
}
fn contents(raw: &Value) -> Option<BeltContents> {
    raw.as_object()?
        .iter()
        .map(|(key, value)| {
            Some((
                key.parse().ok()?,
                BeltReading {
                    kind: value[0].as_str().map(Into::into),
                    count: value[1].as_i64()?,
                },
            ))
        })
        .collect()
}
fn fixture(operations: Vec<Value>) -> Result<Value, Box<dyn Error>> {
    let read = |path: &str| -> Result<Value, Box<dyn Error>> {
        Ok(serde_json::from_slice(&std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
        )?)?)
    };
    let characters = read("config/characters/characters.json")?;
    let rules = read("config/game-rules/casting.json")?;
    Ok(
        json!({"policy":characters["characters"]["flash"]["survival"],"casting":characters["characters"]["flash"]["casting"],"catalog":rules["casting_rule_sets"],"initial_belt":belt(12),"operations":operations}),
    )
}
fn point(value: &Value) -> Option<[i64; 2]> {
    Some([value[0].as_i64()?, value[1].as_i64()?])
}

fn run(request: &Value) -> Result<Value, Box<dyn Error>> {
    let mut state = CharacterSurvivalState::new(
        "flash",
        "room",
        Some(Arc::new(CharacterSurvivalPolicy::from_value(
            &request["policy"],
        )?)),
    )?;
    let initial = ObservationStamp::new("flash", "room", 0, 0.0)?;
    state.observe_belt(&initial, contents(&request["initial_belt"]).as_ref());
    state.confirm_buff(&initial, "battle_orders", Location::Field, Some(true));
    let backend = Arc::new(Backend::default());
    let inputs = HeldFieldInput::new(backend.clone(), "a", "d", None, None, 0.0)?;
    let estimate = CastingCatalog::from_value(&request["catalog"])?.estimate(
        "resurrection",
        Some("sorceress"),
        Some(&CharacterCastingProfile::from_value(&request["casting"])?),
    )?;
    let mut control = FieldControlLoop::new(
        state,
        inputs,
        FieldLimits::new(1.0, 1000.0, 2.0, 1.0, 1.0)?,
        AdaptiveTeleportCadence::new(&estimate, 1.0)?,
    )?;
    let mut results = Vec::new();
    for operation in request["operations"]
        .as_array()
        .ok_or("operations missing")?
    {
        let now = operation.get("now").and_then(Value::as_f64).unwrap_or(0.0);
        let result = match operation["op"].as_str() {
            Some("tick") => json!(control.tick(now)?),
            Some("pause") => {
                control.pause(operation["paused"].as_bool().ok_or("pause missing")?)?;
                Value::Null
            }
            Some("stop") => {
                control.stop()?;
                Value::Null
            }
            Some("ack") => json!(
                control.acknowledge_potion(
                    &raw_stamp(&operation["stamp"])?,
                    operation["column"]
                        .as_u64()
                        .and_then(|column| u8::try_from(column).ok())
                        .unwrap_or(0)
                )
            ),
            Some("belt") => json!(control.reconcile_belt(
                &raw_stamp(&operation["stamp"])?,
                contents(&operation["contents"]).as_ref()
            )),
            _ => {
                let enemy = match operation["enemy"].as_str().unwrap_or("far") {
                    "near" => Enemy::Near,
                    "far" => Enemy::Far,
                    "clear" => Enemy::Clear,
                    _ => Enemy::Unknown,
                };
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
                        .map_or(Some(enemy == Enemy::Clear), Value::as_bool),
                )?;
                let scene = FieldObservation::new(
                    safety,
                    operation
                        .get("belt_closed")
                        .map_or(Some(true), Value::as_bool),
                    enemy,
                    NavigationEvidence {
                        travel_aim: point(&operation["aim"]),
                        hunt_aim: point(&operation["hunt"]),
                        reachable: operation
                            .get("reachable")
                            .map_or(Some(true), Value::as_bool),
                        arrived: operation["arrived"].as_bool(),
                        loot_checked: operation["loot_checked"].as_bool().unwrap_or(false),
                    },
                )?;
                json!(control.observe(
                    &scene,
                    now,
                    operation["focused"].as_bool().unwrap_or(true)
                )?)
            }
        };
        results.push(json!({"value":result,"phase":control.phase(),"pending":control.potion_pending(),"counts":control.state().belt_counts(),"snapshot":control.inputs().snapshot()?,"events":backend.events()?}));
    }
    Ok(json!(results))
}

fn assert_values(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
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
fn field_priorities_loot_cadence_potion_confirmation_and_clock_match_python() -> TestResult {
    let scenarios = vec![
        vec![
            json!({"op":"observe","stamp":stamp(1,0.1),"now":0.1,"aim":[10,10]}),
            json!({"op":"observe","stamp":stamp(2,0.2),"now":0.2,"enemy":"near"}),
            json!({"op":"observe","stamp":stamp(3,0.3),"now":0.3,"enemy":"far","aim":[20,20]}),
            json!({"op":"observe","stamp":stamp(4,0.4),"now":0.4,"enemy":"clear"}),
            json!({"op":"observe","stamp":stamp(5,0.5),"now":0.5,"enemy":"clear","aim":[20,20]}),
            json!({"op":"observe","stamp":stamp(6,0.6),"now":0.6,"enemy":"clear","loot_checked":true,"aim":[20,20]}),
        ],
        vec![
            json!({"op":"observe","stamp":stamp(1,1.0),"now":1.0,"life":0.1}),
            json!({"op":"observe","stamp":stamp(2,1.2),"now":1.2,"life":0.1}),
            json!({"op":"observe","stamp":stamp(3,2.0),"now":2.0,"life":0.1}),
            json!({"op":"ack","stamp":stamp(1,2.0),"column":1}),
            json!({"op":"ack","stamp":stamp(4,2.0),"column":1}),
            json!({"op":"observe","stamp":stamp(5,2.1),"now":2.1,"aim":[10,10]}),
        ],
        vec![
            json!({"op":"observe","stamp":stamp(1,1.0),"now":1.0,"life":0.1}),
            json!({"op":"belt","stamp":stamp(2,1.1),"contents":{"1":["purple_potion",3]}}),
            json!({"op":"observe","stamp":stamp(3,1.2),"now":1.2}),
            json!({"op":"belt","stamp":stamp(4,1.3),"contents":belt(0)}),
            json!({"op":"observe","stamp":stamp(5,1.4),"now":1.4,"enemy":"unknown","life":null}),
        ],
        vec![
            json!({"op":"belt","stamp":stamp(1,0.0),"contents":belt(6)}),
            json!({"op":"observe","stamp":stamp(2,0.1),"now":0.1,"aim":[10,10],"hunt":[20,20]}),
            json!({"op":"belt","stamp":stamp(3,0.2),"contents":belt(7)}),
            json!({"op":"observe","stamp":stamp(4,0.3),"now":0.3,"aim":[10,10],"hunt":[20,20],"arrived":true}),
            json!({"op":"belt","stamp":stamp(5,0.4),"contents":belt(12)}),
            json!({"op":"observe","stamp":stamp(6,0.6),"now":0.6,"aim":[10,10],"hunt":[20,20],"arrived":true}),
        ],
        vec![
            json!({"op":"observe","stamp":stamp(1,0.0),"now":0.0,"aim":[10,10]}),
            json!({"op":"observe","stamp":stamp(2,0.5),"now":0.5,"aim":[20,20]}),
            json!({"op":"observe","stamp":stamp(3,1.0),"now":1.0,"aim":[30,30]}),
            json!({"op":"tick","now":140.0}),
        ],
        vec![
            json!({"op":"observe","stamp":stamp(1,1.0),"now":1.0,"aim":[10,10]}),
            json!({"op":"observe","stamp":stamp(1,1.0),"now":1.1,"aim":[10,10]}),
            json!({"op":"pause","paused":true}),
            json!({"op":"observe","stamp":stamp(2,1.2),"now":1.2,"aim":[10,10]}),
            json!({"op":"pause","paused":false}),
            json!({"op":"observe","stamp":stamp(3,1.3),"now":1.3,"aim":[10,10]}),
            json!({"op":"tick","now":1.0}),
        ],
    ];
    let requests = scenarios
        .into_iter()
        .map(fixture)
        .collect::<Result<Vec<_>, _>>()?;
    let expected = support::legacy_expectation("field-oracle", &json!(requests))?;
    for (index, request) in requests.iter().enumerate() {
        assert_values(&run(request)?, &expected[index], &format!("case/{index}"));
    }
    Ok(())
}

fn directive(point: [i64; 2], deadline: f64) -> Result<FieldDirective, Box<dyn Error>> {
    Ok(FieldDirective::new(
        FieldAction::Travel,
        "test",
        None,
        Some(point),
        Some(deadline),
        false,
    )?)
}

#[test]
fn partial_down_and_failed_up_remain_owned_until_cleanup_succeeds() -> TestResult {
    let backend = Arc::new(Backend::default());
    let inputs = HeldFieldInput::new(backend.clone(), "a", "d", None, None, 0.0)?;
    backend.fail_down(true)?;
    backend.fail_up("a", true)?;
    assert!(inputs.apply(&directive([10, 10], 1.0)?, 0.0).is_err());
    assert!(inputs.closed());
    assert_eq!(inputs.snapshot()?.held, vec!["a"]);
    assert!(inputs.fault()?.is_some());
    backend.fail_up("a", false)?;
    inputs.close()?;
    assert!(inputs.snapshot()?.held.is_empty());
    assert_eq!(
        backend.events()?,
        json!([["move", 10, 10], ["down", "a"], ["up", "a"], ["up", "a"]])
    );
    Ok(())
}

#[test]
fn blocked_pointer_work_cannot_block_release_or_rehold_after_invalidation() -> TestResult {
    for (movement, stop) in [(true, false), (true, true), (false, false), (false, true)] {
        let backend = Arc::new(Backend::default());
        let clock = Arc::new(Clock::default());
        let inputs =
            HeldFieldInput::new(backend.clone(), "a", "d", Some(clock.clone()), None, 0.0)?;
        inputs.apply(&directive([10, 10], 1.0)?, 0.0)?;
        let blocked = if movement { [20, 20] } else { [10, 10] };
        let (entered, resume) = backend.block(movement, blocked)?;
        clock.set(0.2);
        let delayed = inputs.clone();
        let next = directive([20, 20], 1.5)?;
        let worker = std::thread::spawn(move || delayed.apply(&next, 0.2));
        let started = entered.recv_timeout(Duration::from_secs(2));
        if let Err(error) = started {
            let _ = resume.send(());
            let _ = worker.join();
            return Err(error.into());
        }
        clock.set(1.0);
        let released = if stop {
            inputs.close()
        } else {
            inputs.service(1.0)
        };
        let before_resume = inputs.snapshot();
        let _ = resume.send(());
        let finished = worker.join().map_err(|_| "pointer worker panicked")?;
        released?;
        finished?;
        assert!(before_resume?.held.is_empty());
        assert!(inputs.snapshot()?.held.is_empty());
        let events = backend.events()?;
        assert_eq!(
            events
                .as_array()
                .ok_or("events missing")?
                .iter()
                .filter(|event| event[0] == "down")
                .count(),
            1,
            "movement={movement}, stop={stop}: {events}"
        );
    }
    Ok(())
}
