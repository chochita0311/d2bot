use d2bot::actions::{
    ActionError,
    town_loop::{TownOptions, TownPlanner, TownPoint, TownSight},
};
use serde_json::{Value, json};
use std::error::Error;
mod support;

#[derive(Default)]
struct Sight {
    current: Value,
}
impl TownSight for Sight {
    fn normalize(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
    fn find(&mut self, name: &str) -> Result<Option<TownPoint>, ActionError> {
        self.current["finds"]
            .get(name)
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(Into::into)
    }
    fn reset_camera(&mut self) {}
    fn camera_stable(&mut self, _: f64) -> Result<bool, ActionError> {
        Ok(self.current["stable"].as_bool().unwrap_or(true))
    }
    fn camera_reading(&self) -> Value {
        Value::Null
    }
}
fn observation(now: f64, names: &[&str]) -> Value {
    let finds = names
        .iter()
        .map(|name| ((*name).to_owned(), json!({"x":100,"y":100,"score":1.0})))
        .collect::<serde_json::Map<_, _>>();
    json!({"now":now,"finds":finds})
}
fn compare(
    options: Value,
    restore: Value,
    observations: Vec<Value>,
) -> Result<Vec<Value>, Box<dyn Error>> {
    let calibrated = [
        "area_arcane",
        "world_arcane_waypoint",
        "area_canyon",
        "world_canyon_waypoint",
    ];
    let request = json!({"kind":"town_plan","options":options,"restore":restore,"calibrated":calibrated,"observations":observations});
    let expected = support::legacy_expectation("action-oracle", &request)?;
    let options: TownOptions = serde_json::from_value(request["options"].clone())?;
    let mut planner = TownPlanner::new(options, |name| calibrated.contains(&name))?;
    planner.restore(request["restore"].as_object().ok_or("restore")?)?;
    let mut sight = Sight::default();
    let mut result = Vec::new();
    for current in observations {
        sight.current = current;
        if let Some(update) = sight.current["restore"].as_object() {
            planner.restore(update)?;
        }
        let now = sight.current["now"].as_f64().ok_or("time")?;
        let (action, error) = match planner.step(&mut sight, now) {
            Ok(action) => (serde_json::to_value(action)?, Value::Null),
            Err(error) => (Value::Null, json!(error.to_string())),
        };
        result.push(json!({"action":action,"error":error,"state":planner.data}));
    }
    equal_values(&json!(result), &expected, "");
    Ok(result)
}
fn equal_values(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equal_values(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path} keys");
            for (key, value) in a {
                assert!(b.contains_key(key), "{path}/{key} missing");
                equal_values(value, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn full_round_trip_requires_current_menus_arrival_and_character_confirmation()
-> Result<(), Box<dyn Error>> {
    let result = compare(
        json!({}),
        json!({}),
        vec![
            observation(0.0, &[]),
            observation(1.0, &["play", "difficulty_header"]),
            observation(2.0, &["play"]),
            observation(3.0, &["difficulty_header", "hell"]),
            observation(5.0, &["hud", "area_act1", "world_act1_waypoint"]),
            observation(6.0, &["waypoint_header", "list_act1"]),
            observation(7.0, &["waypoint_header", "list_act1"]),
            observation(8.0, &["waypoint_header", "list_act2"]),
            observation(9.0, &["hud", "area_act1"]),
            observation(10.0, &["hud", "area_act2"]),
            observation(11.0, &["hud", "area_act2", "world_act2_waypoint"]),
            observation(12.0, &["waypoint_header", "list_act2"]),
            observation(13.0, &["waypoint_header", "list_act1"]),
            observation(14.0, &["hud", "area_act2"]),
            observation(15.0, &["hud", "area_act1"]),
            observation(16.0, &["save_exit"]),
            observation(17.0, &[]),
            observation(18.0, &["play"]),
            observation(19.0, &["play"]),
        ],
    )?;
    assert_eq!(
        result.last().ok_or("missing result")?["state"]["phase"],
        "done"
    );
    Ok(())
}
#[test]
fn difficulty_preparation_is_not_counted_as_a_completed_run() -> Result<(), Box<dyn Error>> {
    compare(
        json!({"destination":"arcane","arrival_only":true,"reset_act2_start":true}),
        json!({"phase":"act1_spawn","started":0,"deadline":30}),
        vec![
            observation(0.0, &["hud", "area_act2"]),
            observation(0.2, &["save_exit"]),
            observation(0.4, &["play"]),
            observation(0.6, &["difficulty_header", "nightmare"]),
            observation(2.1, &["hud", "area_act1"]),
            observation(2.3, &["save_exit"]),
            observation(2.5, &["play"]),
            observation(2.7, &["difficulty_header", "hell"]),
            observation(4.2, &["hud", "area_act1", "world_act1_waypoint"]),
        ],
    )?;
    compare(
        json!({}),
        json!({"phase":"act1_spawn","started":0,"deadline":30,"reset_stage":"restored_hell","difficulty_reset_used":true}),
        vec![observation(1.0, &["hud", "area_act2"])],
    )?;
    Ok(())
}
#[test]
fn held_movement_release_camera_deadline_and_npc_recovery_are_bounded() -> Result<(), Box<dyn Error>>
{
    let mut unstable = observation(2.6, &["hud", "area_act1"]);
    unstable["stable"] = json!(false);
    let mut timeout = observation(6.1, &["hud", "area_act1"]);
    timeout["stable"] = json!(false);
    compare(
        json!({}),
        json!({"phase":"act1_waypoint","started":0,"deadline":30}),
        vec![
            observation(0.0, &["hud", "area_act1"]),
            observation(0.5, &["hud", "area_act1"]),
            observation(2.0, &["hud", "area_act1", "npc_cancel"]),
            unstable,
            timeout,
        ],
    )?;
    compare(
        json!({}),
        json!({"phase":"act2_spawn","started":0,"deadline":30,"town_layout":"act2_spawn"}),
        vec![
            observation(0.0, &["hud", "area_act2"]),
            observation(2.0, &["hud", "area_act2"]),
            observation(2.2, &["hud", "area_act2"]),
            observation(4.2, &["hud", "area_act2"]),
            observation(4.4, &["hud", "area_act2"]),
            observation(7.0, &["hud", "area_act2"]),
            observation(7.2, &["hud", "area_act2"]),
        ],
    )?;
    compare(
        json!({}),
        json!({"phase":"act2_spawn","started":0,"deadline":30}),
        vec![
            observation(0.0, &["hud", "area_act2", "npc_cancel"]),
            observation(0.3, &["hud", "area_act2", "npc_cancel"]),
            observation(0.6, &["hud", "area_act2", "npc_cancel"]),
            observation(0.9, &["hud", "area_act2", "npc_cancel"]),
        ],
    )?;
    Ok(())
}
#[test]
fn stalled_waypoints_allow_one_side_specific_detour_and_use_fresh_points()
-> Result<(), Box<dyn Error>> {
    for x in [100.5, 1100.5] {
        let mut first = observation(0.0, &["hud", "area_act1", "world_act1_waypoint"]);
        first["finds"]["world_act1_waypoint"] = json!({"x":x,"y":300.5});
        let mut next = first.clone();
        next["now"] = json!(2.1);
        let mut third = first.clone();
        third["now"] = json!(4.2);
        let mut moving = first.clone();
        moving["now"] = json!(4.5);
        let mut release = first.clone();
        release["now"] = json!(5.3);
        let mut after = first.clone();
        after["now"] = json!(5.5);
        compare(
            json!({}),
            json!({"phase":"act1_waypoint","started":0,"deadline":30}),
            vec![first, next, third, moving, release, after],
        )?;
    }
    Ok(())
}
#[test]
fn arcane_hidden_map_requires_native_proof_and_keeps_original_deadline()
-> Result<(), Box<dyn Error>> {
    let options = json!({"destination":"arcane","inspect_arcane_map":true});
    let restore = json!({"phase":"destination_arrival","started":0,"deadline":25});
    let hidden = observation(0.0, &["hud", "area_arcane"]);
    let restore_on = observation(1.5, &["hud"]);
    let on = observation(2.0, &["hud", "area_arcane", "world_arcane_waypoint"]);
    let mut native = on.clone();
    native["now"] = json!(2.5);
    native["finds"]["world_arcane_waypoint"]["evidence"] = json!("current_native_hub");
    let mut returned = native.clone();
    returned["now"] = json!(3.3);
    compare(
        options.clone(),
        restore.clone(),
        vec![
            hidden.clone(),
            restore_on.clone(),
            on,
            native,
            returned,
            observation(4.2, &["waypoint_header", "list_act2"]),
        ],
    )?;
    compare(
        options.clone(),
        restore.clone(),
        vec![
            hidden.clone(),
            restore_on,
            observation(7.5, &["hud", "area_arcane"]),
        ],
    )?;
    compare(options, restore, vec![hidden, observation(1.5, &[])])?;
    compare(
        json!({"destination":"arcane","arrival_only":true}),
        json!({"phase":"destination_arrival"}),
        vec![
            observation(0.0, &["hud", "area_arcane"]),
            observation(0.2, &["hud", "area_arcane", "world_arcane_waypoint"]),
            observation(0.4, &["hud", "area_arcane"]),
        ],
    )?;
    Ok(())
}
#[test]
fn strict_timeout_boundaries_unknown_screens_and_option_validation() -> Result<(), Box<dyn Error>> {
    compare(
        json!({}),
        json!({}),
        vec![
            observation(0.0, &["hud"]),
            observation(25.0, &["hud"]),
            observation(25.01, &["hud"]),
        ],
    )?;
    compare(
        json!({}),
        json!({"started":0,"deadline":1000}),
        vec![observation(900.0, &[]), observation(900.01, &[])],
    )?;
    for options in [
        json!({"repeat_count":0}),
        json!({"repeat_count":4}),
        json!({"arrival_only":true}),
        json!({"reset_act2_start":true,"difficulty":"normal"}),
        json!({"inspect_arcane_map":true}),
    ] {
        assert!(TownPlanner::new(serde_json::from_value(options)?, |_| true).is_err());
    }
    assert!(
        TownPlanner::new(
            TownOptions {
                destination: d2bot::actions::town_loop::Destination::Arcane,
                ..TownOptions::default()
            },
            |_| false
        )
        .is_err()
    );
    let mut planner = TownPlanner::new(TownOptions::default(), |_| true)?;
    let patch = json!({"camera_reading":null,"custom_marker":{"name":"retained"}});
    planner.restore(patch.as_object().ok_or("patch")?)?;
    let saved = serde_json::to_value(&planner.data)?;
    assert!(
        saved
            .as_object()
            .ok_or("state")?
            .contains_key("camera_reading")
    );
    assert_eq!(saved["camera_reading"], Value::Null);
    assert_eq!(saved["custom_marker"], patch["custom_marker"]);
    planner.restore(saved.as_object().ok_or("snapshot")?)?;
    assert_eq!(serde_json::to_value(&planner.data)?, saved);
    Ok(())
}
