mod support;

use d2bot::common::buffing::{BuffConfirmation, BuffSequence, ConfirmationSource};
use d2bot::common::casting::{CastingCatalog, CharacterCastingProfile};
use d2bot::common::config::ConfigError;
use d2bot::common::survival::{
    BeltContents, BeltReading, CharacterSurvivalPolicy, CharacterSurvivalState, Location,
    ObservationLimits, ObservationStamp, SafetyObservation,
};
use serde_json::{Value, json};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

type TestResult = Result<(), Box<dyn Error>>;

fn outcome(result: Result<Value, ConfigError>) -> Value {
    match result {
        Ok(value) => json!({"value":value}),
        Err(error) => json!({"error_type":error.kind(),"error_message":error.message()}),
    }
}

fn value<T: serde::Serialize>(result: T) -> Result<Value, ConfigError> {
    Ok(json!(result))
}

fn stamp(raw: &Value) -> Result<ObservationStamp, ConfigError> {
    ObservationStamp::new(
        raw["character_id"].as_str().unwrap_or(""),
        raw["room_id"].as_str().unwrap_or(""),
        raw["sequence"].as_u64().unwrap_or(0),
        raw["observed_at"].as_f64().unwrap_or(f64::NAN),
    )
}

fn location(raw: &Value) -> Location {
    match raw.as_str().unwrap_or("field") {
        "field" => Location::Field,
        "town" => Location::Town,
        _ => Location::Unknown,
    }
}

fn optional_ratio(raw: &Value, name: &str) -> Option<f64> {
    raw.get(name).map_or(Some(1.0), Value::as_f64)
}

fn survival(request: &Value) -> Result<Value, ConfigError> {
    let policy = if request["policy"].is_null() {
        None
    } else {
        Some(Arc::new(CharacterSurvivalPolicy::from_value(
            &request["policy"],
        )?))
    };
    let mut state = CharacterSurvivalState::new(
        request["character_id"].as_str().unwrap_or("flash"),
        request["room_id"].as_str().unwrap_or("room"),
        policy,
    )?;
    let mut results = Vec::new();
    for operation in request["operations"].as_array().into_iter().flatten() {
        let result = (|| match operation["op"].as_str().unwrap_or("") {
            "belt" => {
                let contents: Option<BeltContents> =
                    operation["contents"].as_object().and_then(|contents| {
                        contents
                            .iter()
                            .map(|(key, reading)| {
                                Some((
                                    key.parse().ok()?,
                                    BeltReading {
                                        kind: reading[0].as_str().map(Into::into),
                                        count: reading[1].as_i64()?,
                                    },
                                ))
                            })
                            .collect()
                    });
                value(state.observe_belt(&stamp(&operation["stamp"])?, contents.as_ref()))
            }
            "delta" => value(
                state.confirm_resource_change(
                    &stamp(&operation["stamp"])?,
                    operation["column"]
                        .as_u64()
                        .and_then(|number| u8::try_from(number).ok())
                        .unwrap_or(0),
                    operation["kind"].as_str().unwrap_or(""),
                    operation["delta"].as_i64().unwrap_or(0),
                ),
            ),
            "invalidate" => value(state.invalidate_belt(&stamp(&operation["stamp"])?)),
            "buff" => value(
                state.confirm_buff(
                    &stamp(&operation["stamp"])?,
                    operation["name"].as_str().unwrap_or(""),
                    location(&operation["location"]),
                    operation
                        .get("monsters_clear")
                        .map_or(Some(true), Value::as_bool),
                ),
            ),
            "pickup" => value(state.potion_pickup_kinds(
                operation["now"].as_f64().unwrap_or(f64::NAN),
                operation["belt_age"].as_f64().unwrap_or(f64::NAN),
            )?),
            "decide" => {
                let observation = SafetyObservation::new(
                    stamp(&operation["stamp"])?,
                    optional_ratio(operation, "life"),
                    optional_ratio(operation, "mana"),
                    location(&operation["location"]),
                    operation
                        .get("monsters_clear")
                        .map_or(Some(true), Value::as_bool),
                )?;
                value(
                    state.decide(
                        &observation,
                        operation["now"].as_f64().unwrap_or(f64::NAN),
                        operation
                            .get("screen_age")
                            .and_then(Value::as_f64)
                            .unwrap_or(1.0),
                        operation
                            .get("belt_age")
                            .and_then(Value::as_f64)
                            .unwrap_or(1000.0),
                    )?,
                )
            }
            _ => unreachable!("fixed oracle operation"),
        })();
        let mut result = outcome(result);
        result["counts"] = value(state.belt_counts())?;
        result["potions"] = value(state.potions_remaining())?;
        result["renewal"] = value(state.next_buff_renewal_at())?;
        results.push(result);
    }
    Ok(Value::Array(results))
}

