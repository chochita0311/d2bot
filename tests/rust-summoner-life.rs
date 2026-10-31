use d2bot::{
    common::encounters::Appearance,
    runs::summoner::{
        appearance_vision::SummonerAppearance, life_vision::SummonerLifeTracker,
        objective_vision::ObjectiveReading,
    },
};
use serde_json::{Value, json};
use std::error::Error;
mod support;
fn op(sequence: u64, at: f64, alive: bool, corpse: bool) -> Value {
    json!({"sequence":sequence,"at":at,"now":at+0.01,
        "platform":{"ground_point":[740,560],"evidence_kind":"runes"},
        "alive":alive.then(||json!({"ground":[800,600],"asset_id":"alive"})),
        "corpse":corpse.then(||json!({"ground":[805,601],"asset_id":"corpse"}))})
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{a:?} != {b:?}");
            for (k, a) in a {
                compare(a, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
fn appearance(raw: &Value, kind: Appearance) -> Result<Option<SummonerAppearance>, Box<dyn Error>> {
    if raw.is_null() {
        return Ok(None);
    }
    Ok(Some(SummonerAppearance {
        r#box: [0, 0, 1, 1],
        ground: serde_json::from_value(raw["ground"].clone())?,
        score: 1.0,
        appearance: kind,
        asset_id: raw["asset_id"].as_str().ok_or("asset")?.into(),
    }))
}
#[test]
fn ordered_current_platform_corpse_pairs_preserve_optional_alive_links()
-> Result<(), Box<dyn Error>> {
    let mut cases = vec![
        vec![
            op(1, 1.0, true, false),
            op(2, 1.1, false, true),
            op(3, 1.2, false, true),
        ],
        vec![op(1, 1.0, false, true), op(2, 1.1, false, true)],
    ];
    for condition in [
        "absence",
        "stale",
        "future",
        "old_pair",
        "moved",
        "wrong_anchor",
        "far",
        "both",
        "same_sequence",
        "same_time",
        "no_platform",
        "no_ground",
        "edge_age",
        "edge_shift",
        "over_shift",
        "edge_link_age",
        "camera_shift",
    ] {
        let first = op(1, 1.0, false, true);
        let mut next = op(2, 1.1, false, true);
        match condition {
            "absence" => next["corpse"] = Value::Null,
            "stale" => next["now"] = json!(1.4),
            "future" => next["now"] = json!(1.0),
            "old_pair" => {
                next["at"] = json!(13.1);
                next["now"] = json!(13.11);
            }
            "moved" => (),
            "wrong_anchor" => next["platform"]["evidence_kind"] = json!("different"),
            "far" => next["corpse"]["ground"] = json!([1500, 800]),
            "both" => next["alive"] = json!({"ground":[800,600],"asset_id":"alive"}),
            "same_sequence" => next["sequence"] = json!(1),
            "same_time" => {
                next["at"] = json!(1.0);
                next["now"] = json!(1.01);
            }
            "no_platform" => next["platform"] = Value::Null,
            "no_ground" => next["platform"]["ground_point"] = Value::Null,
            "edge_age" => {
                next["at"] = json!(1.25);
                next["now"] = json!(1.5);
            }
            "edge_shift" => next["corpse"]["ground"] = json!([829, 601]),
            "over_shift" => next["corpse"]["ground"] = json!([829.001, 601]),
            "edge_link_age" => {
                next["at"] = json!(13.0);
                next["now"] = json!(13.0);
            }
            "camera_shift" => {
                next["corpse"]["ground"] = json!([905, 701]);
                next["platform"]["ground_point"] = json!([840, 660]);
            }
            _ => return Err("unknown case".into()),
        }
        let mut operations = vec![first];
        if condition == "moved" {
            operations.push(json!({"moved":true}));
        }
        operations.push(next);
        operations.push(op(3, 1.5, false, true));
        cases.push(operations);
    }
    for condition in [
        "old_alive",
        "far_alive",
        "moved",
        "edge_target",
        "over_target",
    ] {
        let mut alive = op(0, 0.5, true, false);
        let mut first = op(1, 1.0, false, true);
        let mut second = op(2, 1.1, false, true);
        if condition == "far_alive" {
            alive["alive"]["ground"] = json!([1500, 800]);
        }
        if condition == "old_alive" {
            first["at"] = json!(14.0);
            first["now"] = json!(14.01);
            second["at"] = json!(14.1);
            second["now"] = json!(14.11);
        }
        if condition == "edge_target" || condition == "over_target" {
            let x = if condition == "edge_target" {
                925.0
            } else {
                925.001
            };
            alive["alive"]["ground"] = json!([x, 601]);
        }
        let mut ops = vec![alive];
        if condition == "moved" {
            ops.push(json!({"moved":true}));
        }
        ops.extend([first, second]);
        cases.push(ops);
    }
    cases.push(vec![
        op(5, 5.0, true, false),
        json!({"moved":true}),
        op(5, 5.1, false, true),
        op(6, 5.2, false, true),
        op(7, 5.3, false, true),
    ]);
    let expected = support::legacy_expectation("life-tracker-oracle", &json!(cases))?;
    for (index, operations) in cases.iter().enumerate() {
        let mut tracker = SummonerLifeTracker::new("flash", "room")?;
        let mut actual = Vec::new();
        for operation in operations {
            if operation["moved"] == true {
                tracker.moved()?;
                actual.push(Value::Null);
                continue;
            }
            let alive = appearance(&operation["alive"], Appearance::Alive)?;
            let corpse = appearance(&operation["corpse"], Appearance::Corpse)?;
            let p = &operation["platform"];
            let platform = if p.is_null() {
                None
            } else {
                Some(ObjectiveReading {
                    kind: "summoner_platform".into(),
                    journal_box: None,
                    journal_score: None,
                    rune_boxes: Vec::new(),
                    evidence_kind: p["evidence_kind"].as_str().ok_or("kind")?.into(),
                    glyph_scores: Vec::new(),
                    ground_point: serde_json::from_value(p["ground_point"].clone())?,
                    slot_indices: Vec::new(),
                    layout_residual: None,
                })
            };
            actual.push(serde_json::to_value(tracker.observe(
                alive.as_ref(),
                corpse.as_ref(),
                platform.as_ref(),
                operation["sequence"].as_u64().ok_or("sequence")?,
                operation["at"].as_f64().ok_or("at")?,
                operation["now"].as_f64().ok_or("now")?,
            )?)?);
        }
        eprintln!("life case {index}");
        compare(&json!(actual), &expected[index]);
    }
    Ok(())
}
