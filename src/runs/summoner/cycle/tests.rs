use super::*;
use crate::test_field_world::compare;
use serde_json::{Value, json};
use std::error::Error;
type TestResult = Result<(), Box<dyn Error>>;
fn snapshot(cycle: &SummonerCycle) -> Value {
    json!({"room_id":cycle.room_id(),"room_index":cycle.room_index,"room_started_at":cycle.room_started_at,"last_now":cycle.last_now,"pending":cycle.pending,"results":cycle.results,"checked_wings":cycle.checked_wings,"boss_wing":cycle.boss_wing,"keys_dropped":cycle.keys_dropped,"keys_acquired":cycle.keys_acquired,"done":cycle.done,"command_id":cycle.command_id,"last_sequence":cycle.last_sequence.map(|v| json!(v)).unwrap_or(json!(-1)),"last_observed_at":cycle.last_observed_at,"generation":cycle.generation,"wing_index":cycle.wing_index,"loot_complete":cycle.loot_complete,"room_entered":cycle.room_entered,"terminal_reason":cycle.terminal_reason})
}
fn error(error: ConfigError) -> Value {
    json!({"error":{"kind":error.kind(),"message":error.message()}})
}
fn exercise(case: &Value) -> Result<Value, Box<dyn Error>> {
    let mut policy = json!({"screen_age":1,"room_seconds":100,"total_seconds":500,"stage_seconds":CommandKind::ALL.into_iter().map(|kind|(kind.name(),5)).collect::<BTreeMap<_,_>>()});
    if let Some(values) = case["limits"].as_object() {
        policy
            .as_object_mut()
            .ok_or("limits")?
            .extend(values.clone());
    }
    let limits = CycleLimits::new(
        policy["screen_age"].as_f64().ok_or("age")?,
        policy["room_seconds"].as_f64().ok_or("room")?,
        policy["total_seconds"].as_f64().ok_or("total")?,
        serde_json::from_value(policy["stage_seconds"].clone())?,
    );
    let limits = match limits {
        Ok(v) => v,
        Err(e) => return Ok(error(e)),
    };
    let rooms: Vec<String> = if case["room_ids"].is_array() {
        serde_json::from_value(case["room_ids"].clone())?
    } else {
        (0..case["rooms"].as_u64().unwrap_or(1))
            .map(|i| format!("room-{i}"))
            .collect()
    };
    let mut cycle = match SummonerCycle::new(
        case["character"].as_str().unwrap_or("flash").into(),
        rooms,
        limits,
        10.0,
    ) {
        Ok(v) => v,
        Err(e) => return Ok(error(e)),
    };
    let mut now = 10.0;
    let mut sequence = None;
    let mut rows = vec![];
    for op in case["ops"].as_array().ok_or("ops")? {
        now = op["now"]
            .as_f64()
            .unwrap_or(now + op["dt"].as_f64().unwrap_or(0.1));
        let result = (|| -> Result<CycleDirective, ConfigError> {
            let observation = if op["none"] == true {
                None
            } else {
                let seq = op["sequence"]
                    .as_u64()
                    .unwrap_or(sequence.map_or(0, |v| v + 1));
                sequence = Some(seq);
                Some(CycleObservation::new(
                    ObservationStamp::new(
                        op["character"].as_str().unwrap_or("flash"),
                        op["room"].as_str().unwrap_or(cycle.room_id()),
                        seq,
                        op["captured"].as_f64().unwrap_or(now),
                    )?,
                    CycleEvidence {
                        generation: op["generation"].as_u64().unwrap_or(0),
                        event: serde_json::from_value(
                            op.get("event").cloned().unwrap_or(json!("unknown")),
                        )
                        .map_err(|e| ConfigError::value(e.to_string()))?,
                        command_id: if op.get("command_id").is_some() {
                            op["command_id"].as_u64()
                        } else {
                            cycle.pending().map(|v| v.command_id)
                        },
                        keys_dropped: op["keys_dropped"].as_u64().unwrap_or(0),
                        keys_acquired: op["keys_acquired"].as_u64().unwrap_or(0),
                        reason: op["reason"].as_str().unwrap_or("").into(),
                    },
                )?)
            };
            cycle.step(
                observation.as_ref(),
                now,
                op["focused"].as_bool().unwrap_or(true),
                op["stopped"].as_bool().unwrap_or(false),
            )
        })();
        let mut row = match result {
            Ok(v) => json!({"directive":v}),
            Err(e) => error(e),
        };
        row["state"] = snapshot(&cycle);
        rows.push(row);
    }
    Ok(json!(rows))
}
fn prepare() -> Vec<Value> {
    [
        "character_select",
        "act1_ready",
        "arcane_ready",
        "field_ready",
    ]
    .map(|event| json!({"event":event}))
    .to_vec()
}
fn finish(ops: &mut Vec<Value>, wing: usize, keys: u64) {
    for _ in 0..wing {
        ops.extend([
            json!({"event":"wing_empty_confirmed"}),
            json!({"event":"hub_ready"}),
        ]);
    }
    ops.extend([
        json!({"event":"summoner_alive"}),
        json!({"event":"summoner_dead_confirmed"}),
        json!({"event":"loot_complete","keys_dropped":keys,"keys_acquired":keys}),
        json!({"event":"hub_ready"}),
        json!({"event":"canyon_ready"}),
        json!({"event":"act1_ready"}),
        json!({"event":"restock_complete"}),
        json!({"event":"character_select"}),
    ]);
}
#[test]
fn cycle_transitions_and_room_ownership_match_python() -> TestResult {
    let mut cases = vec![];
    for wing in 0..4 {
        let mut ops = prepare();
        finish(&mut ops, wing, wing as u64);
        ops.push(json!({"event":"character_select"}));
        cases.push(json!({"ops":ops}));
    }
    let mut ops = prepare();
    for room in 0..30 {
        if room > 0 {
            ops.extend(prepare().into_iter().skip(1));
        }
        finish(&mut ops, room % 4, (room % 3) as u64);
    }
    ops.push(json!({"event":"character_select"}));
    cases.push(json!({"rooms":30,"ops":ops}));
    let mut ops = prepare();
    for _ in 0..4 {
        ops.extend([
            json!({"event":"wing_empty_confirmed"}),
            json!({"event":"hub_ready"}),
        ]);
    }
    cases.push(json!({"ops":ops}));
    let mut ops = prepare();
    ops.extend([
        json!({"event":"summoner_alive"}),
        json!({"event":"summoner_dead_confirmed"}),
        json!({"event":"loot_complete","keys_dropped":2,"keys_acquired":1}),
        json!({"event":"character_select"}),
    ]);
    cases.push(json!({"rooms":30,"ops":ops}));
    cases.push(json!({"ops":[{"none":true},{"event":"act1_ready"},{"event":"character_select","command_id":3},{"event":"character_select","command_id":null},{"none":true},{"event":"field_ready"},{"event":"act1_ready","command_id":99},{"event":"act1_ready"}]}));
    for extra in [
        json!({"event":"act1_ready","sequence":0}),
        json!({"event":"act1_ready","captured":10.05}),
        json!({"event":"unknown","generation":2}),
        json!({"event":"act1_ready","generation":1}),
    ] {
        cases.push(json!({"ops":[{"event":"character_select"},extra,{"event":"act1_ready","generation":2}]}));
    }
    let mut ops = prepare();
    ops.extend([
        json!({"event":"summoner_alive"}),
        json!({"event":"wing_empty_confirmed"}),
        json!({"event":"loot_complete"}),
        json!({"event":"summoner_dead_confirmed"}),
        json!({"event":"wing_empty_confirmed"}),
    ]);
    cases.push(json!({"ops":ops}));
    let expected = crate::test_support::legacy_expectation("cycle-oracle", &json!(cases))?;
    for (index, case) in cases.iter().enumerate() {
        compare(
            &exercise(case)?,
            &expected[index],
            &format!("cycle transition {index}"),
        );
    }
    Ok(())
}
#[test]
fn cycle_refusals_and_deadlines_match_python() -> TestResult {
    let mut cases = vec![];
    for op in [
        json!({"stopped":true}),
        json!({"focused":false}),
        json!({"event":"failed"}),
        json!({"event":"failed","reason":"source fault","command_id":99}),
        json!({"character":"other"}),
        json!({"room":"wrong"}),
        json!({"captured":10.2}),
        json!({"captured":9.0}),
        json!({"now":9.0}),
    ] {
        cases.push(json!({"ops":[op,{"none":true}]}));
    }
    cases.extend([json!({"ops":[{"event":"character_select"},{"none":true,"now":15.1}]}),json!({"limits":{"room_seconds":0.5},"ops":[{"event":"character_select"},{"none":true,"now":10.5}]}),json!({"limits":{"total_seconds":0.4},"ops":[{"event":"character_select"},{"none":true,"now":10.4}]}),json!({"ops":[{"event":"character_select"},{"event":"unknown","generation":2},{"event":"act1_ready","generation":1},{"event":"act1_ready","generation":2}]}),json!({"ops":[{"event":"loot_complete","keys_dropped":1,"keys_acquired":2},{"event":"unknown","keys_dropped":1},{"command_id":0}]}),json!({"room_ids":[],"ops":[]}),json!({"room_ids":["x","x"],"ops":[]}),json!({"room_ids":[" "],"ops":[]}),json!({"character":" ","ops":[]}),json!({"limits":{"stage_seconds":{}},"ops":[]})]);
    for name in ["screen_age", "room_seconds", "total_seconds"] {
        let mut p = serde_json::Map::new();
        p.insert(name.into(), json!(0));
        cases.push(json!({"limits":p,"ops":[]}));
    }
    let mut stages: BTreeMap<_, _> = CommandKind::ALL
        .into_iter()
        .map(|kind| (kind.name(), 5))
        .collect();
    stages.insert("create_room", 0);
    cases.push(json!({"limits":{"stage_seconds":stages},"ops":[]}));
    let expected = crate::test_support::legacy_expectation("cycle-oracle", &json!(cases))?;
    for (index, case) in cases.iter().enumerate() {
        compare(
            &exercise(case)?,
            &expected[index],
            &format!("cycle refusal {index}"),
        );
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            CycleLimits::new(
                value,
                100.0,
                500.0,
                CommandKind::ALL.into_iter().map(|k| (k, 5.0)).collect()
            )
            .is_err()
        );
    }
    Ok(())
}
