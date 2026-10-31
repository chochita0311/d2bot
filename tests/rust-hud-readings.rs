mod support;
use d2bot::common::buffing::BeltVisibility;
use d2bot::common::survival::{CharacterSurvivalPolicy, ObservationStamp};
use d2bot::common::survival_vision::{HudReading, ResourceReading, SlotReading};
use serde_json::{Value, json};
use std::error::Error;
use std::path::Path;

type TestResult = Result<(), Box<dyn Error>>;
fn value(reading: ResourceReading) -> Value {
    json!({"current":reading.current(),"maximum":reading.maximum(),"reason":reading.reason,"score":reading.score,"lower_bound":reading.lower_bound(),"ratio":reading.ratio()})
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
fn resource_text_and_explicit_lower_bound_match_python_without_guessing_numbers() -> TestResult {
    let mut requests: Vec<_> = [
        "0/100",
        "1/0",
        "100/99",
        "01/100",
        "1/01",
        "-1/2",
        "1.0/2",
        "1//2",
        "abc",
        "/",
        "1/2\n",
        " 1/2",
        "١/2",
        "9999999/9999999",
        "10000000/10000000",
        "1457/1543",
    ]
    .into_iter()
    .map(|text| json!({"op":"text","text":text}))
    .collect();
    requests.extend([
        json!({"op":"bound","bound":0.8}),
        json!({"op":"bound","bound":0.0}),
    ]);
    let expected = support::legacy_expectation("hud-reading-oracle", &json!(requests))?;
    for (index, request) in requests.iter().enumerate() {
        let reading = if request["op"] == "text" {
            ResourceReading::from_text(request["text"].as_str().ok_or("text missing")?, 1.0)
        } else {
            ResourceReading::with_lower_bound(
                request["bound"].as_f64().ok_or("bound missing")?,
                "orb_bound",
                None,
            )?
        };
        assert_values(
            &value(reading),
            &expected[index],
            &format!("resource/{index}"),
        );
    }
    assert!(ResourceReading::with_lower_bound(f64::NAN, "invalid", None).is_err());
    assert_eq!(
        ResourceReading::from_counts(1, 0, "unverified", None).ratio(),
        None
    );
    Ok(())
}

#[test]
fn belt_requires_all_slot_positions_kinds_and_capacity_and_safety_stays_unknown() -> TestResult {
    let raw: Value = serde_json::from_slice(&std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("config/characters/characters.json"),
    )?)?;
    let policy = raw["characters"]["flash"]["survival"].clone();
    let slots:Vec<Value> = (1..=4).flat_map(|row| (1..=4).map(move |column| json!({"row":row,"column":column,"kind":if column == 4 {"town_portal_scroll"} else {"purple_potion"},"distance":0.0}))).collect();
    let mut requests = vec![json!({"policy":policy,"status":"read","slots":slots})];
    let mut empty = slots.clone();
    for slot in &mut empty {
        if slot["column"] == 1 {
            slot["kind"] = json!("empty");
        }
    }
    requests.push(json!({"policy":policy,"status":"read","slots":empty}));
    for kind in [Value::Null, json!("healing")] {
        let mut wrong = slots.clone();
        wrong[12]["kind"] = kind;
        requests.push(json!({"policy":policy,"status":"read","slots":wrong}));
    }
    let mut duplicate = slots.clone();
    duplicate[0] = duplicate[1].clone();
    requests.push(json!({"policy":policy,"status":"read","slots":duplicate}));
    requests.push(json!({"policy":policy,"status":"read","slots":&slots[..15]}));
    requests.push(json!({"policy":policy,"status":"unverified","slots":slots}));
    let mut small = policy.clone();
    small["belt_columns"][0]["capacity"] = json!(2);
    small["belt_columns"][0]["target_count"] = json!(2);
    requests.push(json!({"policy":small,"status":"read","slots":slots}));
    let mut fit = slots.clone();
    fit[0]["kind"] = json!("empty");
    fit[4]["kind"] = json!("empty");
    fit.reverse();
    requests.push(json!({"policy":small,"status":"read","slots":fit}));
    for request in &mut requests {
        request["op"] = json!("hud");
    }
    let expected = support::legacy_expectation("hud-reading-oracle", &json!(requests))?;
    for (index, request) in requests.iter().enumerate() {
        let policy = CharacterSurvivalPolicy::from_value(&request["policy"])?;
        let slots = request["slots"]
            .as_array()
            .ok_or("slots missing")?
            .iter()
            .map(|slot| -> Result<SlotReading, Box<dyn Error>> {
                Ok(SlotReading {
                    row: u8::try_from(slot["row"].as_u64().ok_or("row missing")?)?,
                    column: u8::try_from(slot["column"].as_u64().ok_or("column missing")?)?,
                    kind: slot["kind"].as_str().map(Into::into),
                    distance: 0.0,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let reading = HudReading {
            layout_id: "layout".into(),
            revision: "revision".into(),
            life: ResourceReading::from_text("1580/1600", 1.0),
            mana: ResourceReading::default(),
            belt_status: request["status"].as_str().ok_or("status missing")?.into(),
            slots,
            belt_visibility: BeltVisibility::Expanded,
            mercenary_status: None,
        };
        let contents = reading.belt_contents(&policy).map(|contents| {
            contents
                .into_iter()
                .map(|(column, reading)| (column, (reading.kind, reading.count)))
                .collect::<std::collections::BTreeMap<_, _>>()
        });
        let safety =
            reading.safety_observation(ObservationStamp::new("another", "room", 1, 10.0)?)?;
        assert_values(
            &json!({"contents":contents,"safety":safety}),
            &expected[index],
            &format!("hud/{index}"),
        );
    }
    Ok(())
}
