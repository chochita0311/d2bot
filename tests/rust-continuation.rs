use d2bot::runs::summoner::{
    continuation::{
        ArrivalDwell, HubLandingTracker, NEAR_GROUND_RADIUS, near_ground, validate_hub_departure,
        validate_live_continuation, validate_terminal_return,
    },
    scene_vision::HubEvidence,
};
use serde_json::{Value, json};
use std::error::Error;
mod support;
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap_or(f64::NAN) - b.as_f64().unwrap_or(f64::NAN)).abs() < 1e-9,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn exercise(cases: Vec<Value>) -> Result<(), Box<dyn Error>> {
    let expected = support::legacy_expectation("continuation-oracle", &json!(cases))?;
    for (i, case) in cases.iter().enumerate() {
        let actual = match case["kind"].as_str().ok_or("kind")? {
            "live" | "terminal" | "hub" => {
                let wing = serde_json::from_value(case["wing"].clone())?;
                let from = serde_json::from_value(case["return_from"].clone())?;
                let now = case["now"].as_f64().ok_or("now")?;
                let outcome = match case["kind"].as_str() {
                    Some("live") => validate_live_continuation(&case["progress"], wing, from, now),
                    Some("terminal") => {
                        validate_terminal_return(&case["progress"], wing, from, now)
                    }
                    _ => validate_hub_departure(&case["progress"], wing, from, now),
                };
                json!(outcome.err().map(|error| error.to_string()))
            }
            "dwell" => {
                let mut dwell = ArrivalDwell::new(case["reference"].as_f64().unwrap_or(0.32))?;
                let mut rows = vec![];
                for op in case["operations"].as_array().ok_or("operations")? {
                    let outcome = if op["reset"] == true {
                        dwell.reset();
                        None
                    } else {
                        Some(dwell.update(
                            op["near"] != false,
                            serde_json::from_value(op["displacement"].clone())?,
                            op["at"].as_f64().ok_or("at")?,
                        )?)
                    };
                    rows.push(json!({"outcome":outcome,"state":dwell}));
                }
                json!(rows)
            }
            "landing" => {
                let mut tracker = HubLandingTracker::default();
                let mut rows = vec![];
                for op in case["operations"].as_array().ok_or("operations")? {
                    let hub = serde_json::from_value(op["hub"].clone())?;
                    let player = serde_json::from_value(op["player"].clone())?;
                    let at = op["at"].as_f64().ok_or("at")?;
                    let outcome = if op["begin"] == true {
                        tracker.begin(hub, player, at)?;
                        None
                    } else {
                        let evidence = (op["evidence"] == "blue_pair_and_floor_ornaments")
                            .then_some(HubEvidence::Current {
                                point: hub,
                                parts: vec![],
                                blue_evidence: "test",
                            });
                        Some(tracker.update(
                            hub,
                            player,
                            evidence.as_ref(),
                            serde_json::from_value(op["displacement"].clone())?,
                            at,
                            op["sequence"].as_u64().ok_or("sequence")?,
                        ))
                    };
                    rows.push(json!({"outcome":outcome,"state":tracker}));
                }
                json!(rows)
            }
            _ => {
                let point = serde_json::from_value(case["point"].clone())?;
                let player = if case["player"].is_null() {
                    [0.0; 2]
                } else {
                    serde_json::from_value(case["player"].clone())?
                };
                let radius = if case["radius"].is_null() {
                    NEAR_GROUND_RADIUS
                } else {
                    serde_json::from_value(case["radius"].clone())?
                };
                let outcome = near_ground(point, player, radius);
                json!({"outcome":outcome.as_ref().ok(),"error":outcome.is_err()})
            }
        };
        compare(&actual, &expected[i], &format!("case{i}"));
    }
    Ok(())
}
#[test]
fn immediate_route_handoffs_preserve_types_exact_statuses_and_proof_ages()
-> Result<(), Box<dyn Error>> {
    let live = json!({"continuous_status":"interval_complete","wing":"north","return_from":"south","continuous_frames":[{"processed_at":100}]});
    let terminal = json!({"status":"wing_end_observed","continuous_status":"current_chest_trio_and_player_settled","wing":"north","return_from":null,"terminal":{"kind":"nonboss_chest_trio"},"continuous_frames":[{"processed_at":100,"action":"terminal_arrival_hold"}]});
    let hub = json!({"status":"hub_arrived","continuous_status":"central_waypoint_and_player_settled","wing":"north","return_from":"north","continuous_frames":[{"processed_at":100,"action":"central_arrival_hold"}]});
    let mut cases = vec![];
    for (kind, progress, wing, from) in [
        ("live", live.clone(), "north", Some("south")),
        ("terminal", terminal.clone(), "north", Some("north")),
        ("hub", hub.clone(), "east", None),
    ] {
        for now in [99.9, 100.0, 100.2, 100.4999999, 100.5] {
            cases.push(
                json!({"kind":kind,"progress":progress,"wing":wing,"return_from":from,"now":now}),
            );
        }
        for field in [
            "status",
            "continuous_status",
            "wing",
            "return_from",
            "continuous_frames",
        ] {
            let mut changed = progress.clone();
            changed[field] = Value::Null;
            cases.push(
                json!({"kind":kind,"progress":changed,"wing":wing,"return_from":from,"now":100.2}),
            );
        }
        for at in [json!(true), json!("100"), Value::Null] {
            let mut changed = progress.clone();
            changed["continuous_frames"][0]["processed_at"] = at;
            cases.push(
                json!({"kind":kind,"progress":changed,"wing":wing,"return_from":from,"now":100.2}),
            );
        }
        cases.push(
            json!({"kind":kind,"progress":progress,"wing":"west","return_from":from,"now":100.2}),
        );
    }
    for finished in [
        json!(99.9),
        json!(100),
        json!(101),
        json!(true),
        json!("101"),
        Value::Null,
    ] {
        let mut p = live.clone();
        p["interval_finished_at"] = finished;
        cases.push(
            json!({"kind":"live","progress":p,"wing":"north","return_from":"south","now":101.2}),
        );
    }
    for (kind, status) in [
        (
            "nonboss_chest_trio",
            "current_chest_trio_and_player_settled",
        ),
        (
            "nonboss_chest_platform",
            "current_terminal_landmarks_and_player_settled",
        ),
        (
            "nonboss_chest_platform",
            "current_chest_trio_and_player_settled",
        ),
    ] {
        let mut p = terminal.clone();
        p["terminal"]["kind"] = json!(kind);
        p["continuous_status"] = json!(status);
        cases.push(json!({"kind":"terminal","progress":p,"wing":"north","return_from":"north","now":100.2}));
    }
    for provenance in [
        "current_static_two_parts",
        "current_relocalized_two_parts",
        "current_motion_settled_two_parts",
        "current_motion_one_part",
    ] {
        for parts in [
            json!([true, true, false]),
            json!([true, false, false]),
            json!([true, true, 1]),
            json!([true, true]),
        ] {
            for age in [-0.01, 0.0, 2.0, 2.0001] {
                let mut p = terminal.clone();
                p["continuous_status"] = json!("recent_terminal_layout_and_player_settled");
                p["terminal"] = json!({"kind":"nonboss_chest_platform","provenance":provenance,"current_parts":parts,"reference_observed_at":100.0-age});
                p["continuous_frames"][0]["acquired_at"] = json!(100);
                cases.push(json!({"kind":"terminal","progress":p,"wing":"north","return_from":"north","now":100.2}));
            }
        }
    }
    for (previous, next) in [("east", "south"), ("south", "west"), ("west", "north")] {
        let mut p = hub.clone();
        p["wing"] = json!(previous);
        p["return_from"] = json!(previous);
        cases.push(json!({"kind":"hub","progress":p,"wing":next,"return_from":null,"now":100.2}));
    }
    exercise(cases)
}
#[test]
fn dwell_hub_landing_and_nearby_ellipse_need_current_independent_evidence()
-> Result<(), Box<dyn Error>> {
    let mut cases = vec![
        json!({"kind":"dwell","operations":[{"at":100,"displacement":[0,0]},{"at":100.2,"displacement":[0,0]},{"at":100.32,"displacement":[0,0]},{"at":100.4,"displacement":null},{"at":100.5,"displacement":[0,0]},{"at":100.72,"displacement":[0,0]},{"at":100.8,"displacement":[2,0]},{"at":100.9,"displacement":[1.999,0]},{"at":101.2,"displacement":[0,0]},{"at":101.3,"near":false},{"at":101.4,"displacement":[0,0]},{"at":101.8,"displacement":[0,0]},{"reset":true}]} ),
    ];
    let begin = json!({"begin":true,"hub":[1100,500],"player":[960,580],"at":100});
    let observed = |at: f64, sequence: u64| json!({"hub":[1000,500],"player":[960,580],"at":at,"sequence":sequence,"evidence":"blue_pair_and_floor_ornaments","displacement":[0,0]});
    for mutation in [
        json!({}),
        json!({"evidence":"recent_floor_and_current_blue_pair"}),
        json!({"displacement":null}),
        json!({"displacement":[2,0]}),
        json!({"sequence":5}),
        json!({"hub":[1037,500]}),
        json!({"hub":[1036,500]}),
        json!({"at":101.5}),
    ] {
        let mut changed = observed(100.2, 3);
        for (k, v) in mutation.as_object().ok_or("mutation")? {
            changed[k] = v.clone();
        }
        cases.push(json!({"kind":"landing","operations":[begin,observed(100.1,2),changed,observed(100.3,4),observed(100.4,5),observed(101.5,6)]}));
    }
    cases.push(json!({"kind":"landing","operations":[observed(100.1,1),begin,observed(100.1,2),observed(100.35,3),observed(100.45,4)]}));
    for point in [
        Value::Null,
        json!([300, 0]),
        json!([300.00001, 0]),
        json!([180, 144]),
        json!([0, 180]),
        json!([0, 181]),
    ] {
        cases.push(json!({"kind":"near","point":point}));
    }
    cases.push(json!({"kind":"near","point":[300,0],"radius":[-300,-180]}));
    cases.push(json!({"kind":"near","point":[1,1],"radius":[0,180]}));
    cases.push(json!({"kind":"near","point":null,"radius":[0,180]}));
    exercise(cases)
}