fn evaluate(request: &Value) -> Result<Value, ConfigError> {
    if request["mode"] == "buff_sequence" {
        return buff_sequence(request);
    }
    if request["mode"] != "casting" {
        return survival(request);
    }
    let catalog = CastingCatalog::from_value(&request["catalog"])?;
    Ok(Value::Array(
        request["queries"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|query| {
                outcome((|| {
                    let profile = if query["profile"].is_null() {
                        None
                    } else {
                        Some(CharacterCastingProfile::from_value(&query["profile"])?)
                    };
                    value(catalog.estimate(
                        query["family"].as_str().unwrap_or(""),
                        query["class"].as_str(),
                        profile.as_ref(),
                    )?)
                })())
            })
            .collect(),
    ))
}

fn buff_sequence(request: &Value) -> Result<Value, ConfigError> {
    let policy = Arc::new(CharacterSurvivalPolicy::from_value(&request["policy"])?);
    let mut state = CharacterSurvivalState::new("flash", "room", Some(policy))?;
    let initial: BeltContents = request["initial_belt"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(key, reading)| {
            (
                key.parse::<u8>().unwrap_or(0),
                BeltReading {
                    kind: reading[0].as_str().map(Into::into),
                    count: reading[1].as_i64().unwrap_or(0),
                },
            )
        })
        .collect();
    state.observe_belt(&stamp(&request["initial_stamp"])?, Some(&initial));
    let order: Vec<_> = request["order"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|token| token.as_str().unwrap_or("").to_owned())
        .collect();
    let mut sequence = BuffSequence::new(&state, &order)?;
    let mut results = Vec::new();
    for operation in request["operations"].as_array().into_iter().flatten() {
        let result = (|| match operation["op"].as_str().unwrap_or("") {
            "cancel" => {
                sequence.cancel();
                Ok(Value::Null)
            }
            "ack" => value(
                sequence.acknowledge_step(
                    &state,
                    operation["index"]
                        .as_u64()
                        .and_then(|index| usize::try_from(index).ok())
                        .unwrap_or(usize::MAX),
                    &stamp(&operation["stamp"])?,
                    operation
                        .get("succeeded")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                ),
            ),
            mode => {
                let observation = SafetyObservation::new(
                    stamp(&operation["stamp"])?,
                    optional_ratio(operation, "life"),
                    optional_ratio(operation, "mana"),
                    location(&operation["location"]),
                    operation
                        .get("monsters_clear")
                        .map_or(Some(true), Value::as_bool),
                )?;
                let now = operation["now"].as_f64().unwrap_or(f64::NAN);
                let screen = operation
                    .get("screen_age")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0);
                let belt = operation
                    .get("belt_age")
                    .and_then(Value::as_f64)
                    .unwrap_or(1000.0);
                if mode == "next" {
                    value(sequence.next_step(&state, &observation, now, screen, belt)?)
                } else {
                    let evidence = BuffConfirmation {
                        stamp: stamp(
                            operation
                                .get("evidence_stamp")
                                .unwrap_or(&operation["stamp"]),
                        )?,
                        source: ConfirmationSource::ValidatedVision,
                        effect_verified: operation
                            .get("effect_verified")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        equipment_restored: operation
                            .get("equipment_restored")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        sequence_effects_verified: operation
                            .get("sequence_effects_verified")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    };
                    value(sequence.confirm_effect(
                        &mut state,
                        operation["name"].as_str().unwrap_or("battle_orders"),
                        &evidence,
                        &observation,
                        now,
                        ObservationLimits::new(screen, belt)?,
                    )?)
                }
            }
        })();
        let mut result = outcome(result);
        result["phase"] = value(sequence.phase())?;
        result["index"] = json!(sequence.index());
        result["renewal"] = value(state.next_buff_renewal_at())?;
        results.push(result);
    }
    Ok(Value::Array(results))
}

fn compare(requests: Vec<Value>) -> TestResult {
    let expected = support::legacy_expectation("policy-oracle", &json!(requests))?;
    for (index, request) in requests.iter().enumerate() {
        assert_values(
            &outcome(evaluate(request)),
            &expected[index],
            &format!("request/{index}"),
        );
    }
    Ok(())
}

fn assert_values(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(
                actual.len(),
                expected.len(),
                "{path}: keys {:?} != {:?}",
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>()
            );
            for (key, expected) in expected {
                assert_values(&actual[key], expected, &format!("{path}/{key}"));
            }
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{path}: array length");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_values(actual, expected, &format!("{path}/{index}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

fn source(path: &str) -> Result<Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
    )?)?)
}
fn policy() -> Result<Value, Box<dyn Error>> {
    Ok(source("config/characters/characters.json")?["characters"]["flash"]["survival"].clone())
}
fn s(sequence: u64, time: f64) -> Value {
    json!({"character_id":"flash","room_id":"room","sequence":sequence,"observed_at":time})
}
fn belt(count: u8, scrolls: u8) -> Value {
    let mut left = count;
    let mut values = serde_json::Map::new();
    for number in 1..=3 {
        let count = left.min(4);
        left -= count;
        values.insert(
            number.to_string(),
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
    values.insert(
        "4".into(),
        json!([
            if scrolls > 0 {
                Some("town_portal_scroll")
            } else {
                None
            },
            scrolls
        ]),
    );
    Value::Object(values)
}
fn initialized() -> Vec<Value> {
    vec![
        json!({"op":"belt","stamp":s(1,0.0),"contents":belt(12,4)}),
        json!({"op":"buff","stamp":s(2,0.0),"name":"battle_orders"}),
    ]
}

#[test]
fn casting_breakpoints_context_precedence_and_unbounded_integers() -> TestResult {
    let catalog = source("config/game-rules/casting.json")?["casting_rule_sets"].clone();
    let base = json!({"rule_set_id":"d2r-normal-teleport-reference-v1","skill_id":"teleport","form":"normal","fcr":105});
    let mut queries = Vec::new();
    for class in ["sorceress", "paladin"] {
        for fcr in (0..=202)
            .map(Value::from)
            .chain([serde_json::from_str(&format!("1{}", "0".repeat(300)))?])
        {
            let mut profile = base.clone();
            profile["fcr"] = fcr;
            queries.push(json!({"family":"resurrection","class":class,"profile":profile}));
        }
    }
    queries.push(json!({"family":"resurrection","class":"sorceress","profile":null}));
    queries.push(json!({"family":"resurrection","class":null,"profile":base}));
    for (name, replacement) in [
        ("fcr", Value::Null),
        ("rule_set_id", json!("unknown")),
        ("skill_id", json!("lightning")),
        ("form", json!("werebear")),
    ] {
        let mut profile = base.clone();
        profile[name] = replacement;
        queries.push(json!({"family":"resurrection","class":"sorceress","profile":profile}));
    }
    queries.push(json!({"family":"rotw","class":"sorceress","profile":base}));
    queries.push(json!({"family":"resurrection","class":"necromancer","profile":base}));
    compare(vec![
        json!({"mode":"casting","catalog":catalog,"queries":queries}),
    ])
}

#[test]
fn survival_priority_equality_freshness_and_request_is_not_confirmation() -> TestResult {
    let mut requests = Vec::new();
    for count in [0, 1, 6, 7, 12] {
        for (life, mana, location, clear, now) in [
            (Some(0.5), Some(0.1), "field", Some(true), 1.0),
            (Some(0.4999), Some(1.0), "field", Some(true), 1.0),
            (Some(1.0), Some(0.0999), "field", Some(true), 140.0),
            (None, Some(1.0), "field", Some(true), 140.0),
            (Some(1.0), Some(1.0), "town", Some(true), 140.0),
            (Some(1.0), Some(1.0), "field", None, 140.0),
            (Some(1.0), Some(1.0), "unknown", Some(true), 1.0),
            (Some(1.0), Some(1.0), "field", Some(true), 139.9999),
            (Some(1.0), Some(1.0), "field", Some(true), 140.0),
        ] {
            let mut operations = initialized();
            operations.push(json!({"op":"belt","stamp":s(3,0.0),"contents":belt(count,2)}));
            let decision = json!({"op":"decide","stamp":s(1000,now),"now":now,"life":life,"mana":mana,"location":location,"monsters_clear":clear});
            operations.extend([
                decision.clone(),
                decision,
                json!({"op":"pickup","now":now,"belt_age":1000}),
            ]);
            requests.push(json!({"policy":policy()?,"operations":operations}));
        }
    }
    let mut operations = initialized();
    for (seq, timestamp, now, screen, belt_age) in [
        (3, 1.0, 2.0, 1.0, 2.0),
        (4, 1.0, 2.00001, 1.0, 3.0),
        (5, 3.0, 2.0, 1.0, 3.0),
        (1, 0.0, 0.0, 1.0, 3.0),
        (6, 2.0, 2.0, 1.0, 1.9999),
    ] {
        operations.push(json!({"op":"decide","stamp":s(seq,timestamp),"now":now,"screen_age":screen,"belt_age":belt_age}));
    }
    let mut wrong = s(9, 1.0);
    wrong["room_id"] = json!("other");
    operations.push(json!({"op":"decide","stamp":wrong,"now":1.0}));
    requests.push(json!({"policy":policy()?,"operations":operations}));
    requests.push(json!({"policy":null,"operations":[{"op":"decide","stamp":s(1,0.0),"now":0}]}));
    compare(requests)
}

#[test]
fn belt_invalid_newer_consumes_stamp_and_reconciles_only_complete_readings() -> TestResult {
    let mut operations = initialized();
    operations.extend([
        json!({"op":"delta","stamp":s(3,1.0),"column":1,"kind":"purple_potion","delta":-1}),
        json!({"op":"delta","stamp":s(3,1.0),"column":1,"kind":"purple_potion","delta":-1}),
        json!({"op":"belt","stamp":s(2,2.0),"contents":belt(12,4)}),
        json!({"op":"belt","stamp":s(4,0.5),"contents":belt(12,4)}),
        json!({"op":"delta","stamp":s(4,2.0),"column":1,"kind":"wrong","delta":-1}),
        json!({"op":"belt","stamp":s(4,2.0),"contents":belt(12,4)}),
        json!({"op":"belt","stamp":s(5,3.0),"contents":belt(9,2)}),
        json!({"op":"belt","stamp":s(6,4.0),"contents":{"1":["purple_potion",4]}}),
        json!({"op":"decide","stamp":s(6,4.0),"now":4}),
        json!({"op":"belt","stamp":s(7,5.0),"contents":belt(8,1)}),
        json!({"op":"invalidate","stamp":s(8,6.0)}),
        json!({"op":"delta","stamp":s(9,7.0),"column":1,"kind":"purple_potion","delta":-1}),
        json!({"op":"belt","stamp":s(9,7.0),"contents":belt(12,4)}),
        json!({"op":"belt","stamp":s(10,8.0),"contents":belt(12,4)}),
        json!({"op":"delta","stamp":s(11,9.0),"column":1,"kind":"purple_potion","delta":i64::MAX}),
    ]);
    let mut learned = policy()?;
    learned["belt_columns"][3]["target_count"] = Value::Null;
    compare(vec![
        json!({"policy":policy()?,"operations":operations}),
        json!({"policy":learned,"operations":[{"op":"belt","stamp":s(1,0.0),"contents":belt(12,2)},{"op":"belt","stamp":s(2,1.0),"contents":belt(12,1)},{"op":"decide","stamp":s(3,1.0),"now":1}]}),
    ])
}

fn buff_request(policy: Value, operations: Vec<Value>) -> Value {
    json!({"mode":"buff_sequence","policy":policy,"initial_stamp":s(0,0.0),"initial_belt":belt(12,4),"order":["w","a","a","s","d","f1","w"],"operations":operations})
}

fn dispatched() -> Vec<Value> {
    (0_u32..7)
        .flat_map(|index| {
            let time = f64::from(index) + 1.0;
            [
                json!({"op":"next","stamp":s(u64::from(index)*2+1,time),"now":time}),
                json!({"op":"ack","index":index,"stamp":s(u64::from(index)*2+2,time+0.1)}),
            ]
        })
        .collect()
}

#[test]
fn buff_repeated_tokens_acknowledgement_and_effect_proof_keep_request_time() -> TestResult {
    let mut requests = Vec::new();
    let mut operations = dispatched();
    operations.extend([
        json!({"op":"next","stamp":s(15,8.0),"now":8.0}),
        json!({"op":"effect","stamp":s(16,8.0),"now":8.0,"equipment_restored":false}),
        json!({"op":"effect","stamp":s(17,8.0),"now":8.0,"evidence_stamp":s(18,8.0)}),
        json!({"op":"effect","stamp":s(19,8.0),"now":8.0}),
        json!({"op":"effect","stamp":s(20,9.0),"now":9.0}),
        json!({"op":"next","stamp":s(21,9.0),"now":9.0}),
        json!({"op":"next","stamp":s(22,144.0),"now":144.0}),
    ]);
    requests.push(buff_request(policy()?, operations));
    for (location, clear, life) in [
        ("town", Some(true), 1.0),
        ("field", None, 1.0),
        ("field", Some(true), 0.1),
    ] {
        let mut operations = dispatched();
        operations.push(json!({"op":"effect","stamp":s(15,8.0),"now":8.0,"location":location,"monsters_clear":clear,"life":life}));
        requests.push(buff_request(policy()?, operations));
    }
    let mut operations = dispatched();
    operations.push(json!({"op":"effect","stamp":s(15,144.0),"now":144.0}));
    requests.push(buff_request(policy()?, operations));
    compare(requests)
}

#[test]
fn buff_pending_wrong_ack_and_preemption_never_resend_or_confirm() -> TestResult {
    let operations = vec![
        json!({"op":"next","stamp":s(1,1.0),"now":1.0}),
        json!({"op":"next","stamp":s(2,1.0),"now":1.0}),
        json!({"op":"ack","index":1,"stamp":s(2,1.0)}),
        json!({"op":"ack","index":0,"stamp":s(1,1.0)}),
        json!({"op":"ack","index":0,"stamp":s(2,1.1)}),
        json!({"op":"next","stamp":s(3,2.0),"now":2.0,"life":0.1}),
        json!({"op":"ack","index":1,"stamp":s(4,2.1)}),
        json!({"op":"next","stamp":s(5,3.0),"now":3.0}),
    ];
    let failure = vec![
        json!({"op":"next","stamp":s(1,1.0),"now":1.0}),
        json!({"op":"ack","index":0,"stamp":s(2,1.1),"succeeded":false}),
        json!({"op":"next","stamp":s(3,2.0),"now":2.0}),
    ];
    compare(vec![
        buff_request(policy()?, operations),
        buff_request(policy()?, failure),
    ])
}

#[test]
fn huge_reference_fcr_and_frames_remain_integers_and_active_query_has_no_selection_fallback()
-> TestResult {
    let integer = |zeros: usize| -> Result<Value, serde_json::Error> {
        serde_json::from_str(&format!("1{}", "0".repeat(zeros)))
    };
    let catalog = json!([{"rule_set_id":"huge","ruleset_family":"test","revision":"test","source_url":"local-test","evidence_status":"reference","rules":[{"character_class":"test","skill_id":"test","form":"normal","breakpoints":[{"minimum_fcr":0,"frames":integer(400)?},{"minimum_fcr":integer(300)?,"frames":integer(399)?}]}]}]);
    let mut requests = Vec::new();
    for fcr in [
        serde_json::from_str::<Value>(&"9".repeat(300))?,
        integer(300)?,
    ] {
        requests.push(json!({"mode":"casting","catalog":catalog,"queries":[{"family":"test","class":"test","profile":{"rule_set_id":"huge","skill_id":"test","form":"normal","fcr":fcr}}]}));
    }
    compare(requests)?;
    let mut config = d2bot::common::config::BotConfig::load(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("config"),
    )?;
    config.apply_character_selection(Some("flash"))?;
    assert_eq!(config.active_casting_estimate()?.frames, Some(8.into()));
    config.active_character = Some("missing".into());
    assert!(config.active_character_profile().is_some());
    assert_eq!(
        config.active_casting_estimate()?.reason,
        "character_missing"
    );
    Ok(())
}
